# Contributing

Contributions that make failure paths easier to reconstruct are welcome.

## Set up

Traci declares Rust 1.74 as its minimum supported version. Stable Rust with
`rustfmt` and `clippy` is sufficient for normal development.

```console
git clone https://github.com/elci-group/traci.git
cd traci
cargo test --all-targets
```

## Before changing code

1. Read [the architecture guide](docs/ARCHITECTURE.md).
2. Identify the rule, output format, or lifecycle boundary affected.
3. Add a focused test that fails for the behavior being changed.
4. Keep the analyzer dependency-free and avoid unsafe Rust.

Generated or fixture-only patterns belong under test paths. Production code
must remain clean under `traci check . --production-only --fail-on warning`.

## Required gates

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
env RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps
cargo run -- check . --production-only --fail-on warning
deliver --spec deliver.toml --strict
```

Do not weaken `traci.toml`, add broad ignores, or add unexplained
`// traci: allow` comments to make a gate pass. An allowance must be narrow and
state why propagation or fallback behavior is observable at the boundary.

## Pull requests

Keep each pull request centered on one behavior. Include:

- the incident-reconstruction problem being solved;
- before/after CLI output when user-visible behavior changes;
- tests for success, failure, and machine-output modes;
- any compatibility or performance tradeoff;
- confirmation that the required gates pass.

Changes to notification commands, webhooks, trace providers, benchmark
execution, worktree isolation, or patch scoping deserve an explicit security
review because they cross process or network boundaries.
