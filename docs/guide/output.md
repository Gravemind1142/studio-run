# Output

## `script` and `other` output

Every piece of output is put in one of two groups:

- **`script`**: your script's `print`/`warn` calls, its uncaught error, and
  errors in threads it spawned.
- **`other`**: everything else Studio prints while the script runs. That
  includes modules your script `require`s (they use the real `print`), your
  installed plugins, and Studio itself.

Use [`--trace-modules`](test-runners.md#--trace-modules) to move required
modules' output into `script`.

## Text format

`--format text` is the default. `print` output goes to stdout. Warnings and
errors go to stderr, prefixed with `warning:` / `error:`. studio-run's own
messages go to stderr, prefixed with `studio-run:`.

`--output` picks what's shown:

- `script`: only the script's own output.
- `normal` (the default): the script's output, plus `print`/`warn`/`error`
  from other sources. Output from test runners and modules shows up here,
  along with some plugin noise.
- `verbose`: also Studio's info-level messages.

A structured log message's context is printed after it as compact JSON:
`joined {"userId":1}`. Structured log messages count as `other`, even when your
script logs them, so `--output script` hides them, and `LogService:Info`
messages only show with `--output verbose`.

Output is streamed while the script runs. With `--cloud`, it arrives when the
task finishes, because Roblox only makes it available then. With
`--output verbose`, the task's progress shows as `studio-run:` notes.

## JSON Lines format

`--format jsonl` prints every event as one JSON object per line, with no
filtering and no `--output` levels:

```json
{"seq":2,"kind":"output","source":"script","level":"print","message":"hello"}
{"seq":9,"kind":"result","ok":true,"exitCode":0,"reason":"returned"}
```

Every field is described in the [event reference](../reference/events.md).

## Message size

Your script's own output can be any length. Output in the `other` group is cut
off by Studio itself at about 100,000 characters per message.
