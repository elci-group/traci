# Traci

Traci is a deliberately pedantic observability enforcement daemon for Rust. It
treats reconstructable telemetry as part of software correctness: if an
engineer cannot establish what failed, where, why, and under which request or
task from telemetry alone, Traci reports the path as incomplete.

Traci complements Kaptaind. Kaptaind protects version integrity by classifying
and committing changes; Traci protects observability integrity by examining the
execution paths those changes introduce.

## What it enforces

| Rule | Default | Finding |
| --- | --- | --- |
| `TRC001` | Error | Error return or handler has no nearby event |
| `TRC002` | Error | A `Result` is discarded with `.ok()` or `let _ =` |
| `TRC003` | Warning | A diagnostic says only "failed" or another generic message |
| `TRC004` | Warning | A trace event contains no structured fields |
| `TRC005` | Error | Spawned async work does not preserve its span |
| `TRC006` | Warning | `map_err` discards the source error |
| `TRC007` | Critical | `unwrap` or `expect` creates an opaque panic path |
| `TRC008` | Warning | A public async boundary has no configured correlation field |

The analyzer is intentionally dependency-free. It performs fast lexical
analysis without compiling the target and ignores strings, comments, unit-test
modules, symlinks, generated output, and configured paths. A narrow escape
hatch, `// traci: allow`, suppresses a deliberately reviewed line (or the line
immediately after it), making intentional propagation visible in review.

## Install and run

```console
cargo install --path .
traci check src
traci check crates/service --format json --fail-on warning
traci check crates/service --format jsonl
traci check . --format sarif > traci.sarif
```

Human output is the default. `json` is a compact document for API/CI consumers,
`jsonl`/`ndjson` emits streaming records for log pipelines, and `sarif` targets
code-scanning systems. Machine modes never add decoration or ANSI colour.

## Production scope and terminal UI

Limit every file-based operation to deployable code with `--production-only`:

```console
traci check . --production-only
traci complexity . --production-only
traci daemon start . --production-only
traci trace . --production-only --goal "instrument production error paths"
```

This scope excludes `test`/`tests`, benches, benchmarks, examples, fixtures,
fuzz targets, snapshots, and conventional `_test.rs`, `.test.rs`, and
`.spec.rs` files. `--all-code` explicitly overrides a policy that enables
production scope. Configured ignore rules still apply in both modes.

Human output automatically gains colour, emoji, and a short progress animation
when attached to a compatible terminal. Redirected output stays undecorated;
`NO_COLOR` is respected. Override presentation with `--color
auto|always|never`, `--emoji`/`--no-emoji`, and
`--animate`/`--no-animate`.

Launch the live, auto-refreshing terminal dashboard with:

```console
traci tui . --production-only
traci tui . --production-only --once --color never
```

The dashboard summarizes policy health, severity counts, risk, and the most
important findings. It uses no alternate screen or raw terminal mode, so a
normal `Ctrl-C` exits cleanly. `--format tui` also renders a one-shot dashboard
for `traci check`. Machine modes never contain animation, emoji, or ANSI codes.

Exit status is `0` when policy passes, `2` when findings meet the configured
threshold, and `1` for invalid configuration or I/O errors.

Estimate implementation complexity without compiling the target:

```console
traci complexity src
traci complexity src --format json
```

The `0..100` approximation combines non-empty lines, functions, branches,
error propagation points, async boundaries, concurrency primitives, maximum
brace nesting, and project breadth. It is a routing signal, not a claim of
cyclomatic precision. Traci reports both the aggregate and the hottest files.

## Daemon and notifications

Start the persistent polling daemon during development:

```console
traci daemon start src
traci daemon status
traci daemon stop
traci daemon run src --format jsonl
traci daemon run src --once --format jsonl
```

The detached process stores its PID, atomic `status.json`, and append-only logs
under `.traci/`. It rescans only when a Rust file is created, removed, resized,
or modified. `traci watch` remains a foreground-compatible alias.

When the finding count changes, notifications can fan out to:

