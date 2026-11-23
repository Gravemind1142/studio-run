//! Generates the task script for a `--cloud` run.

use crate::wrapper::lua_string;

const TEMPLATE: &str = include_str!("wrapper.luau");

/// Open Cloud's limit on a task script.
pub const MAX_SCRIPT_BYTES: usize = 4 * 1024 * 1024;

/// Build the task script around `source`. `chunk_name` (e.g. `test.luau`)
/// names the ModuleScript the source is compiled as.
pub fn generate(source: &[u8], chunk_name: &str, capture_print: bool) -> String {
    let source = source.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(source);
    // Replace SOURCE last, so placeholder text inside the user's code is never touched.
    TEMPLATE
        .replacen("__SR_CHUNK__", &lua_string(chunk_name.as_bytes()), 1)
        .replacen(
            "__SR_CAPTURE__",
            if capture_print { "true" } else { "false" },
            1,
        )
        .replacen("__SR_SOURCE__", &lua_string(source), 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_every_placeholder() {
        let w = generate(b"print(1)", "test.luau", true);
        assert!(!w.contains("__SR_"), "unfilled placeholder");
        assert!(w.contains(r#"local SOURCE = "print(1)""#));
        assert!(w.contains(r#"local CHUNK_NAME = "test.luau""#));
        assert!(w.contains("local CAPTURE_PRINT = true"));
        assert!(w.starts_with("--[[studio-run cloud wrapper]]"));
        // The wrapper's own printed events mustn't be confused with its source.
        assert!(!w.contains(crate::events::SENTINEL));
    }

    #[test]
    fn placeholder_text_in_user_code_is_untouched() {
        let w = generate(b"print('__SR_CHUNK__')", "test.luau", false);
        assert!(w.contains("__SR_CHUNK__"));
        assert!(w.contains("local CAPTURE_PRINT = false"));
    }

    #[test]
    fn strips_utf8_bom() {
        let w = generate(b"\xEF\xBB\xBFprint(1)", "test.luau", true);
        assert!(w.contains(r#"local SOURCE = "print(1)""#));
    }

    #[test]
    fn source_starts_on_the_modules_first_line() {
        // Line numbers in errors are the file's only if nothing precedes the source.
        let w = generate(b"x", "t.luau", true);
        assert!(w.contains(r#"module.Source = "return function(...) " .. SOURCE .. "\nend""#));
    }
}
