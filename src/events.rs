//! Parsing the wrapper's `@@studio-run@@{json}` event lines.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const SENTINEL: &str = "@@studio-run@@";
pub const PROTOCOL: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    #[serde(default, skip_serializing)]
    pub v: u32,
    pub seq: u64,
    /// `start`, `output`, `result`, or `error` (a host failure, from studio-run itself).
    /// The local wrapper also sends `binary` (base64 pieces of the script's binary
    /// output), which studio-run collects and never prints.
    pub kind: String,
    /// `output`: `script` (the user's script, or traced modules) or `other`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `output`: `print`, `info`, `warn` or `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// `output`: the context table of a structured log message
    /// (`LogService:Info(message, context)`), when it isn't empty.
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub context: Option<Map<String, Value>>,
    /// How the wrapper sends `context`: as JSON text, so a long one can be
    /// split into pieces like `message`. Parsed into `context` once joined.
    #[serde(default, skip_serializing)]
    pub context_json: Option<String>,
    /// `output`: error came from a thread the script spawned.
    #[serde(default, rename = "async", skip_serializing_if = "Option::is_none")]
    pub is_async: Option<bool>,
    /// `start`: the script's chunk name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk: Option<String>,
    /// `start`: `local` or `cloud`. Added by studio-run, not the wrapper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// `start`, cloud: the universe, place, place version and task the script ran in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub universe_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// `error`: a stable code for a host failure (`auth`, `rate_limited`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// `result`: whether the script finished without an uncaught error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    /// `result`: exit code derived from the script's return value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    /// `result`: `returned`, `error` or `compile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// More pieces of this message follow (long messages are split).
    #[serde(default, skip_serializing)]
    pub more: Option<bool>,
}

/// Extract an event from a line of Studio output, if it carries one.
pub fn parse_line(line: &str) -> Option<Result<Event, String>> {
    let start = line.find(SENTINEL)? + SENTINEL.len();
    let json = line[start..].trim_end();
    if !json.starts_with("{\"") {
        return None; // mentions the sentinel, but isn't an event
    }
    Some(match serde_json::from_str::<Event>(json) {
        Ok(ev) if ev.v != PROTOCOL => Err(format!("unsupported protocol version {}", ev.v)),
        Ok(ev) => Ok(ev),
        Err(e) => Err(format!("bad event ({e}): {}", truncate(json, 120))),
    })
}

fn truncate(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Joins split messages and drops events already seen (the same event can
/// arrive from both the live Studio log and the final output file).
#[derive(Default)]
pub struct Assembler {
    last_seq: u64,
    pending: Option<Event>,
}

impl Assembler {
    pub fn push(&mut self, ev: Event) -> Option<Event> {
        if ev.seq <= self.last_seq {
            return None;
        }
        self.last_seq = ev.seq;
        let more = ev.more.unwrap_or(false);
        let combined = match self.pending.take() {
            Some(mut first) => {
                append(&mut first.message, ev.message);
                append(&mut first.context_json, ev.context_json);
                first
            }
            None => ev,
        };
        if more {
            self.pending = Some(combined);
            None
        } else {
            Some(complete(combined))
        }
    }

    /// Whatever is left half-assembled when output ends.
    pub fn finish(&mut self) -> Option<Event> {
        self.pending.take().map(complete)
    }
}

fn append(to: &mut Option<String>, piece: Option<String>) {
    if let Some(piece) = piece {
        to.get_or_insert_with(String::new).push_str(&piece);
    }
}

/// A whole event: parses `contextJson` into `context`. A context that
/// doesn't parse (cut off when output ended) is dropped.
fn complete(mut ev: Event) -> Event {
    ev.more = None;
    ev.context = ev
        .context_json
        .take()
        .and_then(|json| serde_json::from_str(&json).ok());
    ev
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(seq: u64, msg: &str, more: bool) -> Event {
        let line = format!(
            r#"2026-01-01T00:00:00Z,1.0,abc,6,Info [FLog::CreatorOutput] @@studio-run@@{{"v":1,"seq":{seq},"kind":"output","source":"script","level":"print","message":{},"more":{more}}}"#,
            serde_json::to_string(msg).unwrap()
        );
        parse_line(&line).unwrap().unwrap()
    }

    #[test]
    fn parses_log_line() {
        let e = ev(3, "hi", false);
        assert_eq!(e.seq, 3);
        assert_eq!(e.message.as_deref(), Some("hi"));
        assert_eq!(e.source.as_deref(), Some("script"));
    }

    #[test]
    fn ignores_lines_without_sentinel() {
        assert!(parse_line("plain output").is_none());
        assert!(parse_line(r#"local SENTINEL = "@@studio-run@@""#).is_none());
    }

    #[test]
    fn rejects_other_protocol_versions() {
        let r = parse_line(r#"@@studio-run@@{"v":99,"seq":1,"kind":"start"}"#).unwrap();
        assert!(r.unwrap_err().contains("protocol"));
    }

    #[test]
    fn joins_split_messages() {
        let mut a = Assembler::default();
        assert_eq!(a.push(ev(1, "hel", true)), None);
        assert_eq!(a.push(ev(2, "lo ", true)), None);
        let done = a.push(ev(3, "world", false)).unwrap();
        assert_eq!(done.message.as_deref(), Some("hello world"));
        assert_eq!(done.seq, 1);
    }

    #[test]
    fn drops_duplicates() {
        let mut a = Assembler::default();
        assert!(a.push(ev(1, "a", false)).is_some());
        assert!(a.push(ev(1, "a", false)).is_none());
        assert!(a.push(ev(2, "b", false)).is_some());
    }

    fn line(json: &str) -> Event {
        parse_line(&format!("@@studio-run@@{json}"))
            .unwrap()
            .unwrap()
    }

    #[test]
    fn joins_and_parses_split_context() {
        let mut a = Assembler::default();
        let first = r#"{"v":1,"seq":1,"kind":"output","source":"other","level":"info","message":"hi","more":true}"#;
        assert_eq!(a.push(line(first)), None);
        let second = r#"{"v":1,"seq":2,"kind":"output","source":"other","level":"info","contextJson":"{\"a\":1,","more":true}"#;
        assert_eq!(a.push(line(second)), None);
        let third = r#"{"v":1,"seq":3,"kind":"output","source":"other","level":"info","contextJson":"\"b\":\"x\"}"}"#;
        let done = a.push(line(third)).unwrap();
        assert_eq!(done.message.as_deref(), Some("hi"));
        assert_eq!(
            serde_json::to_value(&done.context).unwrap(),
            serde_json::json!({ "a": 1, "b": "x" })
        );
        assert_eq!(done.context_json, None);
        let json = serde_json::to_string(&done).unwrap();
        assert!(json.contains(r#""context":{"a":1,"b":"x"}"#), "{json}");
        assert!(!json.contains("contextJson"), "{json}");
    }

    #[test]
    fn context_is_only_read_from_context_json() {
        let e = line(r#"{"v":1,"seq":1,"kind":"output","context":{"a":1}}"#);
        assert_eq!(e.context, None);
        let mut a = Assembler::default();
        let done = a
            .push(line(
                r#"{"v":1,"seq":1,"kind":"output","contextJson":"{\"a\""}"#,
            ))
            .unwrap();
        assert_eq!(done.context, None, "a cut-off context is dropped");
    }

    #[test]
    fn serializes_async_field_name() {
        let mut e = ev(1, "x", false);
        e.is_async = Some(true);
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains(r#""async":true"#));
        assert!(!json.contains("more"));
    }
}
