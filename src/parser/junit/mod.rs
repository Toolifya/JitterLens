//! JUnit XML parsing into normalized test results.

use crate::core::{TestCaseResult, TestId, TestStatus};
use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::path::Path;

/// Result of parsing a JUnit XML file.
#[derive(Debug, Clone, Default)]
pub struct ParsedRun {
    pub cases: Vec<TestCaseResult>,
    /// Non-fatal issues encountered while parsing (malformed fragments, missing fields, etc.).
    pub issues: Vec<String>,
    /// False when the XML could not be fully/validly parsed.
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutcomeKind {
    Failure,
    Error,
    Skipped,
}

fn attr_string(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    for a in e.attributes().flatten() {
        if a.key.as_ref() == key {
            return a.unescape_value().ok().map(|v| v.into_owned());
        }
    }
    None
}

fn attr_f64(e: &BytesStart<'_>, key: &[u8]) -> Option<f64> {
    attr_string(e, key).and_then(|s| s.parse::<f64>().ok())
}

fn parse_case_start(e: &BytesStart<'_>) -> TestCaseResult {
    let name = attr_string(e, b"name").unwrap_or_else(|| "<unnamed>".to_string());
    let classname = attr_string(e, b"classname").unwrap_or_default();
    let file = attr_string(e, b"file");
    let duration_secs = attr_f64(e, b"time");
    TestCaseResult {
        id: TestId::new(file, classname, name),
        status: TestStatus::Passed,
        duration_secs,
        message: None,
        details: None,
    }
}

fn outcome_status(kind: OutcomeKind) -> TestStatus {
    match kind {
        OutcomeKind::Failure => TestStatus::Failed,
        OutcomeKind::Error => TestStatus::Error,
        OutcomeKind::Skipped => TestStatus::Skipped,
    }
}

/// Parse JUnit XML content from bytes/string. Defensive: collects issues and returns partial results on errors.
pub fn parse_str(xml: &str) -> ParsedRun {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut run = ParsedRun {
        cases: Vec::new(),
        issues: Vec::new(),
        complete: true,
    };

    let mut current: Option<TestCaseResult> = None;
    let mut current_outcome: Option<OutcomeKind> = None;
    let mut outcome_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"testcase" => {
                        if let Some(c) = current.take() {
                            run.issues
                                .push(format!("nested or unclosed testcase before {}", c.id.canonical()));
                            run.cases.push(c);
                        }
                        current = Some(parse_case_start(&e));
                        current_outcome = None;
                        outcome_text.clear();
                    }
                    b"failure" if current.is_some() => {
                        current_outcome = Some(OutcomeKind::Failure);
                        outcome_text.clear();
                        if let Some(ref mut c) = current {
                            c.status = TestStatus::Failed;
                            c.message = attr_string(&e, b"message").or(attr_string(&e, b"type"));
                        }
                    }
                    b"error" if current.is_some() => {
                        current_outcome = Some(OutcomeKind::Error);
                        outcome_text.clear();
                        if let Some(ref mut c) = current {
                            c.status = TestStatus::Error;
                            c.message = attr_string(&e, b"message").or(attr_string(&e, b"type"));
                        }
                    }
                    b"skipped" if current.is_some() => {
                        current_outcome = Some(OutcomeKind::Skipped);
                        outcome_text.clear();
                        if let Some(ref mut c) = current {
                            c.status = TestStatus::Skipped;
                            c.message = attr_string(&e, b"message").or(attr_string(&e, b"type"));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"testcase" => {
                        run.cases.push(parse_case_start(&e));
                    }
                    b"failure" if current.is_some() => {
                        if let Some(ref mut c) = current {
                            c.status = TestStatus::Failed;
                            c.message = attr_string(&e, b"message").or(attr_string(&e, b"type"));
                        }
                    }
                    b"error" if current.is_some() => {
                        if let Some(ref mut c) = current {
                            c.status = TestStatus::Error;
                            c.message = attr_string(&e, b"message").or(attr_string(&e, b"type"));
                        }
                    }
                    b"skipped" if current.is_some() => {
                        if let Some(ref mut c) = current {
                            c.status = TestStatus::Skipped;
                            c.message = attr_string(&e, b"message").or(attr_string(&e, b"type"));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) if current_outcome.is_some() => {
                if let Ok(s) = t.unescape() {
                    if !outcome_text.is_empty() {
                        outcome_text.push('\n');
                    }
                    outcome_text.push_str(&s);
                }
            }
            Ok(Event::CData(t)) if current_outcome.is_some() => {
                if let Ok(s) = std::str::from_utf8(&t) {
                    if !outcome_text.is_empty() {
                        outcome_text.push('\n');
                    }
                    outcome_text.push_str(s);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"testcase" => {
                        if let Some(c) = current.take() {
                            if c.id.name.is_empty() || c.id.name == "<unnamed>" {
                                run.issues.push(format!(
                                    "testcase missing name (canonical key: {})",
                                    c.id.canonical()
                                ));
                            }
                            run.cases.push(c);
                        }
                        current_outcome = None;
                        outcome_text.clear();
                    }
                    b"failure" | b"error" | b"skipped" => {
                        if let Some(kind) = current_outcome.take()
                            && let Some(c) = current.as_mut()
                        {
                            c.status = outcome_status(kind);
                            let details = outcome_text.trim();
                            if !details.is_empty() {
                                // Keep details bounded.
                                const MAX: usize = 16 * 1024;
                                c.details = Some(if details.len() > MAX {
                                    let mut end = MAX;
                                    while end > 0 && !details.is_char_boundary(end) {
                                        end -= 1;
                                    }
                                    format!("{}…", &details[..end])
                                } else {
                                    details.to_string()
                                });
                            }
                        }
                        outcome_text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => {
                if current_outcome.is_some() || current.is_some() {
                    run.complete = false;
                    run.issues.push("unexpected end of XML (unclosed elements)".to_string());
                }
                if let Some(kind) = current_outcome.take()
                    && let Some(c) = current.as_mut()
                {
                    c.status = outcome_status(kind);
                    let details = outcome_text.trim();
                    if !details.is_empty() {
                        const MAX: usize = 16 * 1024;
                        c.details = Some(if details.len() > MAX {
                            let mut end = MAX;
                            while end > 0 && !details.is_char_boundary(end) {
                                end -= 1;
                            }
                            format!("{}…", &details[..end])
                        } else {
                            details.to_string()
                        });
                    }
                }
                if let Some(c) = current.take() {
                    run.cases.push(c);
                }
                break;
            }
            Err(e) => {
                run.complete = false;
                run.issues.push(format!(
                    "XML parse error at position {}: {e}",
                    reader.buffer_position()
                ));
                if let Some(kind) = current_outcome.take()
                    && let Some(c) = current.as_mut()
                {
                    c.status = outcome_status(kind);
                    let details = outcome_text.trim();
                    if !details.is_empty() {
                        c.details = Some(details.to_string());
                    }
                }
                if let Some(c) = current.take() {
                    run.cases.push(c);
                }
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    run
}

/// Parse a JUnit XML file. Missing/empty files are reported as incomplete with issues.
pub fn parse_file(path: &Path) -> Result<ParsedRun> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read JUnit XML: {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(ParsedRun {
            cases: Vec::new(),
            issues: vec![format!("JUnit XML file is empty: {}", path.display())],
            complete: false,
        });
    }
    Ok(parse_str(&content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TestStatus;

    #[test]
    fn parses_pytest_style_passing_testcase() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?><testsuites><testsuite><testcase classname="test_ok" name="test_always_passes" time="0.001" /></testsuite></testsuites>"#;
        let run = parse_str(xml);
        assert!(run.complete, "issues: {:?}", run.issues);
        assert_eq!(run.cases.len(), 1);
        assert_eq!(run.cases[0].status, TestStatus::Passed);
        assert_eq!(run.cases[0].id.canonical(), "test_ok::test_always_passes");
    }

    #[test]
    fn parses_failure() {
        let xml = r#"<testsuite><testcase classname="a" name="b" time="0.1"><failure message="boom">traceback</failure></testcase></testsuite>"#;
        let run = parse_str(xml);
        assert!(run.complete);
        assert_eq!(run.cases[0].status, TestStatus::Failed);
        assert_eq!(run.cases[0].message.as_deref(), Some("boom"));
        assert!(run.cases[0].details.as_deref().unwrap_or("").contains("traceback"));
    }

    #[test]
    fn parses_error_and_skipped() {
        let xml = r#"<testsuite>
          <testcase classname="a" name="err"><error message="e">details</error></testcase>
          <testcase classname="a" name="skip"><skipped message="s"/></testcase>
        </testsuite>"#;
        let run = parse_str(xml);
        assert_eq!(run.cases.len(), 2);
        assert_eq!(run.cases[0].status, TestStatus::Error);
        assert_eq!(run.cases[1].status, TestStatus::Skipped);
    }

    #[test]
    fn handles_malformed_xml_defensively() {
        let xml = r#"<testsuite><testcase classname="a" name="b"><failure>oops"#;
        let run = parse_str(xml);
        assert!(!run.complete, "truncated XML should be marked incomplete");
        assert!(!run.issues.is_empty());
        // Partial recovery: we should still capture the testcase when possible.
        assert_eq!(run.cases.len(), 1);
        assert_eq!(run.cases[0].status, TestStatus::Failed);
        assert!(run.cases[0].details.as_deref().unwrap_or("").contains("oops"));
    }

    #[test]
    fn handles_empty_input() {
        let run = parse_str("");
        assert!(run.cases.is_empty());
    }

    #[test]
    fn uses_file_in_identity_when_present() {
        let xml = r#"<testsuite><testcase file="tests/test_a.py" classname="test_a" name="test_x" time="0.0"/></testsuite>"#;
        let run = parse_str(xml);
        assert_eq!(
            run.cases[0].id.canonical(),
            "tests/test_a.py::test_a::test_x"
        );
    }

    #[test]
    fn distinguishes_duplicate_names_in_different_files() {
        let xml = r#"<testsuite>
          <testcase file="a/test_x.py" classname="test_x" name="test_it"/>
          <testcase file="b/test_x.py" classname="test_x" name="test_it"/>
        </testsuite>"#;
        let run = parse_str(xml);
        assert_eq!(run.cases.len(), 2);
        assert_ne!(run.cases[0].id.canonical(), run.cases[1].id.canonical());
    }

    #[test]
    fn parse_file_reports_empty_file() {
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path().join("empty.xml");
        std::fs::write(&p, "   \n").expect("write");
        let run = parse_file(&p).expect("parse_file");
        assert!(!run.complete);
        assert!(!run.issues.is_empty());
    }
}
