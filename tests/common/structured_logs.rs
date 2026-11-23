//! Checks for the structured-logging probes (tests/probes/structured_logs.luau
//! and tests/cloud_probes/structured_logs.luau), which report the same table
//! locally and in the cloud. Findings: docs/internals/studio-behavior.md, "Structured
//! logging".

use serde_json::{Value, json};

/// The entry for `message` in one of the probe's lists.
fn entry<'a>(list: &'a Value, message: &str) -> &'a Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|e| e["message"] == message || e["fields"]["string:message"]["value"] == message)
        .unwrap_or_else(|| panic!("no {message:?} in {list}"))
}

/// The probe's description of a context table, as `{ key: value }`.
fn context(described: &Value) -> Value {
    assert_eq!(
        described["type"], "table",
        "context isn't a table: {described}"
    );
    let fields = &described["fields"];
    if fields.as_array().is_some_and(|a| a.is_empty()) {
        return json!({}); // an empty table encodes as []
    }
    let mut out = serde_json::Map::new();
    for (k, v) in fields.as_object().unwrap() {
        let v = match v["type"].as_str().unwrap() {
            "string" | "number" | "boolean" => v["value"].clone(),
            other => json!(format!("<{other}>")),
        };
        // The probe prefixes keys with their type; all of them are strings here.
        let k = k
            .strip_prefix("string:")
            .unwrap_or_else(|| panic!("non-string key {k}"));
        out.insert(k.to_owned(), v);
    }
    Value::Object(out)
}

