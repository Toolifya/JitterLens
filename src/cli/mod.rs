use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Local-first flaky test detector for pytest projects.
#[derive(Debug, Parser)]
#[command(name = "jitterlens", version, about)]
#[command(
    long_about = "JitterLens detects potentially flaky tests by running test suites repeatedly and \
analyzing consistency. It runs locally (no cloud/AI). SECURITY: scanning executes project tests \
(arbitrary code); only scan projects you trust."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose logging.
    #[arg(long, global = true)]
    pub verbose: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Scan a project by running its tests repeatedly and analyzing results.
    Scan(ScanArgs),
}

#[derive(Debug, clap::Args, Clone)]
pub struct ScanArgs {
    /// Path to the project to scan.
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Number of test suite iterations to run (must be >= 1).
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..))]
    pub runs: u32,

    /// Per-run timeout in seconds. Omit for no timeout.
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    pub timeout: Option<u64>,

    /// Directory where reports/artifacts are written.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Do not persist results to local history (SQLite).
    #[arg(long)]
    pub no_history: bool,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn parses_scan_defaults() {
        let cli = Cli::try_parse_from(["jitterlens", "scan"]).expect("parse");
        match cli.command {
            super::Commands::Scan(args) => {
                assert_eq!(args.path.to_string_lossy(), ".");
                assert_eq!(args.runs, 10);
                assert!(args.timeout.is_none());
                assert!(args.output.is_none());
                assert!(!args.no_history);
            }
        }
        assert!(!cli.verbose);
    }

    #[test]
    fn parses_scan_options() {
        let cli = Cli::try_parse_from([
            "jitterlens",
            "--verbose",
            "scan",
            "./my-project",
            "--runs",
            "30",
            "--timeout",
            "120",
            "--output",
            "out",
            "--no-history",
        ])
        .expect("parse");
        assert!(cli.verbose);
        match cli.command {
            super::Commands::Scan(args) => {
                assert_eq!(args.path.to_string_lossy(), "./my-project");
                assert_eq!(args.runs, 30);
                assert_eq!(args.timeout, Some(120));
                assert_eq!(args.output.unwrap().to_string_lossy(), "out");
                assert!(args.no_history);
            }
        }
    }

    #[test]
    fn rejects_zero_runs() {
        let err = Cli::try_parse_from(["jitterlens", "scan", "--runs", "0"]);
        assert!(err.is_err());
    }

    #[test]
    fn rejects_zero_timeout() {
        let err = Cli::try_parse_from(["jitterlens", "scan", "--timeout", "0"]);
        assert!(err.is_err());
    }
}
