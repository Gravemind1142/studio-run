//! Generates the Luau wrapper that Studio actually runs.
//!
//! Studio's Luau can't read local files, so the user's source and any
//! `--input` files are embedded in the wrapper as escaped string literals.

use crate::binary::Input;

use std::collections::BTreeMap;

const TEMPLATE: &str = include_str!("wrapper.luau");

/// What the script gets as its first vararg (`local ctx = ...`), in the same
/// shape on both hosts.
pub struct ScriptContext<'a> {
    /// `"local"` or `"cloud"`.
    pub host: &'static str,
    /// From `--arg key=value`.
    pub args: &'a BTreeMap<String, String>,
    pub universe_id: Option<u64>,
    pub place_id: Option<u64>,
    pub place_version: Option<u64>,
}

impl ScriptContext<'_> {
    /// The context as a Luau table constructor.
    pub fn to_lua(&self) -> String {
        let args: Vec<String> = self
            .args
            .iter()
            .map(|(k, v)| {
                format!(
                    "[{}] = {}",
                    lua_string(k.as_bytes()),
                    lua_string(v.as_bytes())
                )
            })
            .collect();
        let num = |n: Option<u64>| n.map_or("nil".to_string(), |n| n.to_string());
        format!(
            "{{ host = {}, version = {}, args = {{ {} }}, place = {{ universeId = {}, placeId = {}, version = {} }} }}",
            lua_string(self.host.as_bytes()),
            lua_string(env!("CARGO_PKG_VERSION").as_bytes()),
            args.join(", "),
            num(self.universe_id),
            num(self.place_id),
            num(self.place_version),
        )
    }
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub struct WrapperOptions {
    /// Name shown in errors and tracebacks, e.g. `test.luau`.
    pub chunk_name: String,
    /// Replace `print`/`warn` in the script's env so its output is attributed to it.
    pub capture_print: bool,
    /// Load required ModuleScripts ourselves (opt-in; see docs/guide/test-runners.md).
    pub trace_modules: bool,
    /// Send the script's binary output back (`--binary-output`).
    pub binary_output: bool,
    /// The script's context, as a Luau table ([`ScriptContext::to_lua`]).
    pub context: String,
}

/// Build the wrapper script around `source`, with `inputs` embedded.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub fn generate(source: &[u8], inputs: &[Input], opts: &WrapperOptions) -> String {
    let source = source.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(source);
    let small = TEMPLATE
        .replacen(
            "__SR_CHUNK__",
            &lua_string(format!("={}", opts.chunk_name).as_bytes()),
            1,
        )
        .replacen("__SR_CAPTURE__", bool_lit(opts.capture_print), 1)
        .replacen("__SR_TRACE__", bool_lit(opts.trace_modules), 1)
        .replacen("__SR_BINARY_OUTPUT__", bool_lit(opts.binary_output), 1);
    // The context (with --arg values), inputs and source go in last, in one
    // pass, so placeholder text inside them is never touched and the
    // (possibly large) literals are copied once.
    let inputs = inputs_table(inputs);
    let source = lua_string(source);
    fill_once(
        &small,
        &[
            ("__SR_CONTEXT__", &opts.context),
            ("__SR_INPUTS__", &inputs),
            ("__SR_SOURCE__", &source),
        ],
    )
}

/// Replaces each placeholder, which must appear once in `template`, with its
/// value, without looking inside the values.
fn fill_once(template: &str, parts: &[(&str, &str)]) -> String {
    let mut at: Vec<(usize, &str, &str)> = parts
        .iter()
        .map(|&(p, v)| (template.find(p).expect("placeholder in template"), p, v))
        .collect();
    at.sort_by_key(|&(i, _, _)| i);
    let len = template.len() + parts.iter().map(|(_, v)| v.len()).sum::<usize>();
    let mut out = String::with_capacity(len);
    let mut pos = 0;
    for (i, p, v) in at {
        out.push_str(&template[pos..i]);
        out.push_str(v);
        pos = i + p.len();
    }
    out.push_str(&template[pos..]);
    out
}

