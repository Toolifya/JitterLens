# JitterLens v0.1.0 (MVP) — Release Notes

## Summary
Initial MVP of **JitterLens**, a local-first flaky test detector for **Python/pytest** projects on **Linux**.

JitterLens repeatedly runs pytest, captures per-iteration JUnit XML, parses results defensively, classifies tests deterministically, saves optional local SQLite history, and generates a standalone static HTML report.

## Highlights
- CLI: `jitterlens scan <PATH> --runs --timeout --output --no-history --verbose`
- Pytest project detection and subprocess execution with per-iteration JUnit XML
- Timeout/failed-start/interruption-aware execution records
- Defensive JUnit XML parsing and stable test identities
- Deterministic classifications: `STABLE_PASS`, `STABLE_FAIL`, `FLAKY_SUSPECTED`, `INCONCLUSIVE`
- Local SQLite persistence (migrations, transactions, indexes), disable with `--no-history`
- Standalone static HTML report (embedded CSS, no external JS/assets)
- Security/validation hardening (arbitrary-code execution warning, CLI argument validation)

## Linux release binary (built in this workspace)
- Target: `x86_64-unknown-linux-gnu` (Rust 1.98.1 host reported)
- Build command: `cargo build --release`
- Artifact: `target/release/jitterlens`
- Actual build result: `Finished \`release\` profile [optimized] target(s) in 1m 51s`
- Version: `jitterlens 0.1.0` (`./target/release/jitterlens --version`)
- Actual checksum (this environment):
  - `2ac9e423ff707e4d81041f90da87fb1b7f2f2249403a617d2cc82b2481a060e8  target/release/jitterlens`

## Known limitations
- MVP supports **pytest on Linux** only; sequential runs.
- Classification is explainable/heuristic and conservative; `FLAKY_SUSPECTED` is not scientific proof.
- Environment-dependent issues (order/shared state/external services/timing) may affect results.
- No history browsing UI/CLI, no non-pytest adapters, no parallel/distributed execution.
- Running tests executes project code (no sandbox).

## Security
Scan only trusted projects. JitterLens runs locally and does not transmit data off-machine.
