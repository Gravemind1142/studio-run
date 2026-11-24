# Test runners and modules

## Test runners

A test runner is just a script: run it, and have it return whether the tests
passed. [examples/jest.luau](../../examples/jest.luau) does this for
[jest-roblox](https://github.com/Roblox/jest-roblox):

```sh
studio-run run examples/jest.luau --place tests.rbxl
```

Jest's reporter output appears in the `other` group (Jest's modules use the
real `print`), so leave `--output` at `normal`, the default. Jest's usual
entry script ends with `ProcessService:ExitAsync`. That service doesn't exist
under Studio's CLI, so `return` the result instead.

Jest loads each test file with `debug.loadmodule` if it's available, and
otherwise with `loadstring`. Neither host has `debug.loadmodule`, so Jest
uses `loadstring`:

- **Locally** it works as it is. The script runs with command-bar
  permissions, so modules can use `loadstring` whatever the place's
  LoadStringEnabled says.
- **With `--cloud`** the place must have **ServerScriptService →
  LoadStringEnabled** on (see [Setup](cloud.md#setup)). Otherwise every test
  file fails with `loadstring() is not available`.

Don't use `--trace-modules` with test runners. They have their own module
loader.

Server-only tests can only pass under [`--cloud`](cloud.md#differences-from-local-runs),
because local scripts run in Studio's edit mode. Use `ctx.host` to
[skip them locally](writing-scripts.md#script-context).

## `--trace-modules`

Normally your script's `require` is the real one, so a module's output is in
the `other` group. With `--trace-modules`, `require(moduleScript)` instead
loads the module's `Source` through studio-run. Each module gets the same
captured `print`/`warn` and its own `script`, so its output and errors are
in the `script` group, named after it (e.g. `ReplicatedStorage.Greeter:8`).

This is **not** the real `require`, so behaviour differs in edge cases:

- Modules are cached per run, but not shared with code that uses the real
  `require`.
- `require(assetId)` and non-ModuleScript arguments fall back to the real
  `require` (untraced).
- Functions running in a custom environment lose some Luau optimizations.
- Under `--cloud`, each module is compiled as the body of a function (tasks
  have no `loadstring`), so a compile error at the very end of a module is
  reported one line late, at `end`.
