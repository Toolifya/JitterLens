//! Test execution orchestration (subprocess, timeouts, iteration control).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

const MAX_CAPTURE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IterationStatus {
    Completed,
    TimedOut,
    FailedToStart,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationOutcome {
    pub iteration: u32,
    pub status: IterationStatus,
    pub exit_code: Option<i32>,
    pub junit_xml_path: PathBuf,
    pub junit_xml_exists: bool,
    pub stdout: String,
    pub stderr: String,
    pub message: Option<String>,
}

fn read_capped<R: Read>(mut r: R, max: usize) -> String {
    let mut buf = Vec::new();
    let _ = r.by_ref().take(max as u64).read_to_end(&mut buf);
    // Drain remainder to avoid blocking writers? Best-effort; thread will exit.
    let mut sink = std::io::sink();
    let _ = std::io::copy(&mut r, &mut sink);
    String::from_utf8_lossy(&buf).to_string()
}

/// Execute a single pytest iteration, writing JUnit XML to `junit_xml_path`.
pub fn run_iteration(
    iteration: u32,
    project_root: &Path,
    junit_xml_path: &Path,
    timeout: Option<Duration>,
    mut command: Command,
) -> IterationOutcome {
    command
        .current_dir(project_root)
        .arg(format!("--junitxml={}", junit_xml_path.display()))
        .arg(".")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    tracing::debug!(iteration, cmd = ?command, "spawning pytest");

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return IterationOutcome {
                iteration,
                status: IterationStatus::FailedToStart,
                exit_code: None,
                junit_xml_path: junit_xml_path.to_path_buf(),
                junit_xml_exists: junit_xml_path.is_file(),
                stdout: String::new(),
                stderr: String::new(),
                message: Some(format!("failed to start pytest: {e}")),
            }
        }
    };

    let stdout_handle = child.stdout.take().map(|s| {
        std::thread::spawn(move || read_capped(s, MAX_CAPTURE_BYTES))
    });
    let stderr_handle = child.stderr.take().map(|s| {
        std::thread::spawn(move || read_capped(s, MAX_CAPTURE_BYTES))
    });

    let (status, exit_code, message) = match timeout {
        Some(dur) => match child.wait_timeout(dur) {
            Ok(Some(status)) => (IterationStatus::Completed, status.code(), None),
            Ok(None) => {
                let _ = child.kill();
                let status = child.wait();
                let code = status.ok().and_then(|s| s.code());
                (
                    IterationStatus::TimedOut,
                    code,
                    Some(format!("timed out after {}s", dur.as_secs())),
                )
            }
            Err(e) => (
                IterationStatus::Interrupted,
                None,
                Some(format!("error waiting for pytest: {e}")),
            ),
        },
        None => match child.wait() {
            Ok(status) => (IterationStatus::Completed, status.code(), None),
            Err(e) => (
                IterationStatus::Interrupted,
                None,
                Some(format!("error waiting for pytest: {e}")),
            ),
        },
    };

    let stdout = stdout_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let stderr = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    IterationOutcome {
        iteration,
        status,
        exit_code,
        junit_xml_path: junit_xml_path.to_path_buf(),
        junit_xml_exists: junit_xml_path.is_file(),
        stdout,
        stderr,
        message,
    }
}

/// Execute `runs` iterations sequentially.
/// `command_factory` receives the 1-based iteration number.
pub fn run_all(
    runs: u32,
    project_root: &Path,
    session_dir: &Path,
    timeout: Option<Duration>,
    command_factory: impl Fn(u32) -> Command,
    mut on_progress: impl FnMut(&IterationOutcome),
) -> Result<Vec<IterationOutcome>> {
    anyhow::ensure!(runs > 0, "--runs must be greater than 0");
    std::fs::create_dir_all(session_dir).context("failed to create session directory")?;
    let mut outcomes = Vec::with_capacity(runs as usize);
    for i in 1..=runs {
        let xml_path = crate::utils::iteration_junit_path(session_dir, i);
        // Never overwrite an existing iteration report.
        anyhow::ensure!(
            !xml_path.exists(),
            "refusing to overwrite existing report: {}",
            xml_path.display()
        );
        let mut command = command_factory(i);
        command.env("JITTERLENS_ITERATION", i.to_string());
        command.env("JITTERLENS_TOTAL_RUNS", runs.to_string());
        let outcome = run_iteration(i, project_root, &xml_path, timeout, command);
        if outcome.status == IterationStatus::FailedToStart {
            tracing::warn!(iteration = i, message = ?outcome.message, "iteration failed to start");
        }
        on_progress(&outcome);
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iteration_status_serde() {
        let s = serde_json::to_string(&IterationStatus::TimedOut).expect("json");
        assert!(s.contains("TimedOut"));
    }
}
