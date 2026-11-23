//! Printing a run's events, shared by the local and cloud hosts.

use std::io::Write;

use crate::events::{self, Assembler, Event};

/// Exit code for bad command-line usage.
pub const EXIT_USAGE: i32 = 2;
/// Exit code when the run timed out.
pub const EXIT_TIMEOUT: i32 = 124;
/// Exit code when the host couldn't run the script or never reported a result.
pub const EXIT_TOOL_ERROR: i32 = 125;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// Output as text, like Studio's Output window.
    Text,
    /// One JSON event per line on stdout.
    Jsonl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFilter {
    /// Only output from the script itself.
    Script,
    /// The script's output, plus print/warn/error from anything else.
    Normal,
    /// Everything, including Studio's info messages.
    Verbose,
}

/// Where the script ran, added to the `start` event.
#[derive(Clone, Debug, Default)]
pub struct StartInfo {
    pub host: &'static str,
    pub universe_id: Option<u64>,
    pub place_id: Option<u64>,
    pub place_version: Option<u64>,
    pub task_id: Option<String>,
}

/// Turns wrapper output lines into printed events.
pub struct Printer {
    pub format: Format,
    pub filter: OutputFilter,
    pub start: StartInfo,
    asm: Assembler,
    last_seq: u64,
    pub result: Option<Event>,
}

impl Printer {
    pub fn new(format: Format, filter: OutputFilter, start: StartInfo) -> Printer {
        Printer {
            format,
            filter,
            start,
            asm: Assembler::default(),
            last_seq: 0,
            result: None,
        }
    }

    /// One line of output that may carry an event (others are ignored).
    pub fn line(&mut self, line: &str) {
        match events::parse_line(line) {
            None => {}
            Some(Ok(ev)) => {
                if let Some(ev) = self.asm.push(ev) {
                    self.event(ev);
                }
            }
            Some(Err(e)) => note(&format!("ignoring malformed event: {e}")),
        }
    }

    pub fn finish(&mut self) {
        if let Some(ev) = self.asm.finish() {
            self.event(ev);
        }
    }

    fn event(&mut self, mut ev: Event) {
        // `result` ends the run; anything after it is the host shutting down.
        if self.result.is_some() {
            return;
        }
        self.last_seq = self.last_seq.max(ev.seq);
        if ev.kind == "start" {
            let s = &self.start;
            ev.host = Some(s.host.to_string());
            ev.universe_id = s.universe_id;
            ev.place_id = s.place_id;
            ev.place_version = s.place_version;
            ev.task_id = s.task_id.clone();
        }
        match self.format {
            Format::Jsonl => print_json(&ev),
            Format::Text => self.text(&ev),
        }
        if ev.kind == "result" {
            self.result = Some(ev);
        }
    }

    /// Reports a host failure: an `error` event in jsonl, a note in text.
    /// It precedes a 125 exit.
    pub fn error(&mut self, code: &str, message: &str) {
        match self.format {
            Format::Jsonl => {
                self.last_seq += 1;
                print_json(&Event {
                    seq: self.last_seq,
                    kind: "error".into(),
                    message: Some(message.into()),
                    code: Some(code.into()),
                    ..Default::default()
                });
                note(message);
            }
            Format::Text => note(message),
        }
    }

    /// The script's exit code, if it reported a result.
    pub fn exit_code(&self) -> Option<i32> {
        self.result
            .as_ref()
            .and_then(|r| r.exit_code)
            .map(|c| c as i32)
    }

    fn text(&self, ev: &Event) {
        if ev.kind != "output" {
            return;
        }
        let from_script = ev.source.as_deref() == Some("script");
        let level = ev.level.as_deref().unwrap_or("print");
        let show = match self.filter {
            OutputFilter::Script => from_script,
            OutputFilter::Normal => from_script || level != "info",
            OutputFilter::Verbose => true,
        };
        if !show {
            return;
        }
        let msg = ev.message.as_deref().unwrap_or("");
        match level {
            "warn" => eprintln!("warning: {msg}"),
            "error" => eprintln!("error: {msg}"),
            _ => println!("{msg}"),
        }
    }
}

fn print_json(ev: &Event) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", serde_json::to_string(ev).unwrap_or_default());
}

/// Tool messages go to stderr, prefixed, so stdout stays the script's.
pub fn note(msg: &str) {
    eprintln!("studio-run: {msg}");
}
