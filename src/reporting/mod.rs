//! Static HTML report generation.

use crate::core::{Classification, TestCaseResult};
use crate::detector::DetectionReport;
use crate::runner::{IterationOutcome, IterationStatus};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn class_label(c: Classification) -> &'static str {
    match c {
        Classification::StablePass => "STABLE_PASS",
        Classification::StableFail => "STABLE_FAIL",
        Classification::FlakySuspected => "FLAKY_SUSPECTED",
        Classification::Inconclusive => "INCONCLUSIVE",
    }
}

fn class_css(c: Classification) -> &'static str {
    match c {
        Classification::StablePass => "pass",
        Classification::StableFail => "fail",
        Classification::FlakySuspected => "flaky",
        Classification::Inconclusive => "inconclusive",
    }
}

fn status_label(s: IterationStatus) -> &'static str {
    match s {
        IterationStatus::Completed => "Completed",
        IterationStatus::TimedOut => "Timed out",
        IterationStatus::FailedToStart => "Failed to start",
        IterationStatus::Interrupted => "Interrupted",
    }
}

fn pct(rate: Option<f64>) -> String {
    match rate {
        Some(r) => format!("{:.1}%", r * 100.0),
        None => "n/a".to_string(),
    }
}

fn avg_duration(results: &[TestCaseResult]) -> String {
    let mut sum = 0.0f64;
    let mut n = 0u32;
    for r in results {
        if let Some(d) = r.duration_secs {
            sum += d;
            n += 1;
        }
    }
    if n == 0 {
        "n/a".to_string()
    } else {
        format!("{:.3}s", sum / n as f64)
    }
}

fn find_error_summary<'a>(canonical: &str, parsed_results: &'a [Vec<TestCaseResult>]) -> Option<&'a TestCaseResult> {
    for cases in parsed_results.iter().rev() {
        if let Some(c) = cases
            .iter()
            .rev()
            .find(|c| c.id.canonical() == canonical && (c.message.is_some() || c.details.is_some()))
        {
            return Some(c);
        }
    }
    None
}

