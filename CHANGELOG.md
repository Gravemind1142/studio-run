# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-11-23

### Added

- `--input name=<file>` (repeatable) and `--binary-output <file>`, on both
  hosts: scripts read inputs as buffers in `ctx.inputs[name]` and set their
  binary output with `ctx.setBinaryOutput(buf)`. Inputs are at most 100 MiB in
  total; the output at most 8 MiB locally and 256 MiB with `--cloud`.
- `--arg key=value` (repeatable), and a script context passed as the script's
  first vararg (`local ctx = ...`): `ctx.host` (`local` or `cloud`),
  `ctx.version`, `ctx.args` and `ctx.place` (universe, place and version
  where known). The same on both hosts.
- macOS support. Hidden launches pass Studio's `--background` flag and run in
  a process group, instead of the Windows private desktop and job object.
  Releases publish `macos-aarch64` and `macos-x86_64` archives.
- `--cloud` (or `STUDIO_RUN_HOST=cloud`): run the script as
  an Open Cloud Luau Execution task instead of in local Studio, with the same
  output formats and exit codes. Runs on the place's latest version or on
  `--place-version`. It never uploads place files: `--place` is a usage error
  with `--cloud`. New options `--api-key-env`, `--queue-timeout` and
  `--poll-interval`. `--trace-modules` works under `--cloud` too.
- `studio-run doctor --cloud` checks the cloud settings and key with a trivial
  task.
- Linux x86_64 builds, which run `--cloud` only: local Studio commands exit 2
  there. Releases publish a `linux-x86_64` archive.
- jsonl: the `start` event has `host` (`local` or `cloud`), and with `--cloud`
  the universe, place, place version and task. A new `error` event with a
  stable `code` explains a `125` exit under `--cloud`.
- A "cloud host" section in [docs/internals/design.md](docs/internals/design.md);
  [docs/internals/cloud-behavior.md](docs/internals/cloud-behavior.md), how Luau Execution tasks
  behave, re-checked by `tests/cloud_behavior.rs`; and `tests/cloud.rs`,
  end-to-end tests of `--cloud`. Both need a test experience and key of your
  own ([docs/internals/cloud-testing.md](docs/internals/cloud-testing.md)); a weekly workflow runs
  them against the real API. `tests/cloud_mock.rs` checks `--cloud` against a
  local mock of the API in every `cargo test`, so pull requests are covered
  without a key.

### Changed

- Releases publish one `SHA256SUMS` file instead of a `.sha256` file per zip.
  Foreman matches asset names by substring, so `…-windows-x86_64.zip.sha256`
  could be picked instead of the zip.
- `--place` together with `--place-id`, and `--place-id` without
  `--universe-id`, are now reported by studio-run (exit 2) rather than by the
  argument parser.
- The documentation is reorganised. The README is now a short introduction,
  and the rest lives in `docs/`: a guide, a command-line and event reference,
  and the internals (the existing design and behaviour notes, now in
  `docs/internals/`, plus the release steps). Links to the old README
  sections and to `docs/design.md` and the other notes need updating.

## [0.1.0]

First release.

### Added

- `studio-run run <script>`: runs a Luau script in Roblox Studio on a hidden
  Windows desktop, against the default baseplate, a local place file
  (`--place`) or a published place (`--place-id` / `--universe-id`).
- Exit code from the script's return value: nothing or `true` gives 0, `false`
  gives 1, a number gives that number, `{ exitCode = n }` gives n. Uncaught and
  compile errors give 1, a timeout gives 124, and a studio-run failure gives 125.
- Output streamed live from Studio's log, split into the script's own output
  and everything else, with levels kept. `--format jsonl` gives one JSON event
  per line.
- `--trace-modules` to capture output from required ModuleScripts too.
- `studio-run api-dump` to write Studio's API dump.
- `studio-run doctor` to check that Studio can run a script on this machine.
- A hidden `studio-run raw` command that launches hidden Studio with your own
  arguments and no wrapper, for investigating Studio's behaviour.
- [docs/internals/studio-behavior.md](docs/internals/studio-behavior.md), recording how Studio's
  CLI behaves, and `tests/behavior.rs`, which re-checks it against the
  installed Studio.
- Release builds for Windows x86_64, installable with Foreman, Rokit or Aftman.

[Unreleased]: https://github.com/Gravemind1142/studio-run/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Gravemind1142/studio-run/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Gravemind1142/studio-run/releases/tag/v0.1.0
