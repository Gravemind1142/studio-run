# Writing scripts

A script is a Luau file. Everything here works the same way locally and with
`--cloud`, except where noted. For what differs under `--cloud`, see
[Running in the cloud](cloud.md#differences-from-local-runs).

- **Return a value to set the exit code**: `return true`/`false`, a number,
  or `{ exitCode = n }`. This works for any script, test runners included.
- The script may **yield** (`task.wait`, promises, etc.); the run lasts until
  its main chunk returns.
- Threads the script spawns (`task.spawn`, event connections) are **not**
  waited for. Wait for your own background work before returning. Errors in
  spawned threads are reported (with `async: true`) but don't fail the run.
- Errors and tracebacks point at your file and line, e.g. `test.luau:12`.
- `script` is `nil`.
- A UTF-8 byte-order mark at the start of the file is ignored. Studio itself
  silently refuses to run a script that starts with one.
- The script's first vararg is a **context** table, `local ctx = ...`: see
  [Script context](#script-context).

## Script context

The script's first vararg is a table describing the run, the same shape
locally and under `--cloud`:

```luau
local ctx = ...
ctx.host     -- "local" or "cloud"
ctx.version  -- studio-run's version, e.g. "0.1.0"
ctx.args     -- { [string]: string }, from --arg key=value
ctx.place    -- { universeId: number?, placeId: number?, version: number? }
ctx.inputs   -- { [string]: buffer }, from --input name=<file>
ctx.setBinaryOutput(buf) -- written to --binary-output
```

`ctx.inputs` and `ctx.setBinaryOutput` are described in
[Binary input and output](#binary-input-and-output).

`--arg` splits at the first `=`, so values may contain `=`; giving the same
key twice is a usage error. Locally, `ctx.place` has the IDs only with
`--place-id`, and never a version. Under `--cloud` it has the universe, place
and the version the task ran on.

Use `ctx.host` to skip what only one host can do. For example, server-only
tests can only pass under `--cloud`, because Studio runs scripts in edit mode
(see [Differences from local runs](cloud.md#differences-from-local-runs)):

```luau
local ctx = ...
if ctx.host == "local" then
	print("skipping server-only tests")
end
```

Only the script itself gets the context; required modules don't.

## Binary input and output

`--input name=<file>` (repeatable) passes files to the script as buffers, and
`--binary-output <file>` writes one buffer back, the same way locally and with
`--cloud`:

```luau
local ctx = ...
local data = ctx.inputs.place -- from --input place=build.rbxl: a buffer
ctx.setBinaryOutput(buffer.fromstring("result")) -- a buffer or a string
```

```sh
studio-run run convert.luau --input place=build.rbxl --binary-output out.bin
```

- `ctx.inputs` maps each name to a buffer, and is empty without `--input`.
  Inputs are at most 100 MiB in total.
- The output is what the buffer holds when the script finishes. Calling
  `ctx.setBinaryOutput` again replaces it. It's at most 8 MiB locally and
  256 MiB with `--cloud`; a larger buffer is an error in the script.
- The file is written once the script reports a result, even if it failed,
  and is empty if the script set no output. If the script never reports a
  result (a timeout, or exit code 125), the file is left alone.
- Without `--binary-output`, `ctx.setBinaryOutput` still works, but the output
  isn't sent back.
- Locally, inputs are embedded in the generated wrapper, and the output comes
  back through Studio's log, base64-encoded, so a large output adds a few
  seconds to a run (8 MiB in and out takes about 16 s in all).

## Next

- [Output](output.md): what `print`, `warn` and errors turn into.
- [Test runners and modules](test-runners.md): running jest-roblox, and
  `--trace-modules`.