pub fn check(r: &Value) {
    let calls = &r["calls"];
    for (name, ok) in calls.as_object().unwrap() {
        if name != "error" {
            assert_eq!(ok, &json!(true), "{name} failed: {ok}");
        }
    }
    // LogService:Error raises, like error(), instead of logging.
    assert_eq!(calls["error"], "error: SLOG error");

    let out = &r["messageOut"];
    let got: Vec<&str> = out
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["message"].as_str().unwrap())
        .collect();
    assert_eq!(
        got,
        [
            "SLOG output",
            "SLOG info",
            "SLOG warn",
            "SLOG log-warning",
            "SLOG info-no-context",
            "SLOG info-empty-context",
            "SLOG bad-context-string",
            "SLOG bad-context-function",
            "SLOG cyclic-context",
            "SLOG plain-print",
            "SLOG plain-warn",
            "SLOG logger-info",
            "SLOG logger-error",
            "SLOG child-warn",
        ],
        "MessageOut messages (the text never includes the context)"
    );
    for e in out.as_array().unwrap() {
        assert_eq!(e["extraArgs"], 0, "{e}");
    }
    let level = |m: &str| entry(out, m)["messageType"].as_str().unwrap().to_owned();
    assert_eq!(level("SLOG output"), "Enum.MessageType.MessageOutput");
    assert_eq!(level("SLOG info"), "Enum.MessageType.MessageInfo");
    assert_eq!(level("SLOG warn"), "Enum.MessageType.MessageWarning");
    assert_eq!(level("SLOG log-warning"), "Enum.MessageType.MessageWarning");
    assert_eq!(level("SLOG logger-error"), "Enum.MessageType.MessageError");

    // The context arrives flattened to strings, numbers and booleans.
    let full = context(&entry(out, "SLOG info")["context"]);
    assert_eq!(full["str"], "s");
    assert_eq!(full["num"], 1.5);
    assert_eq!(full["bool"], true);
    assert_eq!(full["1"], "array-key", "number keys become strings");
    assert_eq!(full["vec"], "1, 2, 3");
    assert_eq!(full["inst"], "ProbePart", "an Instance becomes its Name");
    assert!(
        full["nested"].as_str().unwrap().starts_with("table: 0x"),
        "nested tables aren't kept: {full}"
    );
    assert_eq!(full.as_object().unwrap().len(), 7, "{full}");
    assert_eq!(
        context(&entry(out, "SLOG bad-context-function")["context"]),
        json!({ "f": "<function>" })
    );
    assert!(
        context(&entry(out, "SLOG cyclic-context")["context"])["self"]
            .as_str()
            .unwrap()
            .starts_with("table: 0x")
    );
    // Everything else gets an empty table, never nil.
    for m in [
        "SLOG info-no-context",
        "SLOG info-empty-context",
        "SLOG bad-context-string",
        "SLOG plain-print",
        "SLOG plain-warn",
    ] {
        assert_eq!(context(&entry(out, m)["context"]), json!({}), "{m}");
    }
    // A Logger's messages carry only their own context: nothing names the logger.
    assert_eq!(
        context(&entry(out, "SLOG logger-info")["context"]),
        json!({ "who": "logger" })
    );

    // Loggers: one per name, and a logger's MessageOut includes its children's.
    assert_eq!(
        r["logger"],
        json!({ "name": "ProbeLogger", "fullPath": "ProbeLogger", "className": "Logger",
                "childName": "Child", "childFullPath": "ProbeLogger.Child",
                "sameLoggerTwice": true })
    );
    let messages = |list: &Value| -> Vec<String> {
        list.as_array()
            .unwrap()
            .iter()
            .map(|e| e["message"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        messages(&r["loggerMessageOut"]),
        ["SLOG logger-info", "SLOG logger-error", "SLOG child-warn"]
    );
    assert_eq!(messages(&r["childMessageOut"]), ["SLOG child-warn"]);

    // Structured errors never reach ScriptContext.Error.
    assert_eq!(r["scriptContextError"], json!([]));

    // GetLogHistory keeps the context, and names the logger.
    let history = &r["history"];
    let h = &entry(history, "SLOG child-warn")["fields"];
    assert_eq!(h["string:loggerFullPath"]["value"], "ProbeLogger.Child");
    assert_eq!(context(&h["string:context"]), json!({ "who": "child" }));
    let h = &entry(history, "SLOG info")["fields"];
    assert!(h.get("string:loggerFullPath").is_none(), "{h}");
    assert_eq!(context(&h["string:context"]), full);
    let h = &entry(history, "SLOG plain-print")["fields"];
    assert!(
        h.get("string:context").is_none(),
        "print has no context in history: {h}"
    );
}

/// Checks the events of tests/fixtures/structured_logs.luau, from either host.
pub fn check_fixture(events: &[Value]) {
    let other: Vec<&Value> = events
        .iter()
        .filter(|e| e["kind"] == "output" && e["source"] == "other")
        .collect();
    let find = |start: &str| -> &Value {
        other
            .iter()
            .find(|e| e["message"].as_str().is_some_and(|m| m.starts_with(start)))
            .unwrap_or_else(|| panic!("no {start:?} among {other:?}"))
    };
    let small = find("small");
    assert_eq!(small["level"], "info");
    assert_eq!(small["context"], json!({ "n": 1, "s": "x", "b": true }));
    assert!(small.get("contextJson").is_none(), "{small}");

    let big = find("big");
    assert_eq!(big["level"], "warn");
    assert_eq!(big["message"], "big");
    assert_eq!(
        big["context"],
        json!({ "blob": "é".repeat(400), "other": "end" })
    );

    let long = find("mmm");
    assert_eq!(long["message"], format!("{}END", "m".repeat(1500)));
    assert_eq!(long["context"], json!({ "k": "long message" }));

    let logger = find("from logger");
    assert_eq!(logger["level"], "error");
    assert_eq!(logger["context"], json!({ "who": "logger" }));

    for m in ["no context", "empty context"] {
        assert!(find(m).get("context").is_none(), "{}", find(m));
    }
    let plain = events
        .iter()
        .find(|e| e["source"] == "script" && e["message"] == "plain")
        .expect("the script's own print");
    assert!(plain.get("context").is_none(), "{plain}");
}
