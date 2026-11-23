# Event format (`--format jsonl`)

With `--format jsonl`, studio-run prints every event as one JSON object per
line. Nothing is filtered, and the stream is the same locally and with
`--cloud`. Use it to build tooling on top of studio-run; for people reading a
terminal, [text format](../guide/output.md) is easier.

```json
{"seq":1,"kind":"start","host":"local","chunk":"test.luau"}
{"seq":2,"kind":"output","source":"script","level":"print","message":"hello"}
{"seq":5,"kind":"output","source":"other","level":"warn","message":"..."}
{"seq":6,"kind":"output","source":"other","level":"info","message":"joined","context":{"userId":1}}
{"seq":7,"kind":"output","source":"script","level":"error","async":true,"message":"test.luau:3: boom\ntest.luau, line 3\n"}
{"seq":9,"kind":"result","ok":true,"exitCode":0,"reason":"returned"}
```

Output is streamed while a local script runs. With `--cloud`, it all arrives
when the task finishes.

## Events

| `kind` | When |
|---|---|
| `start` | The script is about to run. |
| `output` | The script, or something else in Studio, printed a message. |
| `result` | The script finished. Always the last event when there is one. |
| `error` | studio-run itself failed (`--cloud` only: see `code` below). It explains a `125` exit. |

If the run times out or never reports a result, there is no `result` event;
use the exit code.

`seq` is a number that increases through the run. It may skip numbers, so
don't rely on it being consecutive.

## Fields

| Field | On | Values |
|---|---|---|
| `seq` | every event | The event's position in the run. |
| `kind` | every event | `start`, `output`, `result`, `error`. |
| `chunk` | `start` | The script's chunk name, its file name (e.g. `test.luau`). |
| `host` | `start` | `local` or `cloud`. |
| `universeId`, `placeId` | `start` | The place the script ran in, when it's a published one. |
| `placeVersion`, `taskId` | `start` | `--cloud`: the place version and the task. |
| `source` | `output` | `script` or `other`. See [Output](../guide/output.md#script-and-other-output). |
| `level` | `output` | `print`, `info`, `warn`, `error`. |
| `message` | `output` | The text. A script's own output can be any length; `other` output is cut off by Studio at about 100,000 characters per message. |
| `async` | `output` | `true` for an error in a thread the script spawned. These are reported but don't fail the run. |
| `context` | `output` | A structured log message's context, e.g. `{"userId":1}` from `LogService:Info("joined", {userId = 1})`. Only when it isn't empty. Roblox flattens it: values are strings, numbers or booleans, and anything else arrives as text (`Vector3` → `"1, 2, 3"`, an Instance → its name). |
| `ok` | `result` | `true` unless the script threw an error or didn't compile. A script that returns `false` is still `ok`, with `exitCode` 1: check `exitCode` for pass or fail. |
| `exitCode` | `result` | The exit code derived from the script's return value, which is also studio-run's exit code. See [Exit codes](cli.md#exit-codes). |
| `reason` | `result` | `returned`, `error`, `compile`. |
| `code` | `error` | `--cloud` failures: `auth`, `permission`, `not_found`, `invalid_request`, `rate_limited`, `server`, `http`, `network`, `task_failed`, `binary_output_lost`. |

Fields that don't apply to an event are left out.

## Compatibility

This is a public interface. Between the wrapper and studio-run, events carry
a protocol version (`PROTOCOL`, currently `1`), and studio-run rejects events
of any other version. The version isn't part of the lines it prints, so don't
look for a `v` field.

A change to the format needs a `PROTOCOL` bump in both `src/wrapper.luau` and
`src/events.rs` (see [CONTRIBUTING.md](../../CONTRIBUTING.md#pull-requests)),
and a CHANGELOG entry. To be safe, ignore fields and `kind` values you don't
recognise, so that new ones don't break your parser.

The wrapper also sends `binary` events carrying the script's binary output.
studio-run collects them for `--binary-output` and never prints them. How the
events travel is in [design.md](../internals/design.md#events).
