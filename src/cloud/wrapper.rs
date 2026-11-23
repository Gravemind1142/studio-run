//! Generates the task script for a `--cloud` run.

use crate::wrapper::{bool_lit, lua_string};

const TEMPLATE: &str = include_str!("wrapper.luau");

/// Open Cloud's limit on a task script.
pub const MAX_SCRIPT_BYTES: usize = 4 * 1024 * 1024;

/// Build the task script around `source`. `chunk_name` (e.g. `test.luau`)
/// names the ModuleScript the source is compiled as. `trace_modules`: load
/// required ModuleScripts ourselves (opt-in; see docs/guide/test-runners.md). `binary_output`:
/// return the script's binary output (`--binary-output`). `context` is the
/// script's context as a Luau table ([`crate::wrapper::ScriptContext::to_lua`]).
pub fn generate(
    source: &[u8],
    chunk_name: &str,
    capture_print: bool,
    trace_modules: bool,
    binary_output: bool,
    context: &str,
) -> String {
    let source = source.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(source);
    // Replace SOURCE last, so placeholder text inside the user's code is never
    // touched; CONTEXT (with --arg values) just before it, for the same reason.
    TEMPLATE
        .replacen("__SR_CHUNK__", &lua_string(chunk_name.as_bytes()), 1)
        .replacen("__SR_CAPTURE__", bool_lit(capture_print), 1)
        .replacen("__SR_TRACE__", bool_lit(trace_modules), 1)
        .replacen("__SR_BINARY_OUTPUT__", bool_lit(binary_output), 1)
        .replacen("__SR_CONTEXT__", context, 1)
        .replacen("__SR_SOURCE__", &lua_string(source), 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_every_placeholder() {
        let w = generate(b"print(1)", "test.luau", true, false, true, "{}");
        assert!(!w.contains("__SR_"), "unfilled placeholder");
        assert!(w.contains(r#"local SOURCE = "print(1)""#));
        assert!(w.contains(r#"local CHUNK_NAME = "test.luau""#));
        assert!(w.contains("local CAPTURE_PRINT = true"));
        assert!(w.contains("local TRACE_MODULES = false"));
        assert!(w.contains("local SEND_BINARY_OUTPUT = true"));
        assert!(w.starts_with("--[[studio-run cloud wrapper]]"));
        // The wrapper's own printed events mustn't be confused with its source.
        assert!(!w.contains(crate::events::SENTINEL));
    }

    #[test]
    fn placeholder_text_in_user_code_is_untouched() {
        let w = generate(
            b"print('__SR_CHUNK__ __SR_TRACE__ __SR_CONTEXT__ __SR_BINARY_OUTPUT__')",
            "test.luau",
            false,
            true,
            false,
            "{}",
        );
        assert!(w.contains("__SR_CHUNK__ __SR_TRACE__ __SR_CONTEXT__ __SR_BINARY_OUTPUT__"));
        assert!(w.contains("local CAPTURE_PRINT = false"));
        assert!(w.contains("local TRACE_MODULES = true"));
    }

    #[test]
    fn strips_utf8_bom() {
        let w = generate(
            b"\xEF\xBB\xBFprint(1)",
            "test.luau",
            true,
            false,
            false,
            "{}",
        );
        assert!(w.contains(r#"local SOURCE = "print(1)""#));
    }

    #[test]
    fn source_starts_on_the_modules_first_line() {
        // Line numbers in errors are the file's (or module's) only if nothing
        // precedes the source.
        let w = generate(b"x", "t.luau", true, false, false, "{}");
        assert!(w.contains(r#"module.Source = "return function(...) " .. source .. "\nend""#));
        assert!(w.contains("compile(CHUNK_NAME, SOURCE)"));
        assert!(w.contains("compile(name, target.Source)"));
    }
}
