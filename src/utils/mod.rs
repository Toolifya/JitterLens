//! Small helpers (paths, time, safe file naming).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Create a unique directory under `base` for a scan session.
/// Does not overwrite existing directories; appends a counter if needed.
pub fn create_unique_session_dir(base: &Path, prefix: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(base)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let pid = std::process::id();
    for attempt in 0u32..1000 {
        let name = if attempt == 0 {
            format!("{prefix}-{ts}-{pid}")
        } else {
            format!("{prefix}-{ts}-{pid}-{attempt}")
        };
        let path = base.join(name);
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not create a unique session directory",
    ))
}

/// Return a safe per-iteration JUnit XML path inside `session_dir`.
pub fn iteration_junit_path(session_dir: &Path, iteration: u32) -> PathBuf {
    session_dir.join(format!("run-{iteration:04}.xml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_unique_session_dir() {
        let base = tempfile::tempdir().expect("tmp");
        let a = create_unique_session_dir(base.path(), "sess").expect("a");
        let b = create_unique_session_dir(base.path(), "sess").expect("b");
        assert!(a.exists());
        assert!(b.exists());
        assert_ne!(a, b);
    }

    #[test]
    fn iteration_path_is_stable() {
        let base = tempfile::tempdir().expect("tmp");
        let p = iteration_junit_path(base.path(), 7);
        assert!(p.ends_with("run-0007.xml"));
    }
}
