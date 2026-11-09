//! Shared domain types.

use serde::{Deserialize, Serialize};

/// Classification of a test's behavior across repeated executions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Classification {
    StablePass,
    StableFail,
    FlakySuspected,
    Inconclusive,
}

/// Outcome of a single test case execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestStatus {
    Passed,
    Failed,
    Error,
    Skipped,
}

/// Stable identity for correlating the same test across iterations.
/// Uses the most specific components available from JUnit XML.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TestId {
    pub file: Option<String>,
    pub classname: String,
    pub name: String,
}

impl TestId {
    pub fn new(
        file: Option<impl Into<String>>,
        classname: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            file: file.map(Into::into).filter(|s| !s.is_empty()),
            classname: classname.into(),
            name: name.into(),
        }
    }

    /// Canonical key for grouping results across runs.
    pub fn canonical(&self) -> String {
        match &self.file {
            Some(f) if !f.is_empty() => format!("{}::{}::{}", f, self.classname, self.name),
            _ if !self.classname.is_empty() => format!("{}::{}", self.classname, self.name),
            _ => self.name.clone(),
        }
    }

    pub fn display_name(&self) -> String {
        if self.classname.is_empty() {
            self.name.clone()
        } else {
            format!("{}::{}", self.classname, self.name)
        }
    }
}

/// Normalized result for one test case in one iteration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestCaseResult {
    pub id: TestId,
    pub status: TestStatus,
    pub duration_secs: Option<f64>,
    pub message: Option<String>,
    pub details: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifications_are_distinct() {
        assert_ne!(Classification::StablePass, Classification::StableFail);
        assert_ne!(Classification::FlakySuspected, Classification::Inconclusive);
    }

    #[test]
    fn test_id_canonical_prefers_file() {
        let id = TestId::new(Some("tests/test_a.py"), "test_a.TestC", "test_x");
        assert_eq!(id.canonical(), "tests/test_a.py::test_a.TestC::test_x");
    }

    #[test]
    fn test_id_canonical_without_file() {
        let id = TestId::new(None::<String>, "test_ok", "test_always_passes");
        assert_eq!(id.canonical(), "test_ok::test_always_passes");
    }
}
