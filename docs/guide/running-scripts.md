# Running scripts

```sh
studio-run run <script.luau> [options] [-- <extra Studio args>]
```

The script runs in Studio's edit mode with command-bar permissions, like
`--task RunScript` normally does. Studio is hidden: on Windows it runs on a
private desktop that is never shown, and on macOS it's started with its own
`--background` flag. See [design.md](../internals/design.md#hiding-studio) for
the details. Every option is in the [command-line reference](../reference/cli.md).

To run in the cloud instead, add `--cloud`: see
[Running in the cloud](cloud.md).

## Choosing a place

By default the script runs in an empty baseplate.

```sh
studio-run run smoke-test.luau --place game.rbxl
studio-run run smoke-test.luau --place-id 123 --universe-id 456
```

- `--place <file>` opens a local `.rbxl` or `.rbxlx` file first. studio-run
  never saves it.
- `--place-id` with `--universe-id` opens a published place first.

## Exit codes

The script's return value sets studio-run's exit code, so it works in CI and
shell scripts:

```sh
studio-run run smoke-test.luau --place game.rbxl && echo passed
```

`0` is success, `1` is an error or `false`, and a number the script returns
is passed through. `124` means the run timed out and `125` means studio-run
itself had a problem. The full table is in the
[reference](../reference/cli.md#exit-codes).

## Timeouts

`--timeout <s>` (default 300) kills Studio and fails the run with exit code
124. When the script finishes, Studio gets `--grace <s>` (default 5)
seconds to quit by itself before it's stopped.

## Debugging a run

- `--visible` shows Studio's window on your desktop.
- `--keep-temp` keeps the generated wrapper and the output file, and prints
  where they are.
- `--output verbose` also shows Studio's info-level messages. See
  [Output](output.md).
- `studio-run doctor` checks that studio-run can find and run Studio. See
  [Troubleshooting](troubleshooting.md).
- Arguments after `--` are passed to Studio as-is.

## Next

- [Writing scripts](writing-scripts.md): return values, `ctx`, `--arg`, binary
  input and output.
- [Output](output.md): what gets printed, and how to filter it.
- [Test runners and modules](test-runners.md), such as jest-roblox.
