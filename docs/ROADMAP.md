# Roadmap: B+ to A-

The August 2026 baseline is **B+ (88/100)**: all delivery gates pass, 28 tests
are green, production analysis has zero findings at warning threshold, Rustdoc
is warning-free, and the project has zero third-party Rust dependencies.

The remaining gap is structural and verification depth. Fract currently marks
7 of 16 modules as warnings, with the highest entropy in `config.rs`,
`trace.rs`, and `main.rs`. CI does not yet pin the declared Rust 1.74 MSRV or a
coverage floor.

## Milestone 1 — reduce architectural concentration

Target: move the Fract result from 9 healthy / 7 warning modules to at least
13 healthy / at most 3 warning modules, with no module at entropy `>= 0.65`.

- Extract command-specific parsing and execution from `main.rs` into a `cli/`
  module family while preserving exit codes and machine output byte-for-byte.
- Split `trace.rs` into planning, provider, worktree, benchmark, and evaluation
  modules with one explicit orchestration layer.
- Replace the section/key match concentration in `config.rs` with typed section
  parsers and focused unknown-key tests.
- Separate terminal measurement/render primitives from dashboard composition in
  `ui.rs`.
- Add characterization tests before each extraction and run Fract after every
  merge-sized change.

Exit gate:

```text
fract index: 0 critical, <=3 warnings, maximum entropy <0.65
cargo test --all-targets: green
machine-output snapshots: unchanged
```

## Milestone 2 — deepen compatibility and test evidence

Target: turn the declared compatibility and behavioral claims into CI gates.

- Add a Rust 1.74 MSRV job running `cargo check --all-targets`.
- Add stable Linux, macOS, and Windows test jobs for filesystem and process
  behavior.
- Add deterministic documentation-link validation to CI.
- Measure coverage with `cargo llvm-cov`; establish an initial baseline, then
  enforce at least 85% line and 75% branch coverage.
- Add property-style tests for source sanitization, config round trips, JSON
  escaping, path classification, and patch-scope validation.
- Add fault-injection tests for provider failure, benchmark timeout, worktree
  cleanup, malformed daemon state, and failed notification channels.

Exit gate:

```text
MSRV + stable OS matrix: green
line coverage >=85%; branch coverage >=75%
rustdoc warnings: 0; documentation links: valid
observability findings at warning threshold: 0
```

## Milestone 3 — release and operational maturity

Target: make releases independently verifiable and operational behavior easy to
support.

- Publish signed GitHub releases with Linux, macOS, and Windows binaries plus
  SHA-256 checksums.
- Add a changelog and documented release checklist tied to semantic versions.
- Add performance fixtures and budgets for large repository scans, daemon
  incremental rescans, and SARIF generation.
- Document state-file compatibility and recovery behavior across upgrades.
- Enable GitHub private vulnerability reporting and dependency/security scans.
- Add end-to-end examples for command notifications, webhooks, and one guarded
  trace provider without embedding credentials.

Exit gate:

```text
reproducible release artifacts + checksums
documented rollback/recovery path
performance budgets enforced without regression
two consecutive main-branch runs satisfy every A- gate
```

## A- definition of done

Traci reaches **A-** when all milestone exit gates pass, the public API and
machine formats remain backward compatible within `0.1.x`, no critical or high
security issue is open, and the evidence is reproducible from CI rather than a
one-off local review.
