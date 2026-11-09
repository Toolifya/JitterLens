//! Pytest project detection and command construction.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A detected pytest-capable project.
#[derive(Debug, Clone)]
pub struct PytestProject {
    pub root: PathBuf,
}

/// Detect whether `root` looks like a pytest project.
/// This is a heuristic check for Milestone 2; it does not execute tests.
pub fn detect(root: &Path) -> Option<PytestProject> {
    let root = root.canonicalize().ok()?;
    if !root.is_dir() {
        return None;
    }
    let markers = ["pytest.ini", "pyproject.toml", "tox.ini", "conftest.py", "setup.cfg"];
    for m in markers {
        let p = root.join(m);
        if p.is_file() {
            if m == "pyproject.toml" {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    let sl = s.to_ascii_lowercase();
                    if sl.contains("[tool.pytest") || sl.contains("pytest") {
                        return Some(PytestProject { root });
                    }
                }
                // pyproject.toml exists but no obvious pytest marker: keep checking others.
                continue;
            }
            return Some(PytestProject { root });
        }
    }
    let tests_dir = root.join("tests");
    if tests_dir.is_dir() {
        return Some(PytestProject { root });
    }
    // Shallow scan for test files in root.
    if let Ok(entries) = std::fs::read_dir(&root) {
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if (name.starts_with("test_") || name.ends_with("test.py")) && name.ends_with(".py") {
                return Some(PytestProject { root });
            }
        }
    }
    None
}

/// Build the base command used to invoke pytest.
/// Supports `JITTERLENS_PYTEST_CMD` (split by whitespace) for testing/overrides.
pub fn base_command() -> Command {
    if let Ok(cmd) = std::env::var("JITTERLENS_PYTEST_CMD") {
        return command_from_parts(&cmd);
    }
    default_command()
}

/// Build a command from a whitespace-split string (e.g. from `JITTERLENS_PYTEST_CMD`).
pub fn command_from_parts(cmd: &str) -> Command {
    let mut parts = cmd.split_whitespace();
    let program = parts.next().unwrap_or("python3");
    let mut c = Command::new(program);
    for p in parts {
        c.arg(p);
    }
    if !cmd.contains("pytest") {
        c.args(["-m", "pytest"]);
    }
    c
}

/// Default pytest invocation (`python3 -m pytest`).
pub fn default_command() -> Command {
    let mut c = Command::new("python3");
    c.args(["-m", "pytest"]);
    c
}

/// Verify that the given pytest command is runnable.
pub fn verify_command(mut command: Command) -> Result<String> {
    command.arg("--version");
    let out = command.output().context("failed to execute pytest command")?;
    if out.status.success() {
        let v = String::from_utf8_lossy(&out.stdout);
        let v = v.trim();
        let v = if v.is_empty() {
            String::from_utf8_lossy(&out.stderr).trim().to_string()
        } else {
            v.to_string()
        };
        Ok(if v.is_empty() {
            "pytest (version unknown)".to_string()
        } else {
            v
        })
    } else {
        anyhow::bail!(
            "pytest is not available (exit {}). stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )
    }
}

/// Verify that the default/environment pytest command is runnable.
pub fn verify_available() -> Result<String> {
    verify_command(base_command())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_pytest_ini() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(dir.path().join("pytest.ini"), "[pytest]\n").expect("write");
        assert!(detect(dir.path()).is_some());
    }

    #[test]
    fn detects_tests_dir() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::create_dir(dir.path().join("tests")).expect("mkdir");
        assert!(detect(dir.path()).is_some());
    }

    #[test]
    fn rejects_empty_dir() {
        let dir = tempfile::tempdir().expect("tmp");
        assert!(detect(dir.path()).is_none());
    }
}
