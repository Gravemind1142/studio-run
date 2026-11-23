# Documentation

New here? Start with the [README](../README.md) for install and a first run.

## Guide

How to use studio-run, in the order you'll probably need it.

- [Running scripts](guide/running-scripts.md): the `run` command, choosing a
  place, exit codes, timeouts and debugging.
- [Writing scripts](guide/writing-scripts.md): return values, the `ctx`
  table, `--arg`, and binary input and output.
- [Output](guide/output.md): the `script` and `other` groups, `--output`
  levels, and JSON Lines.
- [Running in the cloud](guide/cloud.md): `--cloud`, API key setup, and how
  cloud runs differ.
- [Test runners and modules](guide/test-runners.md): jest-roblox and
  `--trace-modules`.
- [Troubleshooting and limitations](guide/troubleshooting.md): `doctor`, login
  problems and known limitations.

## Reference

- [Command-line reference](reference/cli.md): every command, option,
  environment variable and exit code.
- [Event format](reference/events.md): every field of `--format jsonl`.

## Internals

For contributors and the curious. See also [CONTRIBUTING.md](../CONTRIBUTING.md).

- [Design notes](internals/design.md): why studio-run works the way it does.
- [How Studio's CLI behaves](internals/studio-behavior.md): what we found out
  about it, and how.
- [How Open Cloud Luau Execution behaves](internals/cloud-behavior.md): the
  same for the cloud.
- [Testing against Open Cloud](internals/cloud-testing.md): setting up your
  own test experience and key.
- [Releasing](internals/releasing.md): cutting a release.
