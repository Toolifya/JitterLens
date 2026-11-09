# JitterLens Roadmap

This roadmap tracks milestone-by-milestone delivery.

## Status
All MVP milestones (1–8) have been implemented and verified in this workspace (see release notes and test results reported per milestone).

## Milestone 1 — Foundation (complete)
- [x] Initialize Rust project.
- [x] Configure Cargo dependencies.
- [x] Implement CLI structure (`scan` entry point and options parsing).
- [x] Establish module architecture (skeleton modules matching target layout).
- [x] Add logging (`tracing`) and error handling (`anyhow`/`thiserror`).
- [x] Add initial unit tests.
- [x] Run relevant tests and confirm results.

## Milestone 2 — Test Execution (complete)
- [x] Implement pytest project detection.
- [x] Implement subprocess execution.
- [x] Generate a separate JUnit XML report for every iteration.
- [x] Handle timeouts and interruptions.
- [x] Avoid overwriting existing project files; use safe unique output paths.

## Milestone 3 — Result Parsing (complete)
- [x] Implement JUnit XML parser.
- [x] Extract individual test results.
- [x] Establish stable test identities.
- [x] Handle malformed and incomplete results.

## Milestone 4 — Detection Engine (complete)
- [x] Implement deterministic classification logic.
- [x] Calculate failure/skip/timeout/missing rates.
- [x] Detect inconsistent outcomes across runs.
- [x] Create synthetic pytest fixtures.
- [x] Validate classifications with tests.

## Milestone 5 — Persistence (complete)
- [x] Implement SQLite storage and migrations.
- [x] Save analysis sessions and per-test results.
- [x] Support historical analysis queries (foundation).
- [x] Ensure transaction safety and useful indexes.

## Milestone 6 — Reporting (complete)
- [x] Generate standalone static HTML report.
- [x] Display classifications, statistics, and explanations.
- [x] Include execution history, durations, and error summaries.

## Milestone 7 — Hardening (complete)
- [x] Improve error handling and edge-case coverage.
- [x] Test unusual scenarios (synthetic fixtures).
- [x] Review resource consumption and security implications.
- [x] Improve CLI usability and messages.

## Milestone 8 — Initial Release (complete)
- [x] Write README and installation instructions.
- [x] Document limitations and security considerations.
- [x] Prepare Linux release binaries (built/verified in this environment).
- [x] Create release notes.

## Future (post-MVP, not committed)
- Additional language/framework adapters.
- Optional parallel execution with safeguards.
- Richer historical trend views and history browsing UX.
- Cross-platform packaging.