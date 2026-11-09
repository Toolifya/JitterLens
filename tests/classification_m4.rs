use jitterlens::adapters::pytest;
use jitterlens::core::Classification;
use jitterlens::detector::{self, IterationInput, RunQuality};
use jitterlens::parser::junit;
use jitterlens::runner::{self, IterationStatus};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn pytest_cmd(_iter: u32) -> Command {
    if let Ok(cmd) = std::env::var("JITTERLENS_PYTEST_CMD") {
        return pytest::command_from_parts(&cmd);
    }
    let venv_python = PathBuf::from("/tmp/opencode/jitterlens-test-venv/bin/python");
    if venv_python.is_file() {
        let mut c = Command::new(venv_python);
        c.args(["-m", "pytest"]);
        return c;
    }
    pytest::default_command()
}

fn require_pytest() {
    pytest::verify_command(pytest_cmd(1)).expect(
        "pytest is required for classification integration tests; install pytest, \
         set JITTERLENS_PYTEST_CMD, or provide /tmp/opencode/jitterlens-test-venv",
    );
}

fn run_quality(outcome: &runner::IterationOutcome, parsed_complete: bool) -> RunQuality {
    match outcome.status {
        IterationStatus::TimedOut => RunQuality::TimedOut,
        IterationStatus::FailedToStart => RunQuality::FailedToStart,
        IterationStatus::Interrupted => RunQuality::Interrupted,
        IterationStatus::Completed => {
            if !outcome.junit_xml_exists {
                RunQuality::NoXml
            } else if parsed_complete {
                RunQuality::Ok
            } else {
                RunQuality::IncompleteParse
            }
        }
    }
}

fn analyze_project(project: &Path, runs: u32, timeout: Option<Duration>) -> detector::DetectionReport {
    let out_base = tempfile::tempdir().expect("tmp");
    let session_dir = jitterlens::utils::create_unique_session_dir(out_base.path(), "session")
        .expect("session dir");
    let outcomes = runner::run_all(runs, project, &session_dir, timeout, pytest_cmd, |_o| {})
        .expect("run_all");
    let mut inputs = Vec::new();
    for o in outcomes {
        let (cases, complete) = if o.junit_xml_exists {
            match junit::parse_file(&o.junit_xml_path) {
                Ok(p) => (p.cases, p.complete && p.issues.is_empty()),
                Err(_) => (Vec::new(), false),
            }
        } else {
            (Vec::new(), false)
        };
        inputs.push(IterationInput {
            iteration: o.iteration,
            quality: run_quality(&o, complete),
            cases,
        });
    }
    detector::analyze(&inputs)
}

fn find_by_name<'a>(
    report: &'a detector::DetectionReport,
    name_contains: &str,
) -> &'a detector::ClassifiedTest {
    report
        .tests
        .iter()
        .find(|t| t.id.canonical().contains(name_contains) || t.id.display_name().contains(name_contains))
        .unwrap_or_else(|| {
            panic!(
                "test containing {:?} not found in {:?}",
                name_contains,
                report.tests.iter().map(|t| t.id.canonical()).collect::<Vec<_>>()
            )
        })
}

#[test]
fn always_passing_is_stable_pass() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/always_pass");
    let report = analyze_project(&project, 5, None);
    let t = find_by_name(&report, "test_always_passes");
    assert_eq!(t.classification, Classification::StablePass);
}

#[test]
fn always_failing_is_stable_fail() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/always_fail");
    let report = analyze_project(&project, 5, None);
    let t = find_by_name(&report, "test_always_fails");
    assert_eq!(t.classification, Classification::StableFail);
}

#[test]
fn deterministically_intermittent_is_flaky() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/intermittent_deterministic");
    let report = analyze_project(&project, 6, None);
    let t = find_by_name(&report, "test_intermittent");
    assert_eq!(t.classification, Classification::FlakySuspected);
    assert!(t.metrics.passed >= 1 && t.metrics.failed >= 1);
}

#[test]
fn random_like_seeded_is_flaky() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/intermittent_random_seeded");
    let report = analyze_project(&project, 6, None);
    let t = find_by_name(&report, "test_random_like");
    assert_eq!(t.classification, Classification::FlakySuspected);
}

#[test]
fn skipped_test_is_inconclusive() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/skipped");
    let report = analyze_project(&project, 3, None);
    let t = find_by_name(&report, "test_always_skipped");
    assert_eq!(t.classification, Classification::Inconclusive);
}

#[test]
fn timeout_run_produces_inconclusive_evidence() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/timeout_case");
    let report = analyze_project(&project, 1, Some(Duration::from_secs(1)));
    // With a timed-out run and no reliable per-test result, test may not be observed.
    // The system must not claim stable pass/fail without evidence.
    for t in &report.tests {
        assert_ne!(t.classification, Classification::StablePass);
        assert_ne!(t.classification, Classification::StableFail);
    }
}

#[test]
fn duplicate_names_in_different_files_are_distinguished() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/duplicate_names");
    let report = analyze_project(&project, 2, None);
    let dups: Vec<_> = report
        .tests
        .iter()
        .filter(|t| t.id.name == "test_dup")
        .collect();
    assert!(dups.len() >= 2, "expected >=2 test_dup entries, got {:?}", report.tests.iter().map(|t| t.id.canonical()).collect::<Vec<_>>());
    let keys: std::collections::HashSet<_> = dups.iter().map(|t| t.id.canonical()).collect();
    assert_eq!(keys.len(), dups.len(), "canonical IDs must be distinct");
    for t in dups {
        assert_eq!(t.classification, Classification::StablePass);
    }
}

#[test]
fn disappearing_test_is_detected() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/disappearing");
    let report = analyze_project(&project, 3, None);
    let stable = find_by_name(&report, "test_stays");
    assert_eq!(stable.classification, Classification::StablePass);
    let flaky = find_by_name(&report, "test_disappears");
    assert_eq!(flaky.classification, Classification::FlakySuspected);
    assert!(flaky.metrics.missing >= 1, "should be missing in at least one complete run");
}

#[test]
fn malformed_xml_scenario_is_handled() {
    // Unit-level validation through detector: incomplete parse makes absent tests unknown.
    let inputs = vec![
        IterationInput { iteration: 1, quality: RunQuality::Ok, cases: vec![jitterlens::core::TestCaseResult {
            id: jitterlens::core::TestId::new(None::<String>, "m", "x"),
            status: jitterlens::core::TestStatus::Passed,
            duration_secs: None, message: None, details: None,
        }]},
        IterationInput { iteration: 2, quality: RunQuality::IncompleteParse, cases: vec![] },
    ];
    let report = detector::analyze(&inputs);
    assert_eq!(report.tests[0].classification, Classification::Inconclusive);
}

#[test]
fn interrupted_run_scenario_is_handled() {
    let inputs = vec![
        IterationInput { iteration: 1, quality: RunQuality::Ok, cases: vec![jitterlens::core::TestCaseResult {
            id: jitterlens::core::TestId::new(None::<String>, "m", "x"),
            status: jitterlens::core::TestStatus::Passed,
            duration_secs: None, message: None, details: None,
        }]},
        IterationInput { iteration: 2, quality: RunQuality::Interrupted, cases: vec![] },
    ];
    let report = detector::analyze(&inputs);
    assert_eq!(report.tests[0].classification, Classification::Inconclusive);
}