- a concise human console event;
- a trusted local command, with notification JSON on stdin and `TRACI_EVENT`,
  `TRACI_SEVERITY`, `TRACI_TITLE`, and `TRACI_BODY` environment variables;
- an HTTP JSON webhook through `curl`.

Severity thresholds and cooldowns suppress noisy, repeated delivery. Failed
channels are recorded but never stop enforcement.

## Guarded LLM improvement with `traci trace`

Planning is safe and non-mutating:

```console
traci trace src --goal "reduce branching in the scanner"
traci trace src --goal "simplify the scanner" --format json
```

Traci computes complexity first, selects the configured low, medium, or high
model, and prints the provider/model/branch decision. It does not contact the
provider or touch Git unless `--apply` is explicit:

```console
traci trace src --goal "reduce branching in the scanner" --apply
```

The applying pipeline is deliberately transactional:

```text
score → select model → baseline benchmark → isolated Git worktree
      → LLM unified diff → path-scope check → candidate analysis
      → repeated benchmark → regression gates → candidate commit
      → fast-forward merge only on success
```

The monitored checkout remains untouched while the LLM works. Generated diffs
are size-limited and may only change the requested paths. Traci rejects
increased observability findings, excessive complexity, failed tests, and
benchmark regressions beyond policy. A benchmarked but rejected candidate is
committed on its branch for inspection; it is never merged. Provider or scope
failures are discarded before evaluation.

Providers are local adapters:

- `provider = "command"` runs a trusted configured command, sends the prompt on
  stdin, and exports `TRACI_MODEL`, `TRACI_GOAL`, and
  `TRACI_COMPLEXITY_SCORE`.
- `provider = "ollama"` runs `ollama run <selected-model>`.

The provider must return a unified diff beginning with `diff --git`. The generic
command adapter can front any hosted LLM CLI without putting credentials in
Traci configuration.

Inspect or explain the policy surface:

```console
traci rules
traci explain TRC005
traci init
```

## Policy

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

[watch]
interval_ms = 750

[daemon]
state_dir = ".traci"

[notifications]
enabled = true
console = true
min_severity = "error"
command = ""
webhook_url = ""
cooldown_seconds = 30

[ui]
color = "auto"
emoji = true
animate = true

[trace]
provider = "command"
provider_command = "codex exec --model $TRACI_MODEL -"
low_model = "fast"
medium_model = "balanced"
high_model = "deep"
medium_threshold = 35
high_threshold = 70
branch_prefix = "traci/trace"
benchmark_command = "cargo test --all-targets"
benchmark_runs = 3
max_regression_percent = 10
max_complexity_increase = 0
max_diagnostic_increase = 0
max_context_bytes = 120000
max_patch_bytes = 250000
require_clean = true
```

For a reviewed exception, place `// traci: allow` on the line. Prefer policy
improvements or real instrumentation over broad suppressions.

## Instrumentation Traci accepts

```rust,ignore
error!(
    operation = "load_customer",
    customer_id = customer_id,
    correlation_id = correlation_id,
    retry = attempt,
    error = %source,
    "primary database timed out"
);
return Err(DatabaseError::Timeout);
```

For spawned futures, preserve the active span explicitly:

```rust,ignore
use tracing::Instrument;

tokio::spawn(async move {
    refresh_customer(customer_id).await;
}.instrument(tracing::Span::current()));
```

## Kaptaind monitoring

This repository includes `kaptaind.toml` and `.kaptainignore`. Kaptaind watches
the repository, clusters related edits, computes semantic version impact,
runs `cargo test --all-targets` as a required pre-commit gate, updates
`VERSION`, and creates local deterministic commits. Network pushing is disabled.

```console
kaptaind-cli -r . analyze
kaptaind --daemon
kaptaind-cli -r . status
kaptaind-cli -r . log --limit 20
```

Runtime state lives under `.kaptaind/` and is ignored by both Git and Kaptaind.

## Development

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo run -- check src
cargo run -- complexity src --format json
cargo run -- trace src --goal "reduce complexity" --format json
deliver --spec deliver.toml --strict
```

The repository has no third-party Rust dependencies. CI runs formatting,
Clippy, tests, and a Traci self-check on every push and pull request.
