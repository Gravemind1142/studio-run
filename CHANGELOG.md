# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- macOS support. Hidden launches pass Studio's `--background` flag and run in
  a process group, instead of the Windows private desktop and job object.
  The next release also publishes `macos-aarch64` and `macos-x86_64` archives.

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
- [docs/studio-behavior.md](docs/studio-behavior.md), recording how Studio's
  CLI behaves, and `tests/behavior.rs`, which re-checks it against the
  installed Studio.
- Release builds for Windows x86_64, installable with Foreman, Rokit or Aftman.

[Unreleased]: https://github.com/Gravemind1142/studio-run/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Gravemind1142/studio-run/releases/tag/v0.1.0