/// Generate a standalone HTML report in `session_dir/report/index.html`.
pub fn generate(
    session_dir: &Path,
    project_path: &Path,
    outcomes: &[IterationOutcome],
    parsed_results: &[Vec<TestCaseResult>],
    report: &DetectionReport,
) -> Result<PathBuf> {
    let report_dir = session_dir.join("report");
    std::fs::create_dir_all(&report_dir)
        .with_context(|| format!("failed to create report directory {}", report_dir.display()))?;
    let index_path = report_dir.join("index.html");
    anyhow::ensure!(
        !index_path.exists(),
        "refusing to overwrite existing report: {}",
        index_path.display()
    );

    let mut history_by_test: HashMap<String, Vec<String>> = HashMap::new();
    for cases in parsed_results {
        for c in cases {
            history_by_test
                .entry(c.id.canonical())
                .or_default()
                .push(format!("{:?}", c.status));
        }
    }

    let mut html = String::with_capacity(32 * 1024);
    let _ = writeln!(html, "<!doctype html>");
    let _ = writeln!(html, "<html lang=\"en\"><head><meta charset=\"utf-8\">");
    let _ = writeln!(html, "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    let _ = writeln!(html, "<title>JitterLens Report</title>");
    let _ = writeln!(html, "<style>{}</style></head><body>", STYLE);
    let _ = writeln!(html, "<main class=\"wrap\">");
    let _ = writeln!(html, "<header><h1>JitterLens</h1><p class=\"sub\">Local-first flaky test analysis report</p></header>");
    let _ = writeln!(html, "<section class=\"card\"><h2>Overview</h2><dl class=\"grid\">");
    let _ = writeln!(html, "<div><dt>Project</dt><dd><code>{}</code></dd></div>", esc(&project_path.display().to_string()));
    let _ = writeln!(html, "<div><dt>Iterations</dt><dd>{}</dd></div>", report.summary.total_iterations);
    let _ = writeln!(html, "<div><dt>Tests analyzed</dt><dd>{}</dd></div>", report.summary.tests_seen);
    let _ = writeln!(html, "<div><dt>Stable passing</dt><dd><span class=\"badge pass\">{}</span></dd></div>", report.summary.stable_pass);
    let _ = writeln!(html, "<div><dt>Stable failing</dt><dd><span class=\"badge fail\">{}</span></dd></div>", report.summary.stable_fail);
    let _ = writeln!(html, "<div><dt>Potentially flaky</dt><dd><span class=\"badge flaky\">{}</span></dd></div>", report.summary.flaky_suspected);
    let _ = writeln!(html, "<div><dt>Inconclusive</dt><dd><span class=\"badge inconclusive\">{}</span></dd></div>", report.summary.inconclusive);
    let _ = writeln!(html, "</dl>");
    let _ = writeln!(html, "<p class=\"note\">Classification is deterministic and explainable. <code>FLAKY_SUSPECTED</code> means inconsistent evidence was observed; it is not a scientific proof of flakiness. Consistently failing tests are reported as <code>STABLE_FAIL</code>, not flaky.</p>");
    let _ = writeln!(html, "</section>");

    // Flagged tests first.
    let mut flaky: Vec<_> = report.tests.iter().filter(|t| t.classification == Classification::FlakySuspected).collect();
    let mut stable_fail: Vec<_> = report.tests.iter().filter(|t| t.classification == Classification::StableFail).collect();
    flaky.sort_by_key(|a| a.id.canonical());
    stable_fail.sort_by_key(|a| a.id.canonical());

    render_test_section(&mut html, "Potentially Flaky Tests", &flaky, parsed_results, &history_by_test, true);
    render_test_section(&mut html, "Stable Failing Tests", &stable_fail, parsed_results, &history_by_test, true);

    // All tests.
    let _ = writeln!(html, "<section class=\"card\"><h2>All Tests</h2><div class=\"table-wrap\"><table>");
    let _ = writeln!(html, "<thead><tr><th>Test</th><th>Classification</th><th>Pass</th><th>Fail/Err</th><th>Skip</th><th>Missing</th><th>Unknown</th><th>Failure rate</th></tr></thead><tbody>");
    for t in &report.tests {
        let _ = writeln!(html, "<tr><td><code>{}</code></td><td><span class=\"badge {}\">{}</span></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(&t.id.display_name()), class_css(t.classification), class_label(t.classification),
            t.metrics.passed, t.metrics.failed, t.metrics.skipped, t.metrics.missing, t.metrics.unknown, pct(t.metrics.failure_rate()));
    }
    let _ = writeln!(html, "</tbody></table></div></section>");

    // Execution history.
    let _ = writeln!(html, "<section class=\"card\"><h2>Execution History</h2><div class=\"table-wrap\"><table>");
    let _ = writeln!(html, "<thead><tr><th>Iteration</th><th>Status</th><th>Exit</th><th>JUnit XML</th><th>Message</th></tr></thead><tbody>");
    for o in outcomes {
        let _ = writeln!(html, "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            o.iteration,
            esc(status_label(o.status)),
            o.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "n/a".into()),
            if o.junit_xml_exists { "yes" } else { "no" },
            esc(o.message.as_deref().unwrap_or("")));
    }
    let _ = writeln!(html, "</tbody></table></div></section>");

    // Durations / error summaries for flagged tests.
    let _ = writeln!(html, "<section class=\"card\"><h2>Test Details &amp; Error Summaries</h2>");
    if flaky.is_empty() && stable_fail.is_empty() {
        let _ = writeln!(html, "<p>No flaky or stable-failing tests detected.</p>");
    } else {
        for t in flaky.iter().chain(stable_fail.iter()) {
            let canonical = t.id.canonical();
            let hist = history_by_test.get(&canonical).map(|v| v.join(", ")).unwrap_or_else(|| "no parsed per-iteration statuses".into());
            let all_results: Vec<TestCaseResult> = parsed_results.iter().flat_map(|v| v.iter().filter(|c| c.id.canonical() == canonical).cloned()).collect();
            let _ = writeln!(html, "<article class=\"test-detail\"><h3><span class=\"badge {}\">{}</span> <code>{}</code></h3>", class_css(t.classification), class_label(t.classification), esc(&t.id.display_name()));
            let _ = writeln!(html, "<p><strong>Reasons:</strong></p><ul>");
            for r in &t.reasons { let _ = writeln!(html, "<li>{}</li>", esc(r)); }
            let _ = writeln!(html, "</ul>");
            let _ = writeln!(html, "<p><strong>Failure rate:</strong> {} &nbsp; <strong>Avg duration:</strong> {}</p>", pct(t.metrics.failure_rate()), avg_duration(&all_results));
            let _ = writeln!(html, "<p><strong>Observed status history:</strong> {}</p>", esc(&hist));
            if let Some(err) = find_error_summary(&canonical, parsed_results) {
                if let Some(msg) = &err.message { let _ = writeln!(html, "<p><strong>Sample message:</strong></p><pre>{}</pre>", esc(msg)); }
                if let Some(details) = &err.details { let _ = writeln!(html, "<p><strong>Sample details:</strong></p><pre>{}</pre>", esc(details)); }
            }
            let _ = writeln!(html, "</article>");
        }
    }
    let _ = writeln!(html, "</section>");

    let _ = writeln!(html, "<footer class=\"note\">Generated locally by JitterLens. Running tests executes project code; review projects before scanning untrusted sources.</footer>");
    let _ = writeln!(html, "</main></body></html>");

    std::fs::write(&index_path, html)
        .with_context(|| format!("failed to write report {}", index_path.display()))?;
    Ok(index_path)
}

