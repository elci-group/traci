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
traci check . --format sarif > traci.sarif
```

Exit status is `0` when policy passes, `2` when findings meet the configured
threshold, and `1` for invalid configuration or I/O errors.

Start the native polling daemon during development:

```console
traci watch src
traci watch . --interval 500
traci watch src --once
```

`--once` runs the same watch entry point without blocking, which is useful for
service health checks. Traci rescans only when a Rust file is created, removed,
resized, or modified.

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
require_correlation_id = true
correlation_fields = ["correlation_id", "request_id", "trace_id"]
ignore = ["target", ".git", ".kaptaind", "tests/fixtures"]
enabled_rules = ["TRC001", "TRC002", "TRC003", "TRC004", "TRC005", "TRC006", "TRC007", "TRC008"]

[watch]
interval_ms = 750
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
deliver --spec deliver.toml --strict
```

The repository has no third-party Rust dependencies. CI runs formatting,
Clippy, tests, and a Traci self-check on every push and pull request.
