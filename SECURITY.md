# Security policy

## Supported versions

Security fixes target the latest release and the `main` branch. Traci is still
in the `0.1.x` series, so users should upgrade to the newest patch before
reporting behavior that may already be fixed.

## Report a vulnerability

Use GitHub's private vulnerability reporting for
`github.com/elci-group/traci`. Do not open a public issue for a suspected
vulnerability or include credentials, repository contents, webhook secrets, or
provider prompts in a report.

Include the affected version, operating system, minimal reproduction, expected
trust boundary, and potential impact. Reports involving command execution,
webhook destinations, Git worktrees, patch-scope escape, or credential exposure
receive highest priority.

## Security model

`traci check` and `traci complexity` are local read-only analysis operations.
`traci trace` remains read-only unless `--apply` is explicit.

The following settings cross trust boundaries and must contain only reviewed
values:

- notification commands and webhook URLs;
- trace provider commands and model output;
- benchmark commands;
- Git repository and worktree paths.

Keep credentials in the process environment or an external credential store,
not in `traci.toml`. Keep trace regression limits strict and
`require_clean = true` unless a deliberate, reviewed workflow requires
otherwise.

Traci's lexical analysis is a policy aid, not a sandbox, compiler, or guarantee
that inspected code is safe to execute.