fn render_test_section(
    html: &mut String,
    title: &str,
    tests: &[&crate::detector::ClassifiedTest],
    _parsed_results: &[Vec<TestCaseResult>],
    _history: &HashMap<String, Vec<String>>,
    show_reasons: bool,
) {
    if tests.is_empty() {
        return;
    }
    let _ = writeln!(html, "<section class=\"card\"><h2>{}</h2>", esc(title));
    let _ = writeln!(html, "<div class=\"table-wrap\"><table><thead><tr><th>Test</th><th>Pass</th><th>Fail/Err</th><th>Missing</th><th>Unknown</th><th>Failure rate</th>{}</tr></thead><tbody>", if show_reasons { "<th>Reasons</th>" } else { "" });
    for t in tests {
        let reasons = if show_reasons { esc(&t.reasons.join("; ")) } else { String::new() };
        let _ = writeln!(html, "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>{}</tr>",
            esc(&t.id.display_name()), t.metrics.passed, t.metrics.failed, t.metrics.missing, t.metrics.unknown, pct(t.metrics.failure_rate()),
            if show_reasons { format!("<td>{}</td>", reasons) } else { String::new() });
    }
    let _ = writeln!(html, "</tbody></table></div></section>");
}

const STYLE: &str = r#"
:root{--bg:#0b1020;--card:#121a2b;--text:#e8eefc;--muted:#9fb0d0;--line:#24314f;--pass:#1f7a4d;--fail:#a33a3a;--flaky:#a56a00;--inc:#506080}
*{box-sizing:border-box} body{margin:0;font:15px/1.5 system-ui,-apple-system,Segoe UI,Roboto,sans-serif;background:linear-gradient(180deg,#0b1020,#0f1730);color:var(--text)}
.wrap{max-width:1100px;margin:0 auto;padding:24px} header{margin-bottom:16px} h1{margin:0;font-size:28px} h2{margin:0 0 12px;font-size:18px} h3{margin:0 0 8px;font-size:16px}
.sub,.note{color:var(--muted)} .card{background:rgba(18,26,43,.95);border:1px solid var(--line);border-radius:12px;padding:16px;margin:14px 0}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:10px;margin:0} dt{color:var(--muted);font-size:12px} dd{margin:2px 0 0;font-size:18px}
.table-wrap{overflow:auto} table{width:100%;border-collapse:collapse;font-size:14px} th,td{padding:8px 10px;border-bottom:1px solid var(--line);vertical-align:top;text-align:left}
th{color:var(--muted);font-weight:600} code,pre{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}
pre{white-space:pre-wrap;background:#09101d;border:1px solid var(--line);padding:10px;border-radius:8px;overflow:auto}
.badge{display:inline-block;padding:2px 8px;border-radius:999px;font-size:12px;font-weight:600;color:#fff}
.badge.pass{background:var(--pass)} .badge.fail{background:var(--fail)} .badge.flaky{background:var(--flaky)} .badge.inconclusive{background:var(--inc)}
.test-detail{border-top:1px solid var(--line);padding-top:12px;margin-top:12px}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Classification, TestId};
    use crate::detector::{ClassifiedTest, DetectionReport, DetectionSummary, TestMetrics};
    use crate::runner::{IterationOutcome, IterationStatus};
    use std::path::PathBuf;

    #[test]
    fn generates_html_report() {
        let dir = tempfile::tempdir().expect("tmp");
        let session = dir.path();
        let outcomes = vec![IterationOutcome {
            iteration: 1,
            status: IterationStatus::Completed,
            exit_code: Some(0),
            junit_xml_path: PathBuf::from("run.xml"),
            junit_xml_exists: true,
            stdout: String::new(),
            stderr: String::new(),
            message: None,
        }];
        let report = DetectionReport {
            tests: vec![ClassifiedTest {
                id: TestId::new(None::<String>, "m", "t"),
                classification: Classification::FlakySuspected,
                metrics: TestMetrics {
                    total_iterations: 1,
                    observed: 1,
                    passed: 0,
                    failed: 1,
                    skipped: 0,
                    missing: 0,
                    unknown: 0,
                },
                reasons: vec!["mixed".into()],
            }],
            summary: DetectionSummary {
                total_iterations: 1,
                tests_seen: 1,
                stable_pass: 0,
                stable_fail: 0,
                flaky_suspected: 1,
                inconclusive: 0,
            },
        };
        let path = generate(session, Path::new("/proj"), &outcomes, &[vec![]], &report).expect("gen");
        assert!(path.is_file());
        let html = std::fs::read_to_string(path).expect("read");
        assert!(html.contains("JitterLens Report"));
        assert!(html.contains("FLAKY_SUSPECTED"));
        assert!(html.contains("Potentially Flaky Tests"));
    }
}
