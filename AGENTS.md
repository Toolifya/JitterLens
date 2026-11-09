# AGENTS.md — Working in the JitterLens Repository

This document guides autonomous and human contributors working on JitterLens.

## Project Summary
JitterLens is a **local-first, open-source** developer tool that detects flaky tests by repeatedly executing test suites, collecting results, and producing explainable reports. The MVP targets **Python projects using pytest** on **Linux**.

## Core Principles
- **Local-only**: no cloud service, hosted backend, paid API, or AI model. All analysis happens locally.
- **Reliability over cleverness**: prefer maintainable, explicit code.
- **Verify, don't assume**: run tests/commands before claiming success. Never fabricate output.
- **Explainable detection**: classifications must be deterministic and documented; avoid overstating certainty.
- **Safety**: test execution runs arbitrary project code—document risks and avoid corrupting user projects.

## Development Rules
1. Inspect the repository and relevant files before making changes.
2. Read `AGENTS.md` and relevant docs (`docs/architecture.md`, `docs/roadmap.md`) before starting work.
3. Do **not** overwrite existing user work. Investigate unfamiliar files before modifying/deleting.
4. **Never delete files** without explicit user approval.
5. Do not introduce unnecessary abstractions or dependencies.
6. Do not implement features outside the current milestone.
7. Write tests for every important feature.
8. Run tests after meaningful changes and fix regressions before continuing.
9. Keep modules focused: separate CLI, execution, parsing, detection, storage, and reporting.
10. Do not commit secrets, credentials, or user test data.

## Technology Constraints (MVP)
- Rust (stable) + Cargo
- `clap` for CLI parsing
- `tokio` only where async provides real benefit
- `anyhow` + `thiserror` for errors
- `tracing` for logging
- `rusqlite` (SQLite) for persistence
- `serde` + JSON for internal data exchange
- pytest integration via subprocess; JUnit XML output for results
- Static HTML/CSS reports (minimal or no JavaScript)

## Repository Structure (target)
- `src/` — application code (modular by responsibility)
- `tests/` — integration tests, fixtures, synthetic pytest projects
- `docs/` — architecture, development, roadmap

## Workflow
- Work **milestone by milestone** (see `docs/roadmap.md`).
- After each milestone, provide: summary, files changed, decisions, commands/tests run with actual results, known issues/limitations, and recommended next step. **Wait for confirmation** before starting a new milestone.

## Testing Expectations
- Unit tests for core logic (especially detection and parsing).
- Integration tests for CLI/runner flows where practical.
- Synthetic pytest projects under `tests/` to validate classifications and edge cases.
- Snapshot tests only where they add clear value.

## Security & Privacy Notes
- Never transmit data off-machine.
- Running `pytest` executes untrusted/project code; make this explicit in docs and CLI help where appropriate.
- Avoid writing reports or temp files into user project trees in ways that overwrite existing files; use safe, unique output paths.
