# Architecture

Traci is a dependency-free Rust library and CLI. Its design favors explicit
data flow and small trust boundaries over parser completeness.

## Data flow

```text
CLI arguments
    │
    ▼
Config discovery and strict parsing
    │
    ▼
Rust file collection ── production and ignore filters
    │
    ▼
Lexical sanitization ── strings, comments, and test modules masked
    │
    ▼
Rule analysis ── TRC001..TRC008
    │
    ▼
Analysis + Summary
    ├── human / JSON / JSONL / SARIF reports
    ├── terminal dashboard
    ├── daemon state and notifications
    └── trace regression gates
```

The complexity estimator follows the same collection and scope rules but
produces routing metrics rather than policy diagnostics.

## Module map

| Module | Responsibility |
| --- | --- |
| `analyzer` | File discovery, production scoping, sanitization, and rule detection |
| `config` | Strict `traci.toml` parsing and defaults |
| `types` | Diagnostics, rule codes, severities, summaries, and UI choices |
| `report` | Deterministic text, JSON, JSONL, and SARIF serialization |
| `ui` | Terminal styling, dashboards, progress, and trace summaries |
| `complexity` | Instrumentation-complexity approximation and hottest-file ranking |
| `watch` | Polling loop and change detection |
| `daemon` | Detached lifecycle, PID/state files, and one-shot scans |
| `notify` | Console, trusted-command, and webhook notification adapters |
| `trace` | Model routing, isolated worktrees, scoped patches, benchmarks, and merge gates |
| `main` | CLI parsing, command dispatch, and process exit semantics |

## Trust boundaries

Policy checks and complexity scans read source files only. The mutation boundary
starts at `traci trace --apply` and is intentionally explicit.

External execution is limited to configured notification commands, HTTP
webhooks through `curl`, model-provider commands, benchmark commands, and Git
worktree operations. Review those settings before enabling them, never embed
credentials in `traci.toml`, and keep `require_clean = true` for trace runs.

The trace pipeline confines generated changes to requested paths and configured
patch-size limits. A candidate must not increase diagnostics or complexity and
must pass repeated benchmarks before a fast-forward merge.

## Analysis tradeoffs

Traci is lexical rather than compiler-semantic. That makes scans fast,
dependency-free, and usable on incomplete worktrees, but macros and unusual
syntax can reduce precision. Treat findings as enforceable review signals, not
as proof that every runtime path has been modeled.

Narrow `// traci: allow` comments handle reviewed false positives. Broad ignore
patterns are discouraged because they hide future execution paths.

## Verification layers

1. Unit tests pin individual rules, serialization, configuration, complexity,
   notifications, trace gates, and TUI behavior.
2. CLI integration tests pin production scoping and machine-clean output.
3. Fixture tests distinguish reconstructable from opaque telemetry.
4. `deliver.toml` composes formatting, Clippy, tests, self-analysis, daemon,
   production, TUI, complexity, and trace-plan gates.
5. GitHub Actions repeats the core gates for every push and pull request.
