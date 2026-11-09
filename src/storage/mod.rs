//! SQLite persistence for sessions and results.

use crate::core::{Classification, TestCaseResult, TestId};
use crate::detector::DetectionReport;
use crate::runner::IterationOutcome;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: i64 = 1;

pub struct Storage {
    conn: Connection,
}

/// Data needed to persist one scan session (borrowed).
pub struct SessionRecord<'a> {
    pub project_path: &'a Path,
    pub started_at_ms: i64,
    pub runs_requested: u32,
    pub framework: &'a str,
    pub output_dir: Option<&'a Path>,
    pub outcomes: &'a [IterationOutcome],
    pub parsed_results: &'a [Vec<TestCaseResult>],
    pub report: &'a DetectionReport,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create database directory {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open SQLite database {}", path.display()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .context("failed to configure SQLite")?;
        let mut s = Self { conn };
        s.migrate().context("failed to migrate database schema")?;
        Ok(s)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("failed to open in-memory SQLite database")?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .context("failed to configure SQLite")?;
        let mut s = Self { conn };
        s.migrate()?;
        Ok(s)
    }

    fn migrate(&mut self) -> Result<()> {
        let tx = self.conn.transaction().context("failed to start migration transaction")?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at_ms INTEGER NOT NULL
            );",
        )?;
        let current: Option<i64> = tx
            .query_row(
                "SELECT MAX(version) FROM schema_migrations",
                [],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let current = current.unwrap_or(0);
        if current < 1 {
            tx.execute_batch(
                "CREATE TABLE sessions (
                    id INTEGER PRIMARY KEY,
                    project_path TEXT NOT NULL,
                    started_at_ms INTEGER NOT NULL,
                    finished_at_ms INTEGER,
                    runs_requested INTEGER NOT NULL,
                    runs_completed INTEGER NOT NULL,
                    framework TEXT NOT NULL,
                    output_dir TEXT
                );
                CREATE TABLE tests (
                    id INTEGER PRIMARY KEY,
                    canonical TEXT NOT NULL UNIQUE,
                    file TEXT,
                    classname TEXT NOT NULL,
                    name TEXT NOT NULL,
                    display_name TEXT NOT NULL
                );
                CREATE TABLE iteration_runs (
                    id INTEGER PRIMARY KEY,
                    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                    iteration INTEGER NOT NULL,
                    status TEXT NOT NULL,
                    exit_code INTEGER,
                    junit_xml_path TEXT,
                    junit_xml_exists INTEGER NOT NULL,
                    message TEXT,
                    UNIQUE(session_id, iteration)
                );
                CREATE TABLE test_results (
                    id INTEGER PRIMARY KEY,
                    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                    iteration_run_id INTEGER NOT NULL REFERENCES iteration_runs(id) ON DELETE CASCADE,
                    test_id INTEGER NOT NULL REFERENCES tests(id),
                    status TEXT NOT NULL,
                    duration_secs REAL,
                    message TEXT,
                    details TEXT,
                    UNIQUE(iteration_run_id, test_id)
                );
                CREATE TABLE classifications (
                    id INTEGER PRIMARY KEY,
                    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                    test_id INTEGER NOT NULL REFERENCES tests(id),
                    classification TEXT NOT NULL,
                    observed INTEGER NOT NULL,
                    passed INTEGER NOT NULL,
                    failed INTEGER NOT NULL,
                    skipped INTEGER NOT NULL,
                    missing INTEGER NOT NULL,
                    unknown INTEGER NOT NULL,
                    failure_rate REAL,
                    reasons_json TEXT NOT NULL,
                    UNIQUE(session_id, test_id)
                );
                CREATE INDEX idx_iteration_runs_session ON iteration_runs(session_id);
                CREATE INDEX idx_test_results_session ON test_results(session_id);
                CREATE INDEX idx_test_results_test ON test_results(test_id);
                CREATE INDEX idx_classifications_session ON classifications(session_id);
                CREATE INDEX idx_tests_canonical ON tests(canonical);",
            )?;
            tx.execute(
                "INSERT INTO schema_migrations(version, applied_at_ms) VALUES (?1, ?2)",
                params![1i64, now_ms()],
            )?;
        }
        if current > SCHEMA_VERSION {
            anyhow::bail!(
                "database schema version {} is newer than supported {}",
                current,
                SCHEMA_VERSION
            );
        }
        tx.commit().context("failed to commit migrations")?;
        Ok(())
    }

    pub fn save_session(&mut self, rec: &SessionRecord<'_>) -> Result<i64> {
        anyhow::ensure!(
            rec.outcomes.len() == rec.parsed_results.len(),
            "outcomes and parsed_results length mismatch"
        );
        let tx = self
            .conn
            .transaction()
            .context("failed to start save transaction")?;
        let finished_at_ms = now_ms();
        let runs_completed = rec.outcomes.len() as i64;
        tx.execute(
            "INSERT INTO sessions(project_path, started_at_ms, finished_at_ms, runs_requested, runs_completed, framework, output_dir)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                rec.project_path.display().to_string(),
                rec.started_at_ms,
                finished_at_ms,
                rec.runs_requested as i64,
                runs_completed,
                rec.framework,
                rec.output_dir.map(|p| p.display().to_string()),
            ],
        )?;
        let session_id = tx.last_insert_rowid();

        for (outcome, cases) in rec.outcomes.iter().zip(rec.parsed_results.iter()) {
            tx.execute(
                "INSERT INTO iteration_runs(session_id, iteration, status, exit_code, junit_xml_path, junit_xml_exists, message)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    session_id,
                    outcome.iteration as i64,
                    format!("{:?}", outcome.status),
                    outcome.exit_code,
                    outcome.junit_xml_path.display().to_string(),
                    if outcome.junit_xml_exists { 1i64 } else { 0i64 },
                    outcome.message.as_deref(),
                ],
            )?;
            let iteration_run_id = tx.last_insert_rowid();
            for case in cases {
                let test_id = upsert_test(&tx, &case.id)?;
                tx.execute(
                    "INSERT OR REPLACE INTO test_results(session_id, iteration_run_id, test_id, status, duration_secs, message, details)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        session_id,
                        iteration_run_id,
                        test_id,
                        format!("{:?}", case.status),
                        case.duration_secs,
                        case.message.as_deref(),
                        case.details.as_deref(),
                    ],
                )?;
            }
        }

        for ct in &rec.report.tests {
            let test_id = upsert_test(&tx, &ct.id)?;
            let reasons_json = serde_json::to_string(&ct.reasons).unwrap_or_else(|_| "[]".to_string());
            tx.execute(
                "INSERT OR REPLACE INTO classifications(session_id, test_id, classification, observed, passed, failed, skipped, missing, unknown, failure_rate, reasons_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    session_id,
                    test_id,
                    classification_str(ct.classification),
                    ct.metrics.observed as i64,
                    ct.metrics.passed as i64,
                    ct.metrics.failed as i64,
                    ct.metrics.skipped as i64,
                    ct.metrics.missing as i64,
                    ct.metrics.unknown as i64,
                    ct.metrics.failure_rate(),
                    reasons_json,
                ],
            )?;
        }

        tx.commit().context("failed to commit session data")?;
        Ok(session_id)
    }

    pub fn session_count(&self) -> Result<i64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
        Ok(n)
    }

    pub fn classification_count(&self, session_id: i64) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM classifications WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )?;
        Ok(n)
    }
}

