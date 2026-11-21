# Contributing

Thanks for helping out. Bug reports, fixes and new findings about Studio's CLI
are all welcome.

## Setup

You need Windows 10 or 11, Rust (stable, `x86_64-pc-windows-msvc`), the Visual
Studio C++ Build Tools, and Roblox Studio installed and logged in.

```powershell
cargo build
cargo run -- doctor
```

## Tests

```powershell
cargo test                      # unit tests
cargo test -- --include-ignored # plus end-to-end tests against real Studio (~1 min)
```

CI runs formatting, clippy, the unit tests and a release build. It **can't**
run the end-to-end tests in `tests/studio.rs`, because its runners have no
logged-in Studio. If your change touches the wrapper (`src/wrapper.luau`,
`src/wrapper.rs`), event parsing (`src/events.rs`), the log reader
(`src/logtail.rs`), launching (`src/sandbox.rs`) or the run loop (`src/run.rs`),
run them locally and say in the pull request that they passed.

Before pushing:

```powershell
cargo fmt
cargo clippy --all-targets -- -D warnings
```

## Finding out what Studio does

Studio's CLI is mostly undocumented. `experiments/` holds the probe scripts we
used, and [PLAN.md](PLAN.md) records the findings. If you find out something
new, such as a behaviour change in a Studio update, add it to the findings
table along with the Studio version you tested.

Run a probe with:

```powershell
cargo run -- run experiments/probe.luau --no-capture --output verbose
```

## Pull requests

- Keep each pull request to one change.
- Add an entry under `[Unreleased]` in [CHANGELOG.md](CHANGELOG.md) for
  anything users would notice.
- Update [README.md](README.md) if you change options, output or exit codes.
  The `jsonl` event format is a public interface; changing it needs a
  `PROTOCOL` bump in both `src/wrapper.luau` and `src/events.rs`.

Releases are described in the README's "Releasing" section.
