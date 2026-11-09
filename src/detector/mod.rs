//! Aggregation and deterministic flakiness classification.
//!
//! The detector is independent from test execution and parsing I/O. It consumes
//! normalized per-iteration observations and produces explainable classifications.
//!
//! ## Classification rules (MVP, deterministic)
//! For each test identity observed across iterations:
//! - `present(pass/fail/error/skip)`: test case appeared in a parsed iteration.
//! - `missing`: iteration had a complete, usable parse but the test did not appear.
//! - `unknown`: iteration result unavailable/ambiguous for this test (no XML,
//!   incomplete parse, timeout, failed to start, interrupted).
//!
//! - **FLAKY_SUSPECTED**: inconsistent outcomes with sufficient evidence, e.g.
//!   both pass and fail/error observed, or the test appears in some complete runs
//!   and disappears in others.
//! - **STABLE_PASS**: at least one pass, no fail/error, no missing, no unknown,
//!   and no skip-only ambiguity preventing a pass conclusion.
//! - **STABLE_FAIL**: at least one fail/error, no pass, no missing, no unknown.
//! - **INCONCLUSIVE**: insufficient or ambiguous evidence (all unknown, only skips,
//!   missing/unknown ambiguity, or never observed).
//!
//! These rules are intentionally conservative and explainable; they are not
//! presented as scientifically proven.

use crate::core::{Classification, TestCaseResult, TestId, TestStatus};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunQuality {
    /// Completed with a complete, usable parse.
    Ok,
    /// Parsed but parser reported issues/incompleteness; cases may be partial.
    IncompleteParse,
    /// No usable JUnit XML/result.
    NoXml,
    TimedOut,
    FailedToStart,
    Interrupted,
}