fn upsert_test(tx: &Transaction<'_>, id: &TestId) -> Result<i64> {
    let canonical = id.canonical();
    tx.execute(
        "INSERT INTO tests(canonical, file, classname, name, display_name)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(canonical) DO UPDATE SET
           file=excluded.file,
           classname=excluded.classname,
           name=excluded.name,
           display_name=excluded.display_name",
        params![
            canonical,
            id.file.as_deref(),
            id.classname,
            id.name,
            id.display_name(),
        ],
    )?;
    let id: i64 = tx.query_row(
        "SELECT id FROM tests WHERE canonical = ?1",
        params![canonical],
        |r| r.get(0),
    )?;
    Ok(id)
}

fn classification_str(c: Classification) -> &'static str {
    match c {
        Classification::StablePass => "STABLE_PASS",
        Classification::StableFail => "STABLE_FAIL",
        Classification::FlakySuspected => "FLAKY_SUSPECTED",
        Classification::Inconclusive => "INCONCLUSIVE",
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{TestCaseResult, TestId, TestStatus};
    use crate::detector::{ClassifiedTest, DetectionReport, DetectionSummary, TestMetrics};
    use crate::runner::{IterationOutcome, IterationStatus};
    use std::path::PathBuf;

    #[test]
    fn opens_file_db_migrates_and_counts() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("nested/history.sqlite");
        let s = Storage::open(&path).expect("open");
        assert!(path.is_file());
        assert_eq!(s.session_count().expect("count"), 0);
        // Re-open to verify migrations are idempotent.
        let s2 = Storage::open(&path).expect("reopen");
        assert_eq!(s2.session_count().expect("count"), 0);
    }

    #[test]
    fn opens_migrates_and_saves_session() {
        let mut s = Storage::open_in_memory().expect("db");
        let id = TestId::new(None::<String>, "m", "t");
        let outcomes = vec![IterationOutcome {
            iteration: 1,
            status: IterationStatus::Completed,
            exit_code: Some(0),
            junit_xml_path: PathBuf::from("run-0001.xml"),
            junit_xml_exists: true,
            stdout: String::new(),
            stderr: String::new(),
            message: None,
        }];
        let parsed = vec![vec![TestCaseResult {
            id: id.clone(),
            status: TestStatus::Passed,
            duration_secs: Some(0.01),
            message: None,
            details: None,
        }]];
        let report = DetectionReport {
            tests: vec![ClassifiedTest {
                id: id.clone(),
                classification: Classification::StablePass,
                metrics: TestMetrics {
                    total_iterations: 1,
                    observed: 1,
                    passed: 1,
                    failed: 0,
                    skipped: 0,
                    missing: 0,
                    unknown: 0,
                },
                reasons: vec!["ok".into()],
            }],
            summary: DetectionSummary {
                total_iterations: 1,
                tests_seen: 1,
                stable_pass: 1,
                stable_fail: 0,
                flaky_suspected: 0,
                inconclusive: 0,
            },
        };
        let sid = s
            .save_session(&SessionRecord {
                project_path: Path::new("/tmp/proj"),
                started_at_ms: now_ms(),
                runs_requested: 1,
                framework: "pytest",
                output_dir: None,
                outcomes: &outcomes,
                parsed_results: &parsed,
                report: &report,
            })
            .expect("save");
        assert!(sid > 0);
        assert_eq!(s.session_count().expect("count"), 1);
        assert_eq!(s.classification_count(sid).expect("c"), 1);
    }
}
