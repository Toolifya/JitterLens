# Development Guide

## Prerequisites
- Rust stable toolchain (edition 2024 compatible; tested with Rust 1.98.x in this workspace)
- Linux (MVP target)
- Python 3 + `pytest` for execution/integration tests and for scanning real projects
  - Tests/integration helpers may use `JITTERLENS_PYTEST_CMD` or a local venv (see `tests/execution_m2.rs` / `tests/classification_m4.rs`).

## Build & test
```bash
cargo test                    # unit + integration tests
cargo clippy --all-targets    # lint checks
cargo build                   # debug build
cargo build --release         # optimized Linux binary
```

## Running locally
```bash
cargo run -- scan . --runs 10
# or after release build:
./target/release/jitterlens scan . --runs 10
```

To use a specific pytest environment:
```bash
JITTERLENS_PYTEST_CMD="/path/to/venv/bin/python -m pytest" ./target/release/jitterlens scan . --runs 10
```

## Project layout
- `src/cli` — clap CLI
- `src/adapters/pytest` — pytest detection and command construction
- `src/runner` — subprocess execution, timeouts, per-iteration JUnit XML paths
- `src/parser/junit` — defensive JUnit XML parsing
- `src/detector` — deterministic classification (independent of runner)
- `src/storage` — SQLite migrations/persistence
- `src/reporting` — standalone static HTML report
- `tests/synthetic_projects` — pytest fixtures used for validation

## Safety notes
- Scanning executes project tests (arbitrary code). Run only trusted projects.
- Do not commit secrets, credentials, local history DBs, or user test data.
- Artifact/session directories are unique; the tool refuses to overwrite existing iteration/report files.

## Milestone workflow
Follow `docs/roadmap.md`. Work milestone by milestone, add tests for important behavior, and run tests before reporting completion.
