# Contributing

Thanks for helping out. Bug reports, fixes and new findings about Studio's CLI
are all welcome.

## Setup

You need Windows 10 or 11, or macOS, plus Rust (stable) and Roblox Studio
installed and logged in. On Windows, also the Visual Studio C++ Build Tools.

```sh
cargo build
cargo run -- doctor
```

## Tests

```sh
cargo test                                # unit tests, and --cloud against a mock API
cargo test --test studio -- --ignored     # end-to-end tests against real Studio (a few minutes)
cargo test --test behavior -- --ignored   # check Studio still behaves as documented (~2 min)
cargo test --test cloud -- --ignored --test-threads=1            # end-to-end --cloud tests (~3 min)
cargo test --test cloud_behavior -- --ignored --test-threads=1   # check Open Cloud still behaves as documented
cargo test --test parity -- --ignored     # both hosts report the same events
```

The end-to-end tests in `tests/studio.rs` need Studio installed and logged in.
They run up to three hidden Studio instances at once.

CI (`.github/workflows/ci.yml`) runs on every pull request and push to `main`,
on Windows, macOS and Linux: `cargo fmt --check`, `cargo clippy -D warnings`,
the unit tests and a release build. It **can't** run the end-to-end tests in
`tests/studio.rs`, because its runners have no logged-in Studio. If your
change touches the wrapper (`src/wrapper.luau`, `src/wrapper.rs`), event
parsing (`src/events.rs`), the log reader (`src/logtail.rs`), launching
(`src/sandbox.rs`) or the run loop (`src/run.rs`), run them locally and say in
the pull request that they passed.

Before pushing:

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
```

## Finding out what Studio does

Studio's CLI is mostly undocumented.
[docs/internals/studio-behavior.md](docs/internals/studio-behavior.md) records
what we found out and how, [docs/internals/design.md](docs/internals/design.md)
explains how studio-run works around it, and `tests/behavior.rs` re-checks
each finding against the installed Studio. Run it after a Studio update:

```sh
cargo test --test behavior -- --ignored --nocapture
```

If a check fails after a Studio update, on Windows or macOS, Studio has
changed: update the finding, the Studio version it was checked against, and
studio-run if it relied on the old behaviour. Run the suite on the platform
you have. macOS findings are recorded separately, because Studio's CLI can
differ there, including what `--background` does. To investigate something
new, write a probe in `tests/probes/` and run it on its own, with or without
the wrapper:

```sh
cargo run -- run tests/probes/capabilities.luau --no-capture --output verbose
cargo run -- raw -- --task RunScript --runScriptFile C:\full\path\probe.luau --outputFile C:\full\path\out.log --quitAfterExecution
```

Once it tells you something studio-run depends on, add a check for it to
`tests/behavior.rs` and a row to the findings.

## Testing against Open Cloud

`cargo test` already checks `--cloud` against a local mock of the API
(`tests/cloud_mock.rs`), and CI runs that on every pull request. If you change
which requests the cloud host makes, or how it reads their responses, update
those tests to match, and the mock only to what
[docs/internals/cloud-behavior.md](docs/internals/cloud-behavior.md) says the
real API does.

Testing against the real API needs your own test experience and API key, set in a
gitignored `.env` file (copy `.env.example`).
[docs/internals/cloud-testing.md](docs/internals/cloud-testing.md) walks
through the setup. If your change touches the cloud host (`src/cloud/`), run
`cargo test --test cloud -- --ignored --test-threads=1` and say in the pull
request that it passed; if it touches the local host, run `tests/studio.rs` as
above. If it touches either wrapper or the events, also run
`cargo test --test parity -- --ignored`, which needs both Studio and the cloud
setup and checks that the two hosts report the same events.

`.github/workflows/cloud.yml` runs the real-API tests weekly and on manual
runs, with the maintainers' secrets, never on pull requests.

## Pull requests

- Keep each pull request to one change.
- Add an entry under `[Unreleased]` in [CHANGELOG.md](CHANGELOG.md) for
  anything users would notice.
- Update the docs if you change options, output or exit codes: the
  [command-line reference](docs/reference/cli.md), and the
  [guide](docs/README.md#guide) page the change belongs to. The `jsonl` event
  format is a public interface
  ([docs/reference/events.md](docs/reference/events.md)); changing it needs a
  `PROTOCOL` bump in both `src/wrapper.luau` and `src/events.rs`.

Releases are described in [docs/internals/releasing.md](docs/internals/releasing.md).
