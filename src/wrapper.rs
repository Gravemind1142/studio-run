//! Generates the Luau wrapper that Studio actually runs.
//!
//! Studio's Luau can't read local files, so the user's source is embedded in
//! the wrapper as an escaped string literal.

const TEMPLATE: &str = include_str!("wrapper.luau");

pub struct WrapperOptions {
    /// Name shown in errors and tracebacks, e.g. `test.luau`.
    pub chunk_name: String,
    /// Replace `print`/`warn` in the script's env so its output is attributed to it.
    pub capture_print: bool,
    /// Load required ModuleScripts ourselves (opt-in; see README).
    pub trace_modules: bool,
}

/// Build the wrapper script around `source`.
pub fn generate(source: &[u8], opts: &WrapperOptions) -> String {
    let source = source.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(source);
    // Replace SOURCE last, so placeholder text inside the user's code is never touched.
    TEMPLATE
        .replacen("__SR_CHUNK__", &lua_string(format!("={}", opts.chunk_name).as_bytes()), 1)
        .replacen("__SR_CAPTURE__", bool_lit(opts.capture_print), 1)
        .replacen("__SR_TRACE__", bool_lit(opts.trace_modules), 1)
        .replacen("__SR_SOURCE__", &lua_string(source), 1)
}

fn bool_lit(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

/// Encode arbitrary bytes as a double-quoted Luau string literal.
/// Printable ASCII is kept; everything else becomes a 3-digit `\ddd` escape,
/// so any input (including invalid UTF-8) round-trips exactly.
pub fn lua_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('"');
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7E => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03}")),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> WrapperOptions {
        WrapperOptions { chunk_name: "test.luau".into(), capture_print: true, trace_modules: false }
    }

    #[test]
    fn escapes_special_bytes() {
        assert_eq!(lua_string(b"a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(lua_string(b"x\ny\tz\r"), r#""x\ny\tz\013""#);
        // Fixed-width escapes: "\0" followed by "1" must not read as "\01".
        assert_eq!(lua_string(b"\x001"), r#""\0001""#);
        // UTF-8 is escaped byte by byte.
        assert_eq!(lua_string("é".as_bytes()), r#""\195\169""#);
    }

    #[test]
    fn fills_every_placeholder() {
        let w = generate(b"print(1)", &opts());
        assert!(!w.contains("__SR_"), "unfilled placeholder");
        assert!(w.contains(r#"local SOURCE = "print(1)""#));
        assert!(w.contains(r#"local CHUNK_NAME = "=test.luau""#));
        assert!(w.contains("local CAPTURE_PRINT = true"));
        assert!(w.contains("local TRACE_MODULES = false"));
        assert!(w.starts_with("--[[studio-run wrapper]]"));
        // Studio echoes the wrapper into its log; the sentinel must not appear in it.
        assert!(!w.contains(crate::events::SENTINEL));
    }

    #[test]
    fn placeholder_text_in_user_code_is_untouched() {
        let w = generate(b"print('__SR_CHUNK__ __SR_TRACE__')", &opts());
        assert!(w.contains("__SR_CHUNK__ __SR_TRACE__"));
        assert!(w.contains(r#"local CHUNK_NAME = "=test.luau""#));
    }

    #[test]
    fn strips_utf8_bom() {
        let w = generate(b"\xEF\xBB\xBFprint(1)", &opts());
        assert!(w.contains(r#"local SOURCE = "print(1)""#));
    }
}
