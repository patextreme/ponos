//! Structured domain events: what one live agent session did, in the
//! order it happened.
//!
//! The session driver folds wire updates (via [`crate::core::turn`]) and
//! emits these through the [`EventSink`](crate::core::ports::EventSink)
//! port; renderers and future sinks format them. Payloads carry the
//! structured facts (ids, kinds, statuses, counts) so a TUI can track
//! state without parsing display strings — all formatting (truncation,
//! budgets, prefixes, colors) belongs to the sink implementation.

use agent_client_protocol::schema::v1::ToolKind;

/// One event from a live agent session.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// A prompt turn was sent; `text` is the raw prompt.
    Prompt { text: String },
    /// A chunk of the agent's streamed message text. `message_break`
    /// marks that tool-call activity ended the previous message run
    /// before this chunk (message-boundary metadata; the line renderer
    /// ignores it).
    TextDelta {
        delta: String,
        message_break: bool,
    },
    /// One rendered tool line, folded by the session's tool policy
    /// (transition dedup + duration).
    ToolLine(ToolLine),
    /// A plan update: entries in order.
    Plan { entries: Vec<PlanEntry> },
    /// Context-window usage report.
    Usage { used: u64, size: u64 },
    /// One line of agent subprocess stderr.
    StderrLine { line: String },
    /// Runtime lifecycle diagnostic (session readiness, config changes,
    /// typed-result setup, teardown notes).
    Lifecycle { message: String },
    /// The verdict for one typed-result submission. `late` marks a
    /// structurally valid submission that arrived with no turn in flight
    /// (dropped, not an error).
    ResultVerdict { accepted: bool, late: bool },
    /// The turn's stream ended: flush any partial line buffers.
    TurnEnd,
}

/// One folded tool-call line: the fully formatted body plus the
/// structured facts it was built from.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolLine {
    /// The call's id (update correlation key).
    pub id: String,
    /// The effective title: the announced title, or the raw call id for
    /// updates that preceded their announcement.
    pub title: String,
    /// The call's folded kind, when one was carried.
    pub kind: Option<ToolKind>,
    /// The status whose transition rendered this line.
    pub status: String,
    /// The formatted line body (`tool: <title> [<peek>] [(<status>,
    /// <duration>)]`).
    pub body: String,
}

/// One plan entry.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanEntry {
    pub status: PlanStatus,
    pub content: String,
}

/// Plan entry status (protocol-agnostic; sinks render their own marker).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
    Other,
}
