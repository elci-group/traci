# Traci configuration

Read this reference before editing `traci.toml`. Unknown keys and unknown rule codes are errors; validate changes with `traci check`.

## Analysis

```toml
[analysis]
fail_on = "error"
trace_window = 6
production_only = false
require_correlation_id = true
correlation_fields = ["correlation_id", "request_id", "trace_id"]
ignore = ["target", ".git", ".kaptaind", ".traci"]
enabled_rules = ["TRC001", "TRC002", "TRC003", "TRC004", "TRC005", "TRC006", "TRC007", "TRC008"]
```

`fail_on` accepts `info`, `warning`, `error`, or `critical`. `production_only = true` excludes tests, benches, examples, fixtures, fuzz targets, snapshots, and test/spec filename conventions from file-based operations. Keep ignores narrow and repository-relative.

## Watch, daemon, and notifications

```toml
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
```

The interval must be at least 50 ms. A notification command receives JSON on stdin plus `TRACI_EVENT`, `TRACI_SEVERITY`, `TRACI_TITLE`, and `TRACI_BODY`. A webhook receives HTTP JSON through `curl`. Failed notification channels are recorded without stopping enforcement.

Only configure trusted commands. Keep secrets in the command's environment or credential store, not this file. Confirm authorization and destination before enabling a webhook.

`ui.color` accepts `auto`, `always`, or `never`. Auto mode decorates only terminals, respects `NO_COLOR`, and leaves redirected and machine output clean. CLI presentation flags override these values for one invocation.

## LLM routing and merge gates

```toml
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

`provider = "command"` sends the prompt to a trusted local command on stdin and exports `TRACI_MODEL`, `TRACI_GOAL`, and `TRACI_COMPLEXITY_SCORE`. `provider = "ollama"` runs `ollama run <selected-model>`. The provider must return a unified diff beginning with `diff --git`.

Use real provider model identifiers in `low_model`, `medium_model`, and `high_model`. Choose thresholds so low handles localized instrumentation, medium handles multi-function control flow, and high handles broad async or distributed-context changes. Keep `require_clean = true` and non-positive diagnostic/complexity increases unless the user knowingly approves a different gate.

Treat `provider_command` and `benchmark_command` as executable code. Inspect both before `traci trace --apply`. Do not weaken regression limits merely to merge a rejected candidate.
