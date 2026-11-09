use jitterlens::adapters::pytest;
use jitterlens::parser::junit;
use jitterlens::runner::{self, IterationStatus};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn make_pytest_cmd(_iter: u32) -> Command {
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
    pytest::verify_command(make_pytest_cmd(1)).expect(
        "pytest is required for execution integration tests; install pytest, \
         set JITTERLENS_PYTEST_CMD, or provide /tmp/opencode/jitterlens-test-venv",
    );
}

#[test]
fn runs_iterations_and_generates_separate_junit_xml() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/always_pass");
    assert!(pytest::detect(&project).is_some(), "fixture must be detected");

    let out_base = tempfile::tempdir().expect("tmp");
    let session_dir = jitterlens::utils::create_unique_session_dir(out_base.path(), "session")
        .expect("session dir");

    let outcomes = runner::run_all(2, &project, &session_dir, None, make_pytest_cmd, |_o| {})
        .expect("run_all");

    assert_eq!(outcomes.len(), 2);
    for (i, o) in outcomes.iter().enumerate() {
        assert_eq!(o.iteration as usize, i + 1);
        assert_eq!(o.status, IterationStatus::Completed);
        assert_eq!(o.exit_code, Some(0));
        assert!(
            o.junit_xml_exists,
            "JUnit XML should exist for iteration {}",
            o.iteration
        );
        assert!(o.junit_xml_path.is_file());
        assert!(o.junit_xml_path.starts_with(&session_dir));
    }
    assert_ne!(outcomes[0].junit_xml_path, outcomes[1].junit_xml_path);
}

#[test]
fn timeout_is_reported() {
    require_pytest();
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::write(
        dir.path().join("test_slow.py"),
        "import time\n\ndef test_slow():\n    time.sleep(5)\n    assert True\n",
    )
    .expect("write");

    let out_base = tempfile::tempdir().expect("tmp");
    let session_dir = jitterlens::utils::create_unique_session_dir(out_base.path(), "session")
        .expect("session dir");

    let outcomes = runner::run_all(
        1,
        dir.path(),
        &session_dir,
        Some(Duration::from_secs(1)),
        make_pytest_cmd,
        |_o| {},
    )
    .expect("run_all");

    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].status, IterationStatus::TimedOut);
    assert!(outcomes[0]
        .message
        .as_deref()
        .unwrap_or("")
        .contains("timed out"));
}

#[test]
fn failing_tests_still_produce_execution_record() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/always_fail");
    let out_base = tempfile::tempdir().expect("tmp");
    let session_dir = jitterlens::utils::create_unique_session_dir(out_base.path(), "session")
        .expect("session dir");
    let outcomes = runner::run_all(1, &project, &session_dir, None, make_pytest_cmd, |_o| {})
        .expect("run_all");
    assert_eq!(outcomes[0].status, IterationStatus::Completed);
    assert_ne!(
        outcomes[0].exit_code,
        Some(0),
        "failing tests should yield non-zero pytest exit"
    );
    assert!(
        outcomes[0].junit_xml_exists,
        "JUnit XML should exist even when tests fail"
    );
}

#[test]
fn generated_junit_xml_is_parseable() {
    require_pytest();
    let project = manifest_dir().join("tests/synthetic_projects/always_pass");
    let out_base = tempfile::tempdir().expect("tmp");
    let session_dir = jitterlens::utils::create_unique_session_dir(out_base.path(), "session")
        .expect("session dir");
    let outcomes = runner::run_all(1, &project, &session_dir, None, make_pytest_cmd, |_o| {})
        .expect("run_all");
    let parsed = junit::parse_file(&outcomes[0].junit_xml_path).expect("read xml");
    assert!(parsed.complete, "issues: {:?}", parsed.issues);
    assert_eq!(parsed.cases.len(), 1);
    assert_eq!(
        parsed.cases[0].id.display_name(),
        "test_ok::test_always_passes"
    );
}
