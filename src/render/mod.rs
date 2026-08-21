//! Terminal rendering of streaming agent output with per-session attribution.
//!
//! No TUI: plain stdout writes with a `[agent/session]` text prefix and a
//! per-session ANSI color assigned round-robin from a small palette.
//! `--no-color` drops the color codes; `--quiet` suppresses everything but
//! script `print` output; `-vv` additionally passes agent stderr through.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::sync::Mutex;

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
    /// One-line tool summary.
    Tool { title: String, status: String },
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
        if self.opts.no_color {
            let _ = writeln!(inner.out, "[{label}] {line}");
        } else {
            let style = self.style_for(inner, label);
            let _ = writeln!(inner.out, "[{label}]{style} {line}{RESET}");
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
            DisplayEvent::Tool { title, status } => {
                self.line(label, &format!("tool: {title} ({status})"))
            }
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
        let _ = writeln!(inner.out, "[ponos] {msg}");
        let _ = inner.out.flush();
    }

    /// `ponos.log`: script-initiated diagnostic on stdout (not suppressed by
    /// `--quiet`, which only silences streaming render/diagnostics).
    pub fn script_log(&self, msg: &str) {
        let mut inner = self.inner.lock().unwrap();
        let _ = writeln!(inner.out, "[ponos] {msg}");
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
