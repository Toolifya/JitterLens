use anyhow::{Context, Result};
use clap::Parser;
use jitterlens::adapters::pytest;
use jitterlens::cli::{Cli, Commands};
use jitterlens::detector::{self, IterationInput, RunQuality};
use jitterlens::parser::junit;
use jitterlens::reporting;
use jitterlens::runner::{self, IterationStatus};
use jitterlens::storage;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

fn init_tracing(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Commands::Scan(args) => {
            let started_at_ms = storage::now_ms();
            println!("JitterLens");
            println!("----------\n");
            let project_path = args
                .path
                .canonicalize()
                .with_context(|| format!("project path does not exist: {}", args.path.display()))?;

            println!("Analyzing project...");
            let project = pytest::detect(&project_path).with_context(|| {
                format!(
                    "no pytest project detected at {} (looked for pytest.ini, pyproject.toml, conftest.py, tests/, etc.)",
                    project_path.display()
                )
            })?;
            println!("Detected framework: pytest");

            let pytest_version = pytest::verify_available().context(
                "pytest could not be executed; install pytest or set JITTERLENS_PYTEST_CMD",
            )?;
            println!("Using: {}", pytest_version.lines().next().unwrap_or("pytest"));

            let base_output = args
                .output
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()).join("jitterlens-artifacts"));
            let session_dir = jitterlens::utils::create_unique_session_dir(&base_output, "session")
                .context("failed to create a unique artifact session directory")?;
            println!("Artifacts: {}\n", session_dir.display());

            println!("Running test suite: {} iteration(s)\n", args.runs);
            let timeout = args.timeout.map(Duration::from_secs);
            let outcomes = runner::run_all(
                args.runs,
                &project.root,
                &session_dir,
                timeout,
                |_iter| pytest::base_command(),
                |outcome| {
                    let label = match outcome.status {
                        IterationStatus::Completed => {
                            if outcome.exit_code == Some(0) {
                                "Completed (exit 0)".to_string()
                            } else {
                                format!("Completed (exit {:?})", outcome.exit_code)
                            }
                        }
                        IterationStatus::TimedOut => "Timed out".to_string(),
                        IterationStatus::FailedToStart => "Failed to start".to_string(),
                        IterationStatus::Interrupted => "Interrupted".to_string(),
                    };
                    let xml = if outcome.junit_xml_exists { "xml: yes" } else { "xml: no" };
                    println!("[{}/{}] {} ({})", outcome.iteration, args.runs, label, xml);
                    if let Some(msg) = &outcome.message {
                        println!("         {}", msg);
                    }
                },
            )?;

            let mut inputs = Vec::with_capacity(outcomes.len());
            let mut parsed_results: Vec<Vec<jitterlens::core::TestCaseResult>> =
                Vec::with_capacity(outcomes.len());
            for o in &outcomes {
                let (cases, quality) = match o.status {
                    IterationStatus::TimedOut => (Vec::new(), RunQuality::TimedOut),
                    IterationStatus::FailedToStart => (Vec::new(), RunQuality::FailedToStart),
                    IterationStatus::Interrupted => (Vec::new(), RunQuality::Interrupted),
                    IterationStatus::Completed => {
                        if !o.junit_xml_exists {
                            (Vec::new(), RunQuality::NoXml)
                        } else {
                            match junit::parse_file(&o.junit_xml_path) {
                                Ok(parsed) => {
                                    let quality = if parsed.complete && parsed.issues.is_empty() {
                                        RunQuality::Ok
                                    } else {
                                        if !parsed.issues.is_empty() {
                                            tracing::debug!(
                                                iteration = o.iteration,
                                                issues = ?parsed.issues,
                                                "JUnit parse issues"
                                            );
                                        }
                                        RunQuality::IncompleteParse
                                    };
                                    (parsed.cases, quality)
                                }
                                Err(e) => {
                                    tracing::warn!(iteration = o.iteration, error = %e, "failed to read/parse JUnit XML");
                                    (Vec::new(), RunQuality::NoXml)
                                }
                            }
                        }
                    }
                };
                parsed_results.push(cases.clone());
                inputs.push(IterationInput {
                    iteration: o.iteration,
                    quality,
                    cases,
                });
            }

            let report = detector::analyze(&inputs);
            println!("\nAnalysis complete.\n");
            println!("Results:");
            println!("  Tests analyzed: {}", report.summary.tests_seen);
            println!("  Stable passing tests: {}", report.summary.stable_pass);
            println!("  Stable failing tests: {}", report.summary.stable_fail);
            println!("  Potentially flaky tests: {}", report.summary.flaky_suspected);
            println!("  Inconclusive tests: {}", report.summary.inconclusive);

            let mut flagged: Vec<_> = report
                .tests
                .iter()
                .filter(|t| {
                    matches!(
                        t.classification,
                        jitterlens::core::Classification::FlakySuspected
                            | jitterlens::core::Classification::StableFail
                            | jitterlens::core::Classification::Inconclusive
                    )
                })
                .collect();
            flagged.sort_by_key(|t| match t.classification {
                jitterlens::core::Classification::FlakySuspected => 0,
                jitterlens::core::Classification::StableFail => 1,
                jitterlens::core::Classification::Inconclusive => 2,
                jitterlens::core::Classification::StablePass => 3,
            });
            if !flagged.is_empty() {
                println!("\nDetails (non-stable-pass):");
                for t in flagged.iter().take(20) {
                    println!("  - {:?} {}", t.classification, t.id.display_name());
                    for r in &t.reasons {
                        println!("      {}", r);
                    }
                }
                if flagged.len() > 20 {
                    println!("  ... and {} more", flagged.len() - 20);
                }
            }

            if args.no_history {
                println!("\nHistory: not persisted (--no-history)");
            } else {
                let db_path = base_output.join("history.sqlite");
                match storage::Storage::open(&db_path) {
                    Ok(mut db) => match db.save_session(&storage::SessionRecord {
                        project_path: &project.root,
                        started_at_ms,
                        runs_requested: args.runs,
                        framework: "pytest",
                        output_dir: Some(session_dir.as_path()),
                        outcomes: &outcomes,
                        parsed_results: &parsed_results,
                        report: &report,
                    }) {
                        Ok(sid) => {
                            println!("\nHistory: saved session {} to {}", sid, db_path.display());
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "failed to save session history");
                            println!("\nHistory: failed to save ({})", e);
                        }
                    },
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to open history database");
                        println!("\nHistory: failed to open database ({})", e);
                    }
                }
            }
            match reporting::generate(
                &session_dir,
                &project.root,
                &outcomes,
                &parsed_results,
                &report,
            ) {
                Ok(path) => {
                    println!("\nHTML report generated:\n  {}", path.display());
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to generate HTML report");
                    println!("\nHTML report: failed to generate ({})", e);
                }
            }
            println!("Artifacts kept at {}", session_dir.display());
        }
    }

    Ok(())
}
