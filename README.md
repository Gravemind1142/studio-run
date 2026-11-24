# studio-run

Run Luau scripts in Roblox Studio on Windows or macOS **without Studio taking
over your screen**, and get the script's output and a meaningful exit code back.

```sh
studio-run run smoke-test.luau --place game.rbxl
```

On Windows, Studio runs on a private desktop that is never shown, so it can't
steal focus or interrupt you, and several runs can go at once. On macOS, Studio
is started with its own `--background` flag, which keeps it out of the
foreground. The commands are the same in either shell.

The same script can also run in the cloud, as an
[Open Cloud Luau Execution](https://create.roblox.com/docs/cloud/reference/features/luau-execution)
task in a place you've published, with the same output format and exit codes.
This works on Windows, Linux and macOS, with no Studio needed, so it suits CI:

```sh
studio-run run smoke-test.luau --cloud
```

## Requirements

- **Local runs:** Windows 10 or 11, or macOS, and Roblox Studio, installed and
  **logged in at least once** (studio-run reuses your saved login).
- **`--cloud` runs:** Windows, macOS or Linux, an experience to run in and an
  Open Cloud API key. See [Running in the cloud](docs/guide/cloud.md).

## Install

With [Foreman](https://github.com/Roblox/foreman), add it to your project's
`foreman.toml` and run `foreman install`:

```toml
[tools]
studio-run = { github = "Gravemind1142/studio-run", version = "0.2.1" }
```

[Rokit](https://github.com/rojo-rbx/rokit) and
[Aftman](https://github.com/LPGhatguy/aftman) can install the same releases:

```sh
rokit add Gravemind1142/studio-run
```

Each [GitHub release](https://github.com/Gravemind1142/studio-run/releases)
also has a zip per platform (Windows x86_64, macOS aarch64 and x86_64, Linux
x86_64) with the executable in it (`studio-run.exe` on Windows, `studio-run`
elsewhere), and a `SHA256SUMS` file. The Linux build runs `--cloud` only. Or
build from source with Rust (stable; on Windows, also the Visual Studio C++
Build Tools):

```sh
cargo install --git https://github.com/Gravemind1142/studio-run
```

Then check everything works:

```sh
studio-run doctor
```

## A first script

```luau
-- smoke-test.luau
local ctx = ...
print("running on", ctx.host)

-- Return true to exit 0, false to exit 1, or a number to exit with it.
return game:GetService("Workspace") ~= nil
```

```sh
studio-run run smoke-test.luau
```

The script's `print` and `warn` go to stdout and stderr, and its return value
becomes studio-run's exit code, so it works in CI and shell scripts.

## Documentation

**Guide**

- [Running scripts](docs/guide/running-scripts.md): places, exit codes,
  timeouts and debugging.
- [Writing scripts](docs/guide/writing-scripts.md): return values, the `ctx`
  table, `--arg`, binary input and output.
- [Output](docs/guide/output.md): what gets printed, and how to filter it.
- [Running in the cloud](docs/guide/cloud.md): `--cloud` setup and differences.
- [Test runners and modules](docs/guide/test-runners.md): jest-roblox and
  `--trace-modules`.
- [Troubleshooting and limitations](docs/guide/troubleshooting.md)

**Reference**

- [Command-line reference](docs/reference/cli.md): commands, options,
  environment variables and exit codes.
- [Event format](docs/reference/events.md): `--format jsonl`.

**Internals:** [design notes](docs/internals/design.md), plus what we found
out about [Studio's CLI](docs/internals/studio-behavior.md) and
[Open Cloud](docs/internals/cloud-behavior.md). The full list is in
[docs/README.md](docs/README.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to contribute, and
[CHANGELOG.md](CHANGELOG.md) for what changed in each release.

## License

[MIT](LICENSE)
