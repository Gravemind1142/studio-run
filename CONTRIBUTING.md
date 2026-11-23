# Contributing

Thanks for helping out. Bug reports, fixes and new findings about Studio's CLI
are all welcome.

## Setup

You need Windows 10 or 11, or macOS, plus Rust (stable) and Roblox Studio
installed and logged in. On Windows, also the Visual Studio C++ Build Tools.

```powershell
cargo build
cargo run -- doctor
```

## Tests

```powershell
cargo test                                # unit tests
cargo test --test studio -- --ignored     # end-to-end tests against real Studio (a few minutes)
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

Studio's CLI is mostly undocumented.
[docs/studio-behavior.md](docs/studio-behavior.md) records what we found out
and how, and `tests/behavior.rs` re-checks each finding:

```powershell
cargo test --test behavior -- --ignored --nocapture
```

If a check fails after a Studio update, on Windows or macOS, Studio has
changed: update the finding, the Studio version it was checked against, and
studio-run if it relied on the old behaviour. Run the suite on the platform
you have. macOS findings are recorded separately, because Studio's CLI can
differ there, including what `--background` does. To investigate something
new, write a probe in `tests/probes/` and run it on its own, with or without
the wrapper:

```powershell
cargo run -- run tests/probes/capabilities.luau --no-capture --output verbose
cargo run -- raw -- --task RunScript --runScriptFile C:\full\path\probe.luau --outputFile C:\full\path\out.log --quitAfterExecution
```

Once it tells you something studio-run depends on, add a check for it to
`tests/behavior.rs` and a row to the findings.

## Testing against Open Cloud

The cloud host needs your own test experience and API key, set in a
gitignored `.env` file (copy `.env.example`). [docs/cloud-testing.md](docs/cloud-testing.md) walks
through the setup. If your change touches the cloud host (`src/cloud/`), run
`cargo test --test cloud -- --ignored --test-threads=1` and say in the pull
request that it passed; if it touches the local host, run `tests/studio.rs` as
above.

## Pull requests

- Keep each pull request to one change.
- Add an entry under `[Unreleased]` in [CHANGELOG.md](CHANGELOG.md) for
  anything users would notice.
- Update [README.md](README.md) if you change options, output or exit codes.
  The `jsonl` event format is a public interface; changing it needs a
  `PROTOCOL` bump in both `src/wrapper.luau` and `src/events.rs`.

Releases are described in the README's "Releasing" section.
