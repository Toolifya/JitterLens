# JitterLens

> See beyond test failures.

JitterLens is a **local-first, open-source** developer tool that detects potentially flaky tests in software projects.

A flaky test is a test that produces inconsistent results without relevant code changes. JitterLens runs a test suite repeatedly, collects per-test results, analyzes consistency, and generates an explainable report.

## MVP scope

- **Platform:** Linux (initial)
- **Test framework:** Python projects using `pytest`
- **Analysis:** local and deterministic (no cloud service, hosted backend, paid API, or AI model)
- **Output:** console summary, standalone static HTML report, and local SQLite history (optional)

## How it works

1. Detects a pytest-capable project.
2. Runs pytest repeatedly (sequentially) with a separate JUnit XML report per iteration.
3. Parses JUnit XML defensively (handles missing/malformed/incomplete results).
4. Classifies tests using explainable, deterministic rules:
   - `STABLE_PASS`
   - `STABLE_FAIL`
   - `FLAKY_SUSPECTED`
   - `INCONCLUSIVE`
5. Persists history locally in SQLite (unless `--no-history`).
6. Generates a standalone static HTML report.

## Requirements

- Linux
- Rust toolchain (to build from source)
- Python 3 and `pytest` available in the environment you want to scan
  - Default invocation: `python3 -m pytest`
  - Override with `JITTERLENS_PYTEST_CMD` (whitespace-split), e.g.:
    ```bash
    JITTERLENS_PYTEST_CMD="/path/to/venv/bin/python -m pytest" jitterlens scan . --runs 10
    ```

## Installation (from source)

```bash
git clone <repository-url>
cd jitterlens
cargo build --release
# binary: target/release/jitterlens
```

Or install locally with Cargo (from the repository root):

```bash
cargo install --path .
```

## Usage

```bash
jitterlens scan . --runs 10
jitterlens scan ./my-project --runs 30 --timeout 120 --output ./jitterlens-artifacts
jitterlens scan . --runs 10 --no-history
jitterlens --help
jitterlens scan --help
```

### `scan` options

- `PATH` (default: `.`): project to scan
- `--runs <N>` (default: `10`, must be `>= 1`): number of iterations
- `--timeout <SECONDS>` (must be `>= 1`): per-run timeout; omit for no timeout
- `--output <DIR>`: base directory for artifacts/history (default: `./jitterlens-artifacts`)
- `--no-history`: do not persist results to SQLite
- `--verbose`: enable verbose logging

## Output

For each scan, JitterLens creates a **unique session directory** under the output base (e.g., `jitterlens-artifacts/session-<timestamp>-<pid>/`) containing:

- `run-0001.xml`, `run-0002.xml`, ... (per-iteration JUnit XML)
- `report/index.html` (standalone static HTML report)

When history is enabled, it also writes/updates:

- `<output-base>/history.sqlite`

The CLI prints execution progress, analysis counts, explanations for non-stable-pass tests, and the report path.

## Detection notes and limitations

- **Deterministic and explainable:** classifications are rule-based and conservative. `FLAKY_SUSPECTED` indicates inconsistent evidence (e.g., mixed pass/fail or disappearance in complete runs); it is **not** scientific proof of flakiness.
- **No false equivalence:** consistently failing tests are classified as `STABLE_FAIL`, not flaky.
- **Insufficient evidence:** missing/incomplete XML, timeouts, interruptions, or ambiguous runs can produce `INCONCLUSIVE`.
- **Environment-dependent behavior:** test order dependence, shared state, external services, timing, and resource contention can affect results.
- **MVP scope:** pytest-only, sequential execution, Linux-focused, text/HTML per-session reporting (no history browsing UI).

## Security and privacy

- **Arbitrary code execution:** running `pytest` executes project test code. Only scan projects you trust. JitterLens does not sandbox test execution.
- **Local-only:** no network services, no telemetry, no cloud/AI analysis. All data stays on your machine.
- **Safe file handling:** JitterLens uses unique artifact/session paths and refuses to overwrite existing iteration/report files. Do not point outputs at locations where overwrites would be harmful, and review project configurations before scanning untrusted code.

## Development

```bash
cargo test
cargo clippy --all-targets
cargo build --release
```

See `docs/architecture.md`, `docs/development.md`, and `docs/roadmap.md`.

## License

MIT OR Apache-2.0
