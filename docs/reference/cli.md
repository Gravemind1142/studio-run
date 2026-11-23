# Command-line reference

```sh
studio-run run <script.luau> [options] [-- <extra Studio args>]
studio-run doctor [--cloud] [options]
studio-run api-dump <out.json> [--kind v2|full|luau] [options]
```

The commands are the same on Windows and macOS (and Linux, with `--cloud`).
For a walkthrough, see [Running scripts](../guide/running-scripts.md).

## `run`

Runs a script and prints its output. The exit code is the script's (see
[Exit codes](#exit-codes)).

The **Host** column says where an option applies: `both`, `local` (Studio on
Windows or macOS) or `cloud` (`--cloud`).

| Option | Default | Host | |
|---|---|---|---|
| `--place <file>` | empty baseplate | local | Open a local `.rbxl`/`.rbxlx` first. It's never saved. Can't be combined with `--place-id`. |
| `--place-id <id>` `--universe-id <id>` | | both | Locally, open a published place first. With `--cloud`, the place to run on. Both must be given together. |
| `--timeout <s>` | 300 | both | Fail with exit code 124 after this long. Locally, Studio is killed. With `--cloud` it's the task's own timeout, at most 300 (Roblox's limit). |
| `--grace <s>` | 5 | local | How long Studio may take to quit after the script finishes before it's stopped. |
| `--format text\|jsonl` | text | both | Text output, or one JSON event per line. See [Output](../guide/output.md) and [Events](events.md). |
| `--output script\|normal\|verbose` | normal | both | Which output to show in text format. See [Output](../guide/output.md). |
| `--arg <key>=<value>` | | both | Pass a string to the script as `ctx.args[key]`. Repeatable. See [Writing scripts](../guide/writing-scripts.md#script-context). |
| `--input <name>=<file>` | | both | Pass the file to the script as the buffer `ctx.inputs[name]`. Repeatable; at most 100 MiB in total. See [Binary input and output](../guide/writing-scripts.md#binary-input-and-output). |
| `--binary-output <file>` | | both | Write the buffer the script passes to `ctx.setBinaryOutput` to the file. At most 8 MiB locally, 256 MiB with `--cloud`. |
| `--no-capture` | | both | Don't replace `print`/`warn` in the script's environment. |
| `--trace-modules` | | both | Attribute required modules' output to them. See [Test runners and modules](../guide/test-runners.md#--trace-modules). |
| `--visible` | | local | Show Studio's window on your desktop, for debugging. |
| `--studio <exe>` | auto | local | Path to the Studio executable, or on macOS the `.app` bundle. Also `STUDIO_EXE`. |
| `--keep-temp` | | both | Keep what studio-run generated, and print where it is: locally the wrapper and output file; with `--cloud` the task script and the final task (without its download URL). |
| `--cloud` | | cloud | Run as an Open Cloud Luau Execution task instead. Also `STUDIO_RUN_HOST=cloud`. See [Running in the cloud](../guide/cloud.md). |
| `--place-version <n>` | latest | cloud | Run on this version of the place instead of its latest. |
| `--queue-timeout <s>` | 120 | cloud | Extra time allowed for the task to start. The run gives up after `--queue-timeout` + `--timeout`. |
| `--poll-interval <s>` | 2 | cloud | Seconds between task status checks. |
| `--api-key-env <name>` | `ROBLOX_API_KEY` | cloud | Environment variable holding the API key. The key is never accepted as an argument. |
| `-- <args>` | | local | Everything after `--` is passed to Studio as-is. |

### Local-only options under `--cloud`

- `--grace` and `--studio` are ignored.
- `--visible` and extra Studio arguments are usage errors (exit code 2).
- `--place <file>` is a usage error too: studio-run doesn't upload place
  files. See [Running in the cloud](../guide/cloud.md#why-no---place).

### Exit codes

| Code | Meaning |
|---|---|
| `0` | The script finished (and returned nothing, `true`, or `0`). |
| `1` | The script threw an uncaught error, didn't compile, or returned `false`. |
| *n* | The script returned the number *n*, or `{ exitCode = n }`. |
| `2` | Bad command-line usage. |
| `124` | Timed out. |
| `125` | studio-run problem: Studio not found, failed to start, or exited without the script reporting a result. With `--cloud`: the API refused a request (key, permissions, IDs), or the task failed without the script reporting a result. |

## `doctor`

```sh
studio-run doctor
studio-run doctor --cloud --universe-id <id> --place-id <id>
```

Checks your setup. Locally, it finds Studio and runs a test script in it.
With `--cloud`, it checks your cloud settings and key and runs a trivial task.
It takes `--studio`, or with `--cloud`, `--api-key-env`, `--universe-id` and
`--place-id`.

## `api-dump`

```sh
studio-run api-dump api.json [--kind v2|full|luau] [--timeout <s>]
```

Writes Roblox's API dump to the file in about a second, with no window.
Windows and macOS only.

| `--kind` | Studio flag | |
|---|---|---|
| `v2` (default) | `--apiV2` | The full API, V2 schema. |
| `full` | `--fullApi` | The full API. |
| `luau` | `--api` | The Luau scripting API. |

`--timeout` defaults to 120 seconds, and `--studio` works as for `run`.

## `raw`

```sh
studio-run raw [--timeout <s>] [--visible] -- <Studio args>
```

Launches hidden Studio with your own arguments and no wrapper, so there's no
output capture, event stream or exit code mapping. It's hidden from `--help`
and exists for investigating Studio's CLI and for the behaviour checks: see
[CONTRIBUTING.md](../../CONTRIBUTING.md#finding-out-what-studio-does).
Windows and macOS only.

## Environment variables

| Variable | Used for |
|---|---|
| `STUDIO_EXE` | The same as `--studio`. |
| `STUDIO_RUN_HOST` | `cloud` is the same as passing `--cloud`. |
| `ROBLOX_API_KEY` | The Open Cloud API key for `--cloud`, unless `--api-key-env` names another variable. |