#[derive(Debug, Clone)]
pub struct IterationInput {
    pub iteration: u32,
    pub quality: RunQuality,
    pub cases: Vec<TestCaseResult>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestMetrics {
    pub total_iterations: u32,
    pub observed: u32,
    pub passed: u32,
    pub failed: u32, // includes Error
    pub skipped: u32,
    pub missing: u32,  // absent in a complete/Ok run
    pub unknown: u32,  // run not evaluable for this test
}

impl TestMetrics {
    pub fn failure_rate(&self) -> Option<f64> {
        let effective = self.passed + self.failed;
        if effective == 0 {
            None
        } else {
            Some(self.failed as f64 / effective as f64)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassifiedTest {
    pub id: TestId,
    pub classification: Classification,
    pub metrics: TestMetrics,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionSummary {
    pub total_iterations: u32,
    pub tests_seen: u32,
    pub stable_pass: u32,
    pub stable_fail: u32,
    pub flaky_suspected: u32,
    pub inconclusive: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DetectionReport {
    pub tests: Vec<ClassifiedTest>,
    pub summary: DetectionSummary,
}

#[derive(Debug, Clone)]
struct Accum {
    id: TestId,
    passed: u32,
    failed: u32,
    skipped: u32,
    missing: u32,
    unknown: u32,
    observed: u32,
}

pub fn analyze(inputs: &[IterationInput]) -> DetectionReport {
    let total = inputs.len() as u32;
    let mut accums: HashMap<String, Accum> = HashMap::new();
    // Preserve a stable display order by canonical key.
    let mut order: BTreeMap<String, ()> = BTreeMap::new();

    // First pass: record observations per iteration and ensure tests seen in any
    // iteration are tracked.
    let mut per_iter_maps: Vec<HashMap<String, TestStatus>> = Vec::with_capacity(inputs.len());
    for input in inputs {
        let mut m = HashMap::new();
        for case in &input.cases {
            let key = case.id.canonical();
            order.entry(key.clone()).or_insert(());
            accums.entry(key.clone()).or_insert_with(|| Accum {
                id: case.id.clone(),
                passed: 0,
                failed: 0,
                skipped: 0,
                missing: 0,
                unknown: 0,
                observed: 0,
            });
            m.insert(key, case.status);
        }
        per_iter_maps.push(m);
    }

    // Second pass: classify each test across all iterations.
    for key in order.keys() {
        let accum = accums.get_mut(key).expect("present");
        for (input, iter_map) in inputs.iter().zip(per_iter_maps.iter()) {
            if let Some(status) = iter_map.get(key) {
                accum.observed += 1;
                match status {
                    TestStatus::Passed => accum.passed += 1,
                    TestStatus::Failed | TestStatus::Error => accum.failed += 1,
                    TestStatus::Skipped => accum.skipped += 1,
                }
            } else {
                match input.quality {
                    RunQuality::Ok => accum.missing += 1,
                    RunQuality::IncompleteParse
                    | RunQuality::NoXml
                    | RunQuality::TimedOut
                    | RunQuality::FailedToStart
                    | RunQuality::Interrupted => accum.unknown += 1,
                }
            }
        }
    }

    let mut tests = Vec::with_capacity(order.len());
    for (key, _) in order {
        let a = accums.remove(&key).expect("present");
        tests.push(classify_one(total, a));
    }

    let mut summary = DetectionSummary {
        total_iterations: total,
        tests_seen: tests.len() as u32,
        ..DetectionSummary::default()
    };
    for t in &tests {
        match t.classification {
            Classification::StablePass => summary.stable_pass += 1,
            Classification::StableFail => summary.stable_fail += 1,
            Classification::FlakySuspected => summary.flaky_suspected += 1,
            Classification::Inconclusive => summary.inconclusive += 1,
        }
    }

    DetectionReport { tests, summary }
}

fn classify_one(total: u32, a: Accum) -> ClassifiedTest {
    let metrics = TestMetrics {
        total_iterations: total,
        observed: a.observed,
        passed: a.passed,
        failed: a.failed,
        skipped: a.skipped,
        missing: a.missing,
        unknown: a.unknown,
    };
    let mut reasons = Vec::new();
    let effective = a.passed + a.failed;
    let has_outcome_inconsistency = a.passed >= 1 && a.failed >= 1;
    let has_discovery_inconsistency = a.missing >= 1 && a.observed >= 1;

    let classification = if a.observed == 0 {
        reasons.push("test was never observed in parsed results".to_string());
        Classification::Inconclusive
    } else if has_outcome_inconsistency {
        reasons.push(format!(
            "inconsistent outcomes: passed {} time(s), failed/errored {} time(s)",
            a.passed, a.failed
        ));
        if a.missing > 0 || a.unknown > 0 {
            reasons.push(format!(
                "additional ambiguity: missing {} time(s), unknown {} time(s)",
                a.missing, a.unknown
            ));
        }
        Classification::FlakySuspected
    } else if has_discovery_inconsistency {
        reasons.push(format!(
            "inconsistent discovery: observed {} time(s) but missing in {} complete run(s)",
            a.observed, a.missing
        ));
        if a.unknown > 0 {
            reasons.push(format!("unknown in {} run(s)", a.unknown));
        }
        Classification::FlakySuspected
    } else if effective == 0 {
        reasons.push(format!(
            "no pass/fail outcomes observed (skipped {}, unknown {}, missing {})",
            a.skipped, a.unknown, a.missing
        ));
        Classification::Inconclusive
    } else if a.failed == 0 && a.passed > 0 {
        if a.unknown == 0 && a.missing == 0 && a.skipped == 0 {
            reasons.push(format!("passed in all {} iteration(s)", a.passed));
            Classification::StablePass
        } else {
            reasons.push(format!(
                "passed in observed iterations ({}), but evidence is incomplete (skipped {}, missing {}, unknown {})",
                a.passed, a.skipped, a.missing, a.unknown
            ));
            Classification::Inconclusive
        }
    } else if a.passed == 0 && a.failed > 0 {
        if a.unknown == 0 && a.missing == 0 && a.skipped == 0 {
            reasons.push(format!("failed/errored in all {} iteration(s)", a.failed));
            Classification::StableFail
        } else {
            reasons.push(format!(
                "failed/errored in observed iterations ({}), but evidence is incomplete (skipped {}, missing {}, unknown {})",
                a.failed, a.skipped, a.missing, a.unknown
            ));
            Classification::Inconclusive
        }
    } else {
        reasons.push("unable to determine a confident classification".to_string());
        Classification::Inconclusive
    };

    ClassifiedTest {
        id: a.id,
        classification,
        metrics,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{TestCaseResult, TestId, TestStatus};

    fn case(name: &str, status: TestStatus) -> TestCaseResult {
        TestCaseResult {
            id: TestId::new(None::<String>, "t", name),
            status,
            duration_secs: None,
            message: None,
            details: None,
        }
    }

    fn input(iteration: u32, quality: RunQuality, cases: Vec<TestCaseResult>) -> IterationInput {
        IterationInput { iteration, quality, cases }
    }

    #[test]
    fn all_pass_is_stable_pass() {
        let inputs: Vec<_> = (1..=10)
            .map(|i| input(i, RunQuality::Ok, vec![case("x", TestStatus::Passed)]))
            .collect();
        let r = analyze(&inputs);
        assert_eq!(r.summary.stable_pass, 1);
        assert_eq!(r.tests[0].classification, Classification::StablePass);
    }

    #[test]
    fn all_fail_is_stable_fail() {
        let inputs: Vec<_> = (1..=10)
            .map(|i| input(i, RunQuality::Ok, vec![case("x", TestStatus::Failed)]))
            .collect();
        let r = analyze(&inputs);
        assert_eq!(r.tests[0].classification, Classification::StableFail);
    }

    #[test]
    fn mixed_pass_fail_is_flaky() {
        let mut cases = vec![TestStatus::Passed; 9];
        cases.push(TestStatus::Failed);
        let inputs: Vec<_> = cases
            .into_iter()
            .enumerate()
            .map(|(i, s)| input(i as u32 + 1, RunQuality::Ok, vec![case("x", s)]))
            .collect();
        let r = analyze(&inputs);
        assert_eq!(r.tests[0].classification, Classification::FlakySuspected);
    }

    #[test]
    fn disappearing_test_is_flaky() {
        let inputs = vec![
            input(1, RunQuality::Ok, vec![case("x", TestStatus::Passed)]),
            input(2, RunQuality::Ok, vec![]), // disappeared in a complete run
            input(3, RunQuality::Ok, vec![case("x", TestStatus::Passed)]),
        ];
        let r = analyze(&inputs);
        assert_eq!(r.tests[0].classification, Classification::FlakySuspected);
        assert!(r.tests[0].metrics.missing >= 1);
    }

    #[test]
    fn timeout_creates_unknown_evidence() {
        let inputs = vec![
            input(1, RunQuality::Ok, vec![case("x", TestStatus::Passed)]),
            input(2, RunQuality::TimedOut, vec![]),
        ];
        let r = analyze(&inputs);
        // Conservative: pass observed but run evidence incomplete => inconclusive.
        assert_eq!(r.tests[0].classification, Classification::Inconclusive);
        assert_eq!(r.tests[0].metrics.unknown, 1);
    }

    #[test]
    fn malformed_incomplete_parse_is_unknown() {
        let inputs = vec![
            input(1, RunQuality::Ok, vec![case("x", TestStatus::Passed)]),
            input(2, RunQuality::IncompleteParse, vec![]),
        ];
        let r = analyze(&inputs);
        assert_eq!(r.tests[0].classification, Classification::Inconclusive);
    }

    #[test]
    fn all_skipped_is_inconclusive() {
        let inputs: Vec<_> = (1..=3)
            .map(|i| input(i, RunQuality::Ok, vec![case("x", TestStatus::Skipped)]))
            .collect();
        let r = analyze(&inputs);
        assert_eq!(r.tests[0].classification, Classification::Inconclusive);
    }
}
