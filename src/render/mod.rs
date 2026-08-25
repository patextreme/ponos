//! Terminal rendering of streaming agent output with per-session attribution.
//!
//! No TUI: plain stdout writes with a `[agent/session]` text prefix and a
//! per-session ANSI color assigned round-robin from a small palette.
//! `--no-color` drops the color codes; `--quiet` suppresses everything but
//! script `print` output; `-vv` additionally passes agent stderr through.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::sync::Mutex;

use crate::core::events::{PlanEntry, PlanStatus, SessionEvent};
use crate::core::ports::EventSink;
use crate::core::text::{LINE_BUDGET, truncate_visible};

/// Palette of distinct ANSI foreground hues, cycled per session label.
const PALETTE: [&str; 6] = [
    "\x1b[36m", // cyan
    "\x1b[33m", // yellow
    "\x1b[35m", // magenta
    "\x1b[32m", // green
    "\x1b[34m", // blue
    "\x1b[91m", // bright red
];

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

/// Local wall-clock time as `yyyy-mm-dd HH:MM:SS`, prefixed to every
/// rendered line. Taken at write time: the producers of display events
/// have no event time distinct from render time.
fn timestamp() -> String {
    let now = jiff::Zoned::now();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderOptions {
    /// `--quiet`: suppress streaming render and diagnostics.
    pub quiet: bool,
    /// `--no-color`: emit text prefixes without ANSI sequences.
    pub no_color: bool,
    /// `-vv`: pass agent subprocess stderr through.
    pub agent_stderr: bool,
    /// `--verbose`: runtime lifecycle diagnostics.
    pub verbose: bool,
}

impl RenderOptions {
    /// All output suppressed (useful for tests).
    pub fn quiet() -> Self {
        Self {
            quiet: true,
            ..Self::default()
        }
    }
}

/// A display event extracted from a `session/update` notification.
#[derive(Debug)]
pub enum DisplayEvent {
    /// A chunk of the agent's message text (streamed).
    Chunk(String),
    /// One rendered tool line: the fully formatted body (`tool: <title>`
    /// at the call's start, `tool: <title> (<status>, <duration>)` when it
    /// settles). Transition policy and duration are decided where the
    /// update stream is folded; the renderer is a dumb sink.
    Tool(String),
    /// Compact plan status list.
    Plan(String),
    /// Context-window usage line.
    Usage { used: u64, size: u64 },
}

#[derive(Default)]
struct SessionBuf {
    /// Partial line buffered for the next newline (message chunks).
    partial: String,
}

struct Inner {
    out: BufWriter<std::io::Stdout>,
    styles: HashMap<String, String>,
    next_style: usize,
    bufs: HashMap<String, SessionBuf>,
}

/// Shared, thread-safe renderer.
pub struct Renderer {
    opts: RenderOptions,
    inner: Mutex<Inner>,
}

impl Renderer {
    pub fn new(opts: RenderOptions) -> Self {
        Self {
            opts,
            inner: Mutex::new(Inner {
                out: BufWriter::new(std::io::stdout()),
                styles: HashMap::new(),
                next_style: 0,
                bufs: HashMap::new(),
            }),
        }
    }

    pub fn options(&self) -> RenderOptions {
        self.opts
    }

    fn style_for(&self, inner: &mut Inner, label: &str) -> String {
        if self.opts.no_color {
            return String::new();
        }
        inner
            .styles
            .entry(label.to_string())
            .or_insert_with(|| {
                let code = PALETTE[inner.next_style % PALETTE.len()].to_string();
                inner.next_style += 1;
                code
            })
            .clone()
    }

    fn prefixed_line(&self, inner: &mut Inner, label: &str, line: &str) {
        let ts = timestamp();
        if self.opts.no_color {
            let _ = writeln!(inner.out, "{ts} [{label}] {line}");
        } else {
            let style = self.style_for(inner, label);
            let _ = writeln!(inner.out, "{DIM}{ts}{RESET} [{label}]{style} {line}{RESET}");
        }
    }

    /// `[ponos]` diagnostic line (lifecycle, script log): timestamped but
    /// never label-colored.
    fn ponos_line(&self, inner: &mut Inner, msg: &str) {
        let ts = timestamp();
        if self.opts.no_color {
            let _ = writeln!(inner.out, "{ts} [ponos] {msg}");
        } else {
            let _ = writeln!(inner.out, "{DIM}{ts}{RESET} [ponos] {msg}");
        }
    }

    /// Write one complete prefixed line.
    pub fn line(&self, label: &str, text: &str) {
        if self.opts.quiet {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        for line in text.lines() {
            self.prefixed_line(&mut inner, label, line);
        }
        let _ = inner.out.flush();
    }

    /// Stream a message chunk; buffers partial lines so prefixes land on
    /// real lines. `flush` finishes any pending partial line (turn end).
    pub fn chunk(&self, label: &str, delta: &str, flush: bool) {
        if self.opts.quiet {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        let mut ready: Vec<String> = Vec::new();
        {
            let buf = inner.bufs.entry(label.to_string()).or_default();
            buf.partial.push_str(delta);
            while let Some(nl) = buf.partial.find('\n') {
                let line: String = buf.partial.drain(..=nl).collect();
                ready.push(line.trim_end_matches('\n').to_string());
            }
            if flush && !buf.partial.is_empty() {
                ready.push(std::mem::take(&mut buf.partial));
            }
        }
        for line in ready {
            self.prefixed_line(&mut inner, label, &line);
        }
        let _ = inner.out.flush();
    }

    /// Render a display event derived from a session update.
    pub fn event(&self, label: &str, event: DisplayEvent) {
        match event {
            DisplayEvent::Chunk(text) => self.chunk(label, &text, false),
            DisplayEvent::Tool(body) => self.line(label, &body),
            DisplayEvent::Plan(summary) => self.line(label, &summary),
            DisplayEvent::Usage { used, size } => {
                self.line(label, &format!("context: {used}/{size} tokens"))
            }
        }
    }

    /// `-vv`: pass one line of agent stderr through with attribution.
    pub fn agent_stderr(&self, label: &str, line: &str) {
        if !self.opts.agent_stderr || self.opts.quiet {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        self.prefixed_line(&mut inner, label, line);
        let _ = inner.out.flush();
    }

    /// `--verbose`: runtime lifecycle diagnostic.
    pub fn lifecycle(&self, msg: &str) {
        if self.opts.quiet || !self.opts.verbose {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        self.ponos_line(&mut inner, msg);
        let _ = inner.out.flush();
    }

    /// `ponos.log`: script-initiated diagnostic on stdout (not suppressed by
    /// `--quiet`, which only silences streaming render/diagnostics).
    pub fn script_log(&self, msg: &str) {
        let mut inner = self.inner.lock().unwrap();
        self.ponos_line(&mut inner, msg);
        let _ = inner.out.flush();
    }

    /// Flush buffered partial lines at turn end.
    pub fn flush_session(&self, label: &str) {
        if self.opts.quiet {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        if let Some(buf) = inner.bufs.get_mut(label)
            && !buf.partial.is_empty()
        {
            let line = std::mem::take(&mut buf.partial);
            self.prefixed_line(&mut inner, label, &line);
        }
        let _ = inner.out.flush();
    }
}

/// One-line preview of a prompt for the prompt line: whitespace runs
/// collapsed to single spaces, truncated to the shared visible-char
/// budget.
fn prompt_preview(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_visible(&collapsed, LINE_BUDGET).into_owned()
}

/// Render marker for one plan status (matches the streaming plan line).
fn plan_marker(status: PlanStatus) -> char {
    match status {
        PlanStatus::Pending => ' ',
        PlanStatus::InProgress => '>',
        PlanStatus::Completed => 'x',
        PlanStatus::Other => '?',
    }
}

/// Compact plan status list for the plan line.
fn plan_summary(entries: &[PlanEntry]) -> String {
    let rendered: Vec<String> = entries
        .iter()
        .map(|e| format!("[{}] {}", plan_marker(e.status), e.content))
        .collect();
    format!("plan: {}", rendered.join(" "))
}

/// The terminal renderer as an [`EventSink`]: structured session events
/// map onto the existing display-event handling; every byte of formatting
/// (truncation, prefixes, colors, gating) stays here.
impl EventSink for Renderer {
    fn emit(&self, label: &str, event: SessionEvent) {
        match event {
            SessionEvent::Prompt { text } => {
                self.line(label, &format!("prompt: {}", prompt_preview(&text)))
            }
            SessionEvent::TextDelta { delta, .. } => self.event(label, DisplayEvent::Chunk(delta)),
            SessionEvent::ToolLine(line) => self.event(label, DisplayEvent::Tool(line.body)),
            SessionEvent::Plan { entries } => {
                self.event(label, DisplayEvent::Plan(plan_summary(&entries)))
            }
            SessionEvent::Usage { used, size } => {
                self.event(label, DisplayEvent::Usage { used, size })
            }
            SessionEvent::StderrLine { line } => self.agent_stderr(label, &line),
            SessionEvent::Lifecycle { message } => self.lifecycle(&message),
            // A structurally valid submission with no turn in flight is
            // dropped (not errored); the one-line note is the only render.
            SessionEvent::ResultVerdict { late: true, .. } => self.lifecycle(&format!(
                "{label}: dropped late typed-result submission (no turn in flight)"
            )),
            // Verdicts ride the result channel to the bridge; nothing to
            // render on the terminal sink.
            SessionEvent::ResultVerdict { .. } => {}
            SessionEvent::TurnEnd => self.flush_session(label),
        }
    }

    fn script_log(&self, message: &str) {
        self.script_log(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `yyyy-mm-dd HH:MM:SS`: fixed width 19, digits and separators in
    /// the right slots (render-logging "Timestamp shape").
    #[test]
    fn timestamp_shape_is_date_prefixed() {
        let ts = timestamp();
        let b = ts.as_bytes();
        assert_eq!(ts.len(), 19, "fixed width: {ts:?}");
        let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
        assert!(digits(0..4) && b[4] == b'-', "year: {ts:?}");
        assert!(digits(5..7) && b[7] == b'-', "month: {ts:?}");
        assert!(digits(8..10) && b[10] == b' ', "day: {ts:?}");
        assert!(digits(11..13) && b[13] == b':', "hour: {ts:?}");
        assert!(digits(14..16) && b[16] == b':', "minute: {ts:?}");
        assert!(digits(17..19), "second: {ts:?}");
    }

    #[test]
    fn prompt_preview_collapses_and_truncates() {
        assert_eq!(
            prompt_preview("review\n  the\tauth\nmodule\n"),
            "review the auth module"
        );
        let long = "y".repeat(LINE_BUDGET + 10);
        assert_eq!(
            prompt_preview(&long),
            format!("{}\u{2026}", "y".repeat(LINE_BUDGET))
        );
    }

    #[test]
    fn plan_summary_renders_status_markers() {
        let entries = vec![
            PlanEntry {
                status: PlanStatus::Completed,
                content: "read the code".into(),
            },
            PlanEntry {
                status: PlanStatus::InProgress,
                content: "fix the bug".into(),
            },
        ];
        assert_eq!(
            plan_summary(&entries),
            "plan: [x] read the code [>] fix the bug"
        );
    }
}