pub fn bool_lit(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

/// `{ ["name"] = "bytes", ... }`
fn inputs_table(inputs: &[Input]) -> String {
    let mut out = String::from("{");
    for i in inputs {
        out.push_str("\n\t[");
        out.push_str(&lua_string(i.name.as_bytes()));
        out.push_str("] = ");
        out.push_str(&lua_string(&i.data));
        out.push(',');
    }
    if !inputs.is_empty() {
        out.push('\n');
    }
    out.push('}');
    out
}

/// Encode arbitrary bytes as a double-quoted Luau string literal.
/// Printable ASCII is kept; everything else becomes a 3-digit `\ddd` escape,
/// so any input (including invalid UTF-8) round-trips exactly. `@` is escaped
/// too: Studio echoes the wrapper into its log, so embedded text must never
/// contain the event sentinel.
pub fn lua_string(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 4 + 2);
    out.push(b'"');
    for &b in bytes {
        match b {
            b'\\' => out.extend_from_slice(br#"\\"#),
            b'"' => out.extend_from_slice(br#"\""#),
            b'\n' => out.extend_from_slice(br#"\n"#),
            b'\t' => out.extend_from_slice(br#"\t"#),
            b'@' => out.extend_from_slice(br#"\064"#),
            0x20..=0x7E => out.push(b),
            _ => out.extend_from_slice(&[b'\\', b'0' + b / 100, b'0' + b / 10 % 10, b'0' + b % 10]),
        }
    }
    out.push(b'"');
    String::from_utf8(out).expect("the literal is ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> WrapperOptions {
        WrapperOptions {
            chunk_name: "test.luau".into(),
            capture_print: true,
            trace_modules: false,
            binary_output: false,
            context: "{}".into(),
        }
    }

    #[test]
    fn context_is_a_luau_table() {
        let mut args = BTreeMap::new();
        args.insert("b".to_string(), "x\"y".to_string());
        args.insert("a".to_string(), "1".to_string());
        let ctx = ScriptContext {
            host: "local",
            args: &args,
            universe_id: Some(1),
            place_id: Some(2),
            place_version: None,
        };
        assert_eq!(
            ctx.to_lua(),
            format!(
                r#"{{ host = "local", version = "{}", args = {{ ["a"] = "1", ["b"] = "x\"y" }}, place = {{ universeId = 1, placeId = 2, version = nil }} }}"#,
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn placeholder_text_in_args_is_untouched() {
        let mut o = opts();
        o.context = r#"{ args = { x = "__SR_SOURCE__", y = "__SR_INPUTS__" } }"#.into();
        let w = generate(b"print(1)", &[], &o);
        assert!(w.contains(r#"local SOURCE = "print(1)""#));
        assert!(w.contains("local INPUTS = {}"));
        assert!(w.contains(r#"x = "__SR_SOURCE__", y = "__SR_INPUTS__""#));
    }

    #[test]
    fn escapes_special_bytes() {
        assert_eq!(lua_string(b"a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(lua_string(b"x\ny\tz\r"), r#""x\ny\tz\013""#);
        // Fixed-width escapes: "\0" followed by "1" must not read as "\01".
        assert_eq!(lua_string(b"\x001"), r#""\0001""#);
        // UTF-8 is escaped byte by byte.
        assert_eq!(lua_string("é".as_bytes()), r#""\195\169""#);
        assert_eq!(lua_string(b"\xff"), r#""\255""#);
        assert_eq!(lua_string(b"a@b"), r#""a\064b""#);
    }

    #[test]
    fn fills_every_placeholder() {
        let w = generate(b"print(1)", &[], &opts());
        assert!(!w.contains("__SR_"), "unfilled placeholder");
        assert!(w.contains(r#"local SOURCE = "print(1)""#));
        assert!(w.contains(r#"local CHUNK_NAME = "=test.luau""#));
        assert!(w.contains("local CAPTURE_PRINT = true"));
        assert!(w.contains("local TRACE_MODULES = false"));
        assert!(w.contains("local SEND_BINARY_OUTPUT = false"));
        assert!(w.contains("local INPUTS = {}"));
        assert!(w.contains("local CONTEXT = {}"));
        assert!(w.starts_with("--[[studio-run wrapper]]"));
        // Studio echoes the wrapper into its log; the sentinel must not appear in it.
        assert!(!w.contains(crate::events::SENTINEL));
    }

    #[test]
    fn embeds_inputs() {
        let inputs = [
            Input {
                name: "a.bin".into(),
                data: b"\x00\xff".to_vec(),
            },
            Input {
                name: "b".into(),
                data: b"@@studio-run@@{\"v\":1}".to_vec(),
            },
        ];
        let w = generate(b"x", &inputs, &opts());
        assert!(w.contains("local INPUTS = {\n\t[\"a.bin\"] = \"\\000\\255\",\n"));
        assert!(!w.contains(crate::events::SENTINEL));
    }

    #[test]
    fn placeholder_text_in_user_code_is_untouched() {
        let w = generate(b"print('__SR_CHUNK__ __SR_TRACE__')", &[], &opts());
        assert!(w.contains("__SR_CHUNK__ __SR_TRACE__"));
        assert!(w.contains(r#"local CHUNK_NAME = "=test.luau""#));
        let inputs = [Input {
            name: "n".into(),
            data: b"__SR_SOURCE__".to_vec(),
        }];
        let w = generate(b"print(1)", &inputs, &opts());
        assert!(w.contains(r#"local SOURCE = "print(1)""#));
        assert!(w.contains(r#"["n"] = "__SR_SOURCE__""#));
    }

    #[test]
    fn strips_utf8_bom() {
        let w = generate(b"\xEF\xBB\xBFprint(1)", &[], &opts());
        assert!(w.contains(r#"local SOURCE = "print(1)""#));
    }
}
