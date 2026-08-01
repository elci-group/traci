---
name: traci
description: Operate Traci, the Rust observability-enforcement daemon, production-code scoper, terminal dashboard, and guarded LLM improvement pipeline. Use when Codex needs to audit Rust error paths, structured tracing, correlation propagation, spawned-task span continuity, swallowed errors, or opaque panics; restrict analysis to production code; estimate trace implementation complexity; configure styled human, TUI, JSON, JSONL, or SARIF output; run Traci continuously with notifications; explain TRC rules; or plan and execute a benchmark-gated `traci trace` code improvement.
---

# Traci

Use the installed `traci` CLI to enforce reconstructable telemetry in Rust projects. Treat policy checks as read-only diagnostics and `traci trace --apply` as an explicitly authorized code-changing operation.

## Establish context

1. Run `command -v traci` and `traci --version`.
2. Work from the target repository root so Traci can discover `traci.toml` upward from the first scan path.
3. Run `traci init` only when the user asks to configure Traci or the project has no policy. Do not overwrite an existing policy unless explicitly requested.
4. Read [references/configuration.md](references/configuration.md) before changing policy, notifications, model routing, or benchmark gates.

## Choose the workflow

- Audit current instrumentation: run `traci check`.
- Size an instrumentation or refactoring task: run `traci complexity`.
- Monitor a working tree: use `traci daemon`.
- Monitor interactively: use `traci tui`.
- Exclude tests and other non-runtime sources: add `--production-only`.
- Integrate with CI or another tool: select JSON, JSONL, or SARIF output.
- Understand a diagnostic: use `traci explain RULE`.
- Ask an LLM to improve code under regression gates: use `traci trace`, dry-run first.

## Audit instrumentation

Start with human-readable output:

```console
traci check src
```

Use the exit status in automation: `0` means policy passes, `2` means findings reached the configured failure threshold, and `1` means configuration or I/O failed.

Use machine output only when it has a consumer:

```console
traci check src --format json --fail-on warning
traci check src --format jsonl
traci check . --format sarif
```

Use `--production-only` for deployable-code scope across checks, complexity, daemon, TUI, and trace workflows. It excludes tests, benches, examples, fixtures, fuzz targets, snapshots, and test/spec filename conventions. Use `--all-code` only when non-production sources are intentionally in scope.

Human output auto-detects terminals and may use colour, emoji, and animation. Use `traci tui . --production-only` for the live dashboard. Use `--color never --no-emoji --no-animate` for deterministic human text; machine formats remain undecorated regardless of presentation settings.

Do not hide findings merely to make the check pass. Prefer real structured instrumentation. Use `// traci: allow` only for a narrow, reviewed exception on that line or the immediately following line.

For each material finding, report the rule, file and line, why incident reconstruction is impaired, and a concrete remediation. Inspect the source before proposing a fix.

## Estimate complexity

Run:

```console
traci complexity src
traci complexity src --format json
```

Interpret the `0..100` score as a routing approximation, not cyclomatic complexity. It combines code size, functions, branches, error propagation, async and concurrency boundaries, nesting, and project breadth. Use the aggregate score and hottest files to scope work and estimate model strength.

## Run the daemon

Use the detached daemon for normal repository monitoring:

```console
traci daemon start src
traci daemon status
traci daemon stop
```

Use `traci daemon run src --format jsonl` for foreground supervision and `--once` for a single daemon-style scan. `traci watch` is the foreground-compatible alias.

Use `traci tui src --production-only` for a foreground dashboard that refreshes when Rust files change. Use `--once` when capturing a static dashboard without a terminal.

The daemon stores its PID, atomic status, and logs under the configured `.traci/` state directory. Before starting another instance, check `traci daemon status`. After starting, verify status and state the monitored paths. Stop it only when requested or when the task explicitly owns its lifecycle.

Notifications may target the console, a trusted local command, or an HTTP webhook. Treat command and webhook changes as security-sensitive: inspect their exact destination, never place credentials in `traci.toml`, and obtain authorization before enabling a new external destination.

## Apply guarded LLM improvements

Always plan first without `--apply`:

```console
traci trace src --goal "preserve span context across spawned tasks"
traci trace src --goal "reduce opaque error paths" --format json
```

The plan computes complexity and shows the configured provider, selected low/medium/high model, and branch. Review this output, repository cleanliness, path scope, provider command, benchmark command, and regression limits.

Run the mutating pipeline only when the user has asked Traci to make or apply the improvement:

```console
traci trace src --goal "preserve span context across spawned tasks" --apply
```

The pipeline establishes a benchmark baseline, forks an isolated Git worktree, requests a unified diff from the configured provider, enforces path and patch-size limits, re-runs analysis and benchmarks, commits the candidate, and fast-forward merges only when every gate succeeds. A rejected benchmarked candidate may remain committed on its branch for inspection but is not merged.

After `--apply`, verify:

```console
git status --short
git log -1 --oneline
traci check src
```

Report the selected model and score, branch, benchmark result, diagnostic and complexity deltas, merge or rejection outcome, and any retained candidate branch. Never claim success solely because the provider returned a patch.

## Explain rules

Run `traci rules` to list policy and `traci explain TRC005` (substitute the reported code) for remediation guidance. Preserve Traci's distinction between errors, warnings, and critical dead zones when summarizing results.
