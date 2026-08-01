<p align="center">
  <img src="assets/traci-logo.png" width="184" alt="Traci logo: a cyan telemetry trace with three coral event nodes inside a midnight hexagon">
</p>

<h1 align="center">Traci</h1>

<p align="center">
  Pedantic observability enforcement for Rust.<br>
  Make every failure path reconstructable before it reaches production.
</p>

<p align="center">
  <a href="https://github.com/elci-group/traci/actions/workflows/ci.yml"><img src="https://github.com/elci-group/traci/actions/workflows/ci.yml/badge.svg" alt="CI status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-38D9FF.svg" alt="MIT license"></a>
  <img src="https://img.shields.io/badge/Rust-1.74%2B-FF6B6B.svg" alt="Rust 1.74 or newer">
  <img src="https://img.shields.io/badge/dependencies-0-101827.svg" alt="Zero third-party Rust dependencies">
</p>

Traci treats reconstructable telemetry as part of software correctness. If an
engineer cannot establish what failed, where, why, and under which request or
task from telemetry alone, Traci reports the execution path as incomplete.

It is a fast, dependency-free lexical analyzer: it does not compile the target
project, does not contact a network service, and never changes inspected source
code during `check` or `complexity`.

## Quick start

```console
git clone https://github.com/elci-group/traci.git
cd traci
cargo install --path .
traci check src
```

A healthy project exits `0`:

```text
TRACI  OBSERVABILITY GATE · ALL CODE
OK  All inspected execution paths are observable.
SUMMARY  12 file(s) · 0 finding(s) · 0 warning · 0 error · 0 critical
```

Use the production scope and fail on warnings in CI:

```console
traci check . --production-only --fail-on warning --color never --no-emoji
```

## What Traci catches

| Rule | Default severity | Finding |
| --- | --- | --- |
| `TRC001` | Error | Error return or handler has no nearby event |
| `TRC002` | Error | A `Result` is discarded with `.ok()` or `let _ =` |
| `TRC003` | Warning | A diagnostic says only “failed” or another generic message |
| `TRC004` | Warning | A trace event contains no structured fields |
| `TRC005` | Error | Spawned async work does not preserve its span |
| `TRC006` | Warning | `map_err` discards the source error |
| `TRC007` | Critical | `unwrap` or `expect` creates an opaque panic path |
| `TRC008` | Warning | A public async boundary has no configured correlation field |

Inspect the active policy from the CLI:

```console
traci rules
traci explain TRC005
```

The analyzer ignores strings, comments, unit-test modules, symlinks, generated
output, and configured paths. A narrow `// traci: allow` comment suppresses a
deliberately reviewed line or the immediately following line. Prefer real
instrumentation; every allowance should explain why propagation is safe.

## Instrumentation Traci accepts

Record the operation, stable identifiers, correlation context, and source
error:

```rust,ignore
error!(
    operation = "load_customer",
    customer_id,
    correlation_id,
    retry = attempt,
    error = %source,
    "primary database timed out"
);
return Err(DatabaseError::Timeout);
```

Preserve the current span across spawned work:

```rust,ignore
use tracing::Instrument;

tokio::spawn(async move {
    refresh_customer(customer_id).await;
}.instrument(tracing::Span::current()));
```

## Command map

| Command | Purpose |
| --- | --- |
| `traci check [PATH...]` | Enforce the configured observability policy |
| `traci complexity [PATH...]` | Estimate instrumentation and refactoring difficulty |
| `traci watch [PATH...]` | Rescan in the foreground when Rust files change |
| `traci tui [PATH...]` | Show a live terminal dashboard |
| `traci daemon start\|run\|status\|stop` | Manage persistent monitoring |
| `traci trace [PATH...] --goal TEXT` | Plan a guarded LLM-assisted improvement |
| `traci rules` | List rule codes and severities |
| `traci explain RULE` | Explain one rule and its remediation |
| `traci init` | Create a starter `traci.toml` without overwriting policy |

Exit status is `0` when policy passes, `2` when findings meet the configured
threshold, and `1` for configuration or I/O failure.

## Scope and output

`--production-only` excludes tests, benches, examples, fixtures, fuzz targets,
snapshots, and test/spec filename conventions. `--all-code` overrides a policy
that enables production scope. Configured ignore paths still apply.

```console
traci check . --production-only
traci complexity . --production-only --format json
traci daemon run . --production-only --once --format jsonl
traci check . --format sarif > traci.sarif
```

Human output may use color, emoji, and a short progress animation on a terminal.
Redirected output stays undecorated, and `NO_COLOR` is respected. JSON, JSONL,
and SARIF never contain presentation control sequences.

```console
traci tui . --production-only
traci tui . --production-only --once --color never --no-emoji
```

The complexity score is a `0..100` routing signal, not cyclomatic complexity.
It combines code size, functions, branches, error propagation, async and
concurrency boundaries, nesting, and project breadth.

## Configuration

Traci discovers `traci.toml` from the first scan path upward. Unknown keys and
unknown rule codes are errors so a typo cannot silently weaken enforcement.

```toml
[analysis]
fail_on = "error"
trace_window = 6
production_only = false
require_correlation_id = true
correlation_fields = ["correlation_id", "request_id", "trace_id"]
ignore = ["target", ".git", ".kaptaind", ".traci", "tests/fixtures"]
enabled_rules = ["TRC001", "TRC002", "TRC003", "TRC004", "TRC005", "TRC006", "TRC007", "TRC008"]

[ui]
color = "auto"
emoji = true
animate = true
```

See [the configuration reference](skills/traci/references/configuration.md) for
watch intervals, notification channels, model routing, and regression gates.
Treat notification commands, webhooks, provider commands, and benchmark
commands as executable security boundaries; keep credentials out of TOML.

## Daemon and notifications

```console
traci daemon start src --production-only
traci daemon status
traci daemon stop
traci daemon run src --once --format jsonl
```

Detached state, PID data, and append-only logs live under `.traci/`. The daemon
rescans when Rust files are created, removed, resized, or modified.
Notifications can target the console, a trusted local command, or an HTTP JSON
webhook. Channel failures are recorded without disabling enforcement.

## Guarded LLM improvement

Planning is read-only:

```console
traci trace src --goal "reduce branching in the analyzer"
traci trace src --goal "preserve span context" --format json
```

Only `--apply` enables mutation. The pipeline benchmarks the baseline, creates
an isolated Git worktree, accepts a path-limited unified diff, repeats the
benchmark, rejects diagnostic or complexity regressions, commits the candidate,
and fast-forwards only after every gate passes.

```text
score → select model → baseline benchmark → isolated worktree
      → scoped patch → candidate analysis → repeated benchmark
      → regression gates → candidate commit → fast-forward or reject
```

Provider and scope failures are discarded. A benchmarked but rejected candidate
may remain on its branch for inspection and is never merged automatically.

## Project documentation

- [Architecture and trust boundaries](docs/ARCHITECTURE.md)
- [Contribution guide](CONTRIBUTING.md)
- [Security policy](SECURITY.md)
- [Path from B+ to A-](docs/ROADMAP.md)
- [Agent skill](skills/traci/SKILL.md)

## Development

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
env RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps
cargo run -- check . --production-only --fail-on warning
deliver --spec deliver.toml --strict
```

The project forbids unsafe Rust, denies Clippy warnings, and has no third-party
Rust dependencies. CI runs formatting, linting, warning-free Rustdoc, tests,
strict production self-analysis, complexity, daemon, TUI, and trace-plan checks
on every push and pull request.

## License

Traci is available under the [MIT License](LICENSE).
