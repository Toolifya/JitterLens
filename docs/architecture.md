# JitterLens Architecture (MVP)

## Overview
JitterLens is a local CLI tool that scans a target project, repeatedly runs its tests (pytest initially), parses per-run results, classifies test stability/flakiness, persists history, and generates a standalone HTML report.

**Initial scope:** Linux + pytest + JUnit XML + SQLite + static HTML. No network services.

## High-Level Data Flow
1. **CLI** parses arguments (`scan <path> --runs ...`) and loads config/defaults.
2. **Project detection** identifies a pytest-capable project (and validates prerequisites as feasible).
3. **Runner (pytest adapter)** executes N iterations sequentially:
   - invokes `pytest` as a subprocess
   - requests a **unique JUnit XML report per iteration**
   - enforces timeout, captures exit code/stdout/stderr summaries
   - records interrupted/incomplete runs without silently ignoring them
4. **Parser (JUnit XML)** reads each iteration's XML and extracts per-test outcomes (pass/fail/skip/error), durations, and error summaries; handles malformed/incomplete XML defensively.
5. **Test identity** normalizes a stable key per test (e.g., file/class/name/nodeid as available) to correlate results across runs and detect missing/disappearing tests.
6. **Detection engine** aggregates per-test outcomes and classifies:
   - `STABLE_PASS`, `STABLE_FAIL`, `FLAKY_SUSPECTED`, `INCONCLUSIVE`
   - accounts for missing results, timeouts, interruptions, and inconsistent discovery
   - deterministic, explainable thresholds; limitations documented
7. **Storage (SQLite)** persists sessions, test identities, per-run results, and classifications (with migrations, transactions, indexes).
8. **Reporting** generates a lightweight static HTML report summarizing stats, flaky suspects, stable failures, histories, and explanations.

## Module Layout (target)
```
src/
  main.rs
  cli/        # clap definitions, command dispatch
  config/     # defaults, config loading/validation
  core/       # shared domain types (TestId, RunOutcome, Classification, etc.)
  runner/     # execution orchestration, timeouts, process management
  adapters/
    pytest/   # pytest detection + command construction
  parser/
    junit/    # JUnit XML parsing into normalized results
  detector/   # aggregation + classification logic (independent of runner)
  storage/    # SQLite schema, migrations, queries
  reporting/  # HTML report generation
  utils/      # small helpers (paths, time, safe file naming)
```

## Key Design Decisions (MVP)
- **Sequential runs first**: simpler, lower resource usage, easier to reason about; parallelism deferred.
- **JUnit XML per iteration**: avoids cross-run contamination and simplifies parsing; output paths must be unique and safe.
- **Detector independent from runner**: enables unit testing with synthetic data and future adapters.
- **Graceful degradation**: if a run produces no/invalid XML or times out, record the incident and reflect uncertainty (`INCONCLUSIVE`) rather than crashing silently.
- **No AI, no network**: all heuristics are local and deterministic.

## Error Handling & Observability
- Use `anyhow` for application-level propagation and `thiserror` for domain errors where useful.
- Use `tracing` for structured logs; `--verbose` increases detail. User-facing CLI output remains clear and concise.

## Persistence Model (conceptual)
- `sessions`: scan metadata (project path, timestamp, runs requested/completed, config).
- `tests`: stable identity + display name/location.
- `executions`/`results`: per-iteration outcomes, duration, status, error summary, missing/timeout flags.
- `classifications`: per-test final label + metrics (rates, counts) for a session.

## Potential Technical Risks & Mitigations
1. **Arbitrary code execution**: running tests executes user/project code.
   - *Mitigation*: document clearly; never elevate privileges; enforce timeouts; isolate output files.
2. **JUnit XML variability/malformed output**: pytest/plugins may produce unexpected XML or none on crashes.
   - *Mitigation*: defensive parsing; treat invalid/missing XML as an incomplete run; preserve stderr/exit info.
3. **Unstable test identities**: parameterized tests, duplicate names, or nodeid changes can misgroup results.
   - *Mitigation*: use the most specific identifier available (pytest nodeid from XML); record source file/class when present; flag ambiguity.
4. **Resource usage on weak machines**: repeated runs can be slow/heavy.
   - *Mitigation*: sequential default, configurable runs/timeout, minimal dependencies, efficient parsing/storage.
5. **Interfering with the target project**: overwriting existing reports or creating files in-tree.
   - *Mitigation*: write iteration artifacts to a dedicated, unique work directory (e.g., under output/temp) and never overwrite user files.
6. **Non-deterministic environments**: order dependence, shared state, external services in tests may look flaky or mask flakiness.
   - *Mitigation*: report is evidence-based; document limitations; future options may include randomization controls (out of MVP scope unless trivial).
7. **SQLite concurrency/corruption edge cases** (low in single-process CLI).
   - *Mitigation*: transactions, sane pragmas later if needed, clear error handling.

## Out of Scope for MVP
Non-pytest adapters, parallel/distributed execution, cloud sync, AI analysis, GUI, Windows/macOS support guarantees (Linux first).
