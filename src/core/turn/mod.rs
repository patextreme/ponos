//! Turn and tool-call fold semantics: the pure state machines that
//! accumulate one prompt turn's outcome from the ACP update stream.
//!
//! [`TurnFold`] tracks the turn's message runs and typed-result slot
//! (`begin_turn` / `break_message` / `settle_turn`); [`ToolFold`] holds
//! the per-session tool-call display policy — which tool updates deserve
//! a rendered line, with what input peek and duration. Kept here (where
//! the update stream is folded) rather than in the renderer, which stays
//! a dumb sink.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{ToolCallLocation, ToolKind, ToolCallUpdateFields};

use crate::core::text::{LINE_BUDGET, truncate_visible};
use crate::core::contract::SubmissionSink;

/// The in-flight turn's accumulator, folded on the connection's dispatch
/// loop (in wire order, before the response is delivered).
#[derive(Default)]
pub(crate) struct TurnFold {
    /// The turn's current message run: text streamed since the last
    /// tool-call activity (see `break_message`).
    pub(crate) text: String,
    /// The turn's last completed non-empty message run — the fallback
    /// when a turn ends on tool activity with no trailing message.
    prev_text: String,
    /// Whether a turn is currently in flight (submissions landing outside
    /// a turn are dropped as late).
    pub(crate) in_flight: bool,
    /// The turn's last accepted typed submission (last-wins).
    pub(crate) result: Option<serde_json::Value>,
    /// Per-session tool call display state. Deliberately outside
    /// `begin_turn`/`settle_turn`: entries live for the session lifetime
    /// so a repeat terminal status for an old id still dedups.
    pub(crate) tools: ToolFold,
}

impl TurnFold {
    /// A fold for a session whose cwd is `cwd` (drives peek path
    /// shortening).
    pub(crate) fn with_cwd(cwd: PathBuf) -> Self {
        Self {
            tools: ToolFold::with_cwd(cwd),
            ..Self::default()
        }
    }

    /// A turn starts: fresh text and a fresh slot, so a turn never
    /// observes the previous turn's state.
    pub(crate) fn begin_turn(&mut self) {
        self.in_flight = true;
        self.text.clear();
        self.prev_text.clear();
        self.result = None;
    }

    /// Tool-call activity ends the current message run: the run, when
    /// non-empty, becomes the last completed run. An agent that emits a
    /// tool call has, by construction, finished speaking, so tool updates
    /// are the only message boundary.
    pub(crate) fn break_message(&mut self) {
        if !self.text.is_empty() {
            self.prev_text = std::mem::take(&mut self.text);
        }
    }

    /// A turn settles: returns the turn's text — the current message
    /// run, falling back to the last completed non-empty run when the
    /// turn ends on tool activity with no trailing message — and the
    /// accepted submission. Both fields are drained; `discard` (a
    /// cancelled, timed-out, or failed turn) yields empty text and no
    /// submission.
    pub(crate) fn settle_turn(&mut self, discard: bool) -> (String, Option<serde_json::Value>) {
        self.in_flight = false;
        let text = std::mem::take(&mut self.text);
        let prev_text = std::mem::take(&mut self.prev_text);
        let submission = self.result.take();
        if discard {
            (String::new(), None)
        } else {
            (if text.is_empty() { prev_text } else { text }, submission)
        }
    }
}

/// The submission sink for the result channel: accept into the in-flight
/// turn's slot (last-wins), or report a late submission (no turn in
/// flight) so the channel can drop it with a lifecycle line.
pub(crate) fn submission_sink(fold: Arc<Mutex<TurnFold>>) -> SubmissionSink {
    Arc::new(move |value| {
        let mut fold = fold.lock().unwrap();
        if fold.in_flight {
            fold.result = Some(value);
            true
        } else {
            false
        }
    })
}

/// Peek-relevant fields shared by `tool_call` announcements and
/// `tool_call_update` payloads: the inputs the peek is synthesized from.
/// `None`/absent means "not carried by this message" (patch semantics).
#[derive(Default)]
pub(crate) struct PeekInputs<'a> {
    pub(crate) kind: Option<&'a ToolKind>,
    pub(crate) locations: Option<&'a [ToolCallLocation]>,
    pub(crate) raw_input: Option<&'a serde_json::Value>,
}

/// Display state for one tool call (keyed by call id).
#[derive(Default)]
struct ToolCallDisplay {
    /// Title learned from the call's `tool_call` announcement; `None`
    /// until one arrives — updates for ids that were never announced fall
    /// back to the raw call id.
    title: Option<String>,
    /// Input peek appended to rendered lines: kind-aware, synthesized from
    /// the folded state below. First non-empty candidate sticky — later
    /// data never overwrites an already-set peek.
    peek: Option<String>,
    /// Peek inputs folded from announcements and updates alike.
    kind: Option<ToolKind>,
    locations: Vec<ToolCallLocation>,
    raw_input: Option<serde_json::Value>,
    /// Duration anchor: the `in_progress` transition once one has
    /// rendered, otherwise the call's first observation.
    first_activity: Option<Instant>,
    /// Last status a line was rendered for; a transition that repeats it
    /// renders nothing.
    last_rendered: Option<String>,
}

impl ToolCallDisplay {
    /// Fold one message's peek inputs into the call's state, then
    /// synthesize the peek when none sticks yet (an absent kind or no
    /// derivable candidate leaves it unset for a later message to fill).
    fn learn_peek(&mut self, inputs: &PeekInputs<'_>, cwd: &Path, home: Option<&Path>) {
        if let Some(kind) = inputs.kind {
            self.kind = Some(*kind);
        }
        if let Some(locations) = inputs.locations {
            self.locations = locations.to_vec();
        }
        if let Some(raw) = inputs.raw_input.filter(|v| !v.is_null()) {
            self.raw_input = Some(raw.clone());
        }
        if self.peek.is_none() {
            let candidate = synthesize_peek(self, cwd, home);
            if candidate.is_some() {
                self.peek = candidate;
            }
        }
    }
}

/// Kind-aware peek candidate from a call's folded state, in priority
/// order (render-logging "Tool lines carry an input peek"):
///
/// 1. `execute` → the `command`/`cmd` string from the raw input;
/// 2. `read`/`edit`/`move`/`search`/`fetch`/`delete` → the first location
///    as `path[:line]`, shortened against the session cwd / `$HOME`;
/// 3. otherwise (including the above yielding nothing) → compact JSON of
///    the raw input.
///
/// The shared visible-char budget is applied to whichever candidate wins.
fn synthesize_peek(entry: &ToolCallDisplay, cwd: &Path, home: Option<&Path>) -> Option<String> {
    let candidate = match entry.kind.as_ref() {
        Some(ToolKind::Execute) => command_peek(entry.raw_input.as_ref()),
        Some(
            ToolKind::Read
            | ToolKind::Edit
            | ToolKind::Move
            | ToolKind::Search
            | ToolKind::Fetch
            | ToolKind::Delete,
        ) => location_peek(entry.locations.first(), cwd, home),
        _ => None,
    };
    candidate
        .or_else(|| json_peek(entry.raw_input.as_ref()))
        .map(|c| truncate_visible(&c, LINE_BUDGET).into_owned())
}

/// `command` or `cmd` string from an execute call's raw input.
fn command_peek(raw: Option<&serde_json::Value>) -> Option<String> {
    let raw = raw?;
    ["command", "cmd"]
        .iter()
        .find_map(|k| raw.get(k).and_then(serde_json::Value::as_str))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// First location as `path[:line]` with the path shortened for display.
fn location_peek(
    location: Option<&ToolCallLocation>,
    cwd: &Path,
    home: Option<&Path>,
) -> Option<String> {
    let loc = location?;
    let path = shorten_path(&loc.path, cwd, home);
    Some(match loc.line {
        Some(line) => format!("{path}:{line}"),
        None => path,
    })
}

/// Compact JSON fallback: the raw input object serialized with no spaces.
fn json_peek(raw: Option<&serde_json::Value>) -> Option<String> {
    let raw = raw?;
    serde_json::to_string(raw).ok()
}

/// Shorten one location path for display (render-logging "Peek paths
/// render session-relative"): relative to the session cwd when under it,
/// collapsed to `~` when under the user's home but not the cwd, as
/// received otherwise.
fn shorten_path(path: &Path, cwd: &Path, home: Option<&Path>) -> String {
    if let Ok(rel) = path.strip_prefix(cwd)
        && !rel.as_os_str().is_empty()
    {
        return rel.display().to_string();
    }
    if let Some(home) = home
        && let Ok(rel) = path.strip_prefix(home)
        && !rel.as_os_str().is_empty()
    {
        return format!("~/{}", rel.display());
    }
    path.display().to_string()
}

/// Tool-line policy for one session: which tool updates deserve a line.
/// Kept here (where the update stream arrives) rather than in the
/// renderer, which stays a dumb sink.
pub(crate) struct ToolFold {
    calls: HashMap<String, ToolCallDisplay>,
    /// Session cwd (sent at `session/new`) for peek path shortening.
    cwd: Arc<PathBuf>,
}

impl Default for ToolFold {
    fn default() -> Self {
        Self {
            calls: HashMap::new(),
            cwd: Arc::new(PathBuf::new()),
        }
    }
}

impl ToolFold {
    pub(crate) fn with_cwd(cwd: PathBuf) -> Self {
        Self {
            calls: HashMap::new(),
            cwd: Arc::new(cwd),
        }
    }

    /// Fold a `tool_call` announcement. `pending` seeds the map only; an
    /// announcement already `in_progress` renders the start line; an
    /// announcement already terminal renders the terminal line, duration
    /// measured from first observation.
    pub(crate) fn announce(
        &mut self,
        id: &str,
        title: &str,
        status: &str,
        inputs: &PeekInputs<'_>,
        now: Instant,
    ) -> Option<String> {
        let cwd = Arc::clone(&self.cwd);
        let home = home_dir();
        match self.calls.get_mut(id) {
            Some(entry) => {
                entry.title = Some(title.to_string());
                entry.learn_peek(inputs, &cwd, home.as_deref());
            }
            None => {
                let mut entry = ToolCallDisplay {
                    title: Some(title.to_string()),
                    first_activity: Some(now),
                    ..ToolCallDisplay::default()
                };
                entry.learn_peek(inputs, &cwd, home.as_deref());
                self.calls.insert(id.to_string(), entry);
            }
        }
        self.transition(id, status, now)
    }

    /// Fold a `tool_call_update`: peek-relevant fields land in the call's
    /// state (seeding a titleless entry when the id was never announced —
    /// the raw-id fallback); a status, when present, drives the render
    /// policy.
    pub(crate) fn update(
        &mut self,
        id: &str,
        fields: &ToolCallUpdateFields,
        now: Instant,
    ) -> Option<String> {
        let cwd = Arc::clone(&self.cwd);
        let home = home_dir();
        let inputs = PeekInputs {
            kind: fields.kind.as_ref(),
            locations: fields.locations.as_deref(),
            raw_input: fields.raw_input.as_ref(),
        };
        let entry = self
            .calls
            .entry(id.to_string())
            .or_insert_with(|| ToolCallDisplay {
                first_activity: Some(now),
                ..ToolCallDisplay::default()
            });
        entry.learn_peek(&inputs, &cwd, home.as_deref());
        fields
            .status
            .as_ref()
            .and_then(|status| self.transition(id, &status_string(status), now))
    }

    /// Apply the render policy to one observed status and return the
    /// fully formatted line body when a line should render.
    ///
    /// - `pending` (and unknown statuses) never render;
    /// - `in_progress` renders the title (+peek) start line once — repeats
    ///   are silent (the flood guard);
    /// - terminal statuses render title + peek + status + duration, once.
    ///
    /// The peek appends only when the title does not already contain it
    /// (pi-acp-style bash titles are the command itself).
    fn transition(&mut self, id: &str, status: &str, now: Instant) -> Option<String> {
        let entry = self.calls.get_mut(id).expect("entry just seeded");
        // Title via the id→title map; the raw call id is the fallback for
        // updates that preceded their announcement.
        let title = entry.title.clone().unwrap_or_else(|| id.to_string());
        let peek = entry.peek.as_deref().filter(|peek| !title.contains(peek));
        let head = match peek {
            Some(peek) => format!("tool: {title} {peek}"),
            None => format!("tool: {title}"),
        };
        match status {
            "in_progress" => {
                if entry.last_rendered.as_deref() == Some(status) {
                    return None;
                }
                entry.last_rendered = Some(status.to_string());
                // The start line is the duration anchor once it exists.
                entry.first_activity = Some(now);
                Some(head)
            }
            "completed" | "failed" => {
                if entry.last_rendered.as_deref() == Some(status) {
                    return None;
                }
                entry.last_rendered = Some(status.to_string());
                let anchor = entry.first_activity.unwrap_or(now);
                Some(format!(
                    "{head} ({status}, {})",
                    format_duration(now - anchor)
                ))
            }
            _ => None,
        }
    }
}

/// The user's home directory, for `~`-collapsing peek paths.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// `X.Ys` under a minute, `Mm SS.Ss` above. Tenths are rounded up-front so
/// the seconds part can never display `60.0`.
fn format_duration(d: Duration) -> String {
    let tenths = (d.as_millis() + 50) / 100;
    if tenths < 600 {
        format!("{}.{}s", tenths / 10, tenths % 10)
    } else {
        format!(
            "{}m {:02}.{}s",
            tenths / 600,
            (tenths % 600) / 10,
            tenths % 10
        )
    }
}

/// Protocol `ToolCallStatus` as its fold policy name.
pub(crate) fn status_string(status: &agent_client_protocol::schema::v1::ToolCallStatus) -> String {
    use agent_client_protocol::schema::v1::ToolCallStatus::*;
    match status {
        Pending => "pending".into(),
        InProgress => "in_progress".into(),
        Completed => "completed".into(),
        Failed => "failed".into(),
        _ => "unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{ToolCallLocation, ToolKind, ToolCallUpdateFields};

    #[test]
    fn turn_fold_slot_lifecycle() {
        let mut fold = TurnFold::default();
        // Before any turn: submissions are late.
        assert!(!submission_sink(Arc::new(Mutex::new(TurnFold::default())))(
            serde_json::json!({"n": 1})
        ));

        // In flight: accepted, last-wins.
        fold.begin_turn();
        assert!(fold.in_flight && fold.result.is_none());
        fold.result = Some(serde_json::json!({"n": 1}));
        fold.result = Some(serde_json::json!({"n": 2}));
        assert_eq!(
            fold.settle_turn(false),
            (String::new(), Some(serde_json::json!({"n": 2})))
        );
        assert!(!fold.in_flight && fold.result.is_none());

        // Fresh slot per turn: a second turn without submissions yields
        // None even though the first turn had one.
        fold.begin_turn();
        assert_eq!(fold.settle_turn(false), (String::new(), None));

        // Discard on cancelled/failed turns.
        fold.begin_turn();
        fold.result = Some(serde_json::json!({"n": 3}));
        assert_eq!(fold.settle_turn(true), (String::new(), None));
        assert!(!fold.in_flight && fold.result.is_none());
    }

    // -----------------------------------------------------------------------
    // Turn-text fold: last message wins, fallback, discard, no leak
    // -----------------------------------------------------------------------

    #[test]
    fn turn_fold_last_message_run_wins() {
        // chunks → tool → chunks settles to the last run only.
        let mut fold = TurnFold::default();
        fold.begin_turn();
        fold.text.push_str("Lead preamble. ");
        fold.break_message();
        fold.text.push_str("Final");
        fold.text.push_str(" answer.");
        let (text, _) = fold.settle_turn(false);
        assert_eq!(text, "Final answer.");
    }

    #[test]
    fn turn_falls_back_to_previous_run_when_final_is_empty() {
        // chunks → tool with no trailing message: the earlier run is the
        // turn's last agent message.
        let mut fold = TurnFold::default();
        fold.begin_turn();
        fold.text.push_str("The bug is on line 3");
        fold.break_message();
        // Turn ends on tool activity (no chunks after it). An empty
        // current run never clobbers the completed one.
        fold.break_message();
        let (text, _) = fold.settle_turn(false);
        assert_eq!(text, "The bug is on line 3");
    }

    #[test]
    fn turn_without_any_message_settles_empty() {
        let mut fold = TurnFold::default();
        fold.begin_turn();
        fold.break_message();
        let (text, _) = fold.settle_turn(false);
        assert_eq!(text, "");
    }

    #[test]
    fn settle_discard_empties_text() {
        // A cancelled turn's partial text is discarded with its submission.
        let mut fold = TurnFold::default();
        fold.begin_turn();
        fold.text.push_str("partial");
        fold.result = Some(serde_json::json!(1));
        assert_eq!(fold.settle_turn(true), (String::new(), None));
        assert!(fold.text.is_empty() && fold.prev_text.is_empty());

        // Even the fallback run is drained by a discarding settle.
        fold.begin_turn();
        fold.text.push_str("seen");
        fold.break_message();
        assert_eq!(fold.settle_turn(true).0, "");
    }

    #[test]
    fn settled_fold_never_leaks_into_next_turn() {
        let mut fold = TurnFold::default();
        fold.begin_turn();
        fold.text.push_str("first turn");
        fold.break_message();
        fold.text.push_str("tail");
        assert_eq!(fold.settle_turn(false).0, "tail");

        // A second turn on the same fold starts clean — even with no
        // messages of its own, and even if it, too, ends on tool
        // activity (the fallback path).
        fold.begin_turn();
        fold.break_message();
        assert_eq!(fold.settle_turn(false).0, "");
    }

    // -----------------------------------------------------------------------
    // Tool-line fold policy
    // -----------------------------------------------------------------------

    #[test]
    fn tool_fold_pending_seeds_only() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        assert_eq!(
            tools.announce(
                "c1",
                "Search files \"foo\"",
                "pending",
                &PeekInputs::default(),
                t0
            ),
            None
        );
        let entry = tools.calls.get("c1").expect("pending seeds the map");
        assert_eq!(entry.title.as_deref(), Some("Search files \"foo\""));
        assert!(
            entry.first_activity.is_some(),
            "first observation anchors duration"
        );
        assert!(entry.last_rendered.is_none(), "nothing rendered yet");
    }

    #[test]
    fn tool_fold_start_then_terminal_with_duration() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        assert_eq!(
            tools.announce("c1", "T", "pending", &PeekInputs::default(), t0),
            None
        );
        // Start line at the in_progress transition: bare title, no status.
        assert_eq!(
            tools.update(
                "c1",
                &fields_status("in_progress"),
                t0 + Duration::from_millis(100)
            ),
            Some("tool: T".to_string())
        );
        // Terminal line: status + duration measured from the start line.
        assert_eq!(
            tools.update(
                "c1",
                &fields_status("completed"),
                t0 + Duration::from_millis(3300)
            ),
            Some("tool: T (completed, 3.2s)".to_string())
        );
    }

    #[test]
    fn tool_fold_repeated_statuses_are_suppressed() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        tools.announce("c1", "T", "in_progress", &PeekInputs::default(), t0);
        // Repeated in_progress (resent by flood-prone agents).
        assert_eq!(
            tools.update(
                "c1",
                &fields_status("in_progress"),
                t0 + Duration::from_millis(50)
            ),
            None
        );
        // Repeated terminal status.
        tools.update(
            "c1",
            &fields_status("completed"),
            t0 + Duration::from_millis(100),
        );
        assert_eq!(
            tools.update(
                "c1",
                &fields_status("completed"),
                t0 + Duration::from_millis(150)
            ),
            None
        );
    }

    #[test]
    fn tool_fold_announcement_already_in_progress_or_terminal() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        assert_eq!(
            tools.announce("c1", "T", "in_progress", &PeekInputs::default(), t0),
            Some("tool: T".to_string())
        );
        assert_eq!(
            tools.announce(
                "c2",
                "U",
                "completed",
                &PeekInputs::default(),
                t0 + Duration::from_millis(250)
            ),
            Some("tool: U (completed, 0.0s)".to_string())
        );
    }

    #[test]
    fn tool_fold_unannounced_update_falls_back_to_raw_id() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        assert_eq!(
            tools.update("call_0bb9", &fields_status("in_progress"), t0),
            Some("tool: call_0bb9".to_string())
        );
        assert_eq!(
            tools.update(
                "call_0bb9",
                &fields_status("failed"),
                t0 + Duration::from_millis(200)
            ),
            Some("tool: call_0bb9 (failed, 0.2s)".to_string())
        );
        // A late announcement still teaches the map the real title.
        assert_eq!(
            tools.announce(
                "call_0bb9",
                "Real title",
                "completed",
                &PeekInputs::default(),
                t0 + Duration::from_millis(400)
            ),
            Some("tool: Real title (completed, 0.4s)".to_string())
        );
    }

    #[test]
    fn tool_fold_direct_completion_measures_from_first_observation() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        tools.announce("c1", "T", "pending", &PeekInputs::default(), t0);
        assert_eq!(
            tools.update(
                "c1",
                &fields_status("completed"),
                t0 + Duration::from_millis(1200)
            ),
            Some("tool: T (completed, 1.2s)".to_string())
        );
    }

    #[test]
    fn duration_format_shapes() {
        assert_eq!(format_duration(Duration::from_millis(0)), "0.0s");
        assert_eq!(format_duration(Duration::from_millis(49)), "0.0s");
        assert_eq!(format_duration(Duration::from_millis(50)), "0.1s");
        assert_eq!(format_duration(Duration::from_millis(3149)), "3.1s");
        assert_eq!(format_duration(Duration::from_millis(59_949)), "59.9s");
        // The minute boundary and rounding across it.
        assert_eq!(format_duration(Duration::from_millis(59_950)), "1m 00.0s");
        assert_eq!(format_duration(Duration::from_millis(125_040)), "2m 05.0s");
    }

    // -----------------------------------------------------------------------
    // Peeks: kind-aware selection, stickiness, containment, path shortening
    // -----------------------------------------------------------------------

    /// `ToolCallUpdateFields` carrying only a status.
    fn fields_status(status: &str) -> ToolCallUpdateFields {
        ToolCallUpdateFields::new().status(tool_status(status))
    }

    /// Protocol `ToolCallStatus` from its string name.
    fn tool_status(status: &str) -> agent_client_protocol::schema::v1::ToolCallStatus {
        use agent_client_protocol::schema::v1::ToolCallStatus::*;
        match status {
            "pending" => Pending,
            "in_progress" => InProgress,
            "completed" => Completed,
            "failed" => Failed,
            _ => Pending,
        }
    }

    fn loc(path: &str, line: Option<u32>) -> ToolCallLocation {
        let mut l = ToolCallLocation::new(path);
        l.line = line;
        l
    }

    #[test]
    fn peek_execute_kind_shows_the_command() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        let inputs = PeekInputs {
            kind: Some(&ToolKind::Execute),
            locations: Some(&[]),
            raw_input: Some(&serde_json::json!({"command": "git status"})),
        };
        assert_eq!(
            tools.announce("c1", "bash", "in_progress", &inputs, t0),
            Some("tool: bash git status".to_string())
        );
        // The same peek rides on the terminal line.
        assert_eq!(
            tools.update(
                "c1",
                &fields_status("completed"),
                t0 + Duration::from_millis(500)
            ),
            Some("tool: bash git status (completed, 0.5s)".to_string())
        );
    }

    #[test]
    fn peek_execute_prefers_cmd_when_command_is_absent() {
        let mut tools = ToolFold::default();
        let inputs = PeekInputs {
            kind: Some(&ToolKind::Execute),
            locations: Some(&[]),
            raw_input: Some(&serde_json::json!({"cmd": "ls -la"})),
        };
        assert_eq!(
            tools.announce("c1", "bash", "in_progress", &inputs, Instant::now()),
            Some("tool: bash ls -la".to_string())
        );
    }

    #[test]
    fn peek_read_kind_shows_shortened_location() {
        let mut tools = ToolFold::with_cwd(Path::new("/home/u/repo").to_path_buf());
        let inputs = PeekInputs {
            kind: Some(&ToolKind::Read),
            locations: Some(&[loc("/home/u/repo/src/a.rs", Some(12))]),
            raw_input: None,
        };
        assert_eq!(
            tools.announce("c1", "read", "in_progress", &inputs, Instant::now()),
            Some("tool: read src/a.rs:12".to_string())
        );
    }

    #[test]
    fn peek_other_kind_falls_back_to_compact_json() {
        let mut tools = ToolFold::default();
        let inputs = PeekInputs {
            kind: Some(&ToolKind::Other),
            locations: Some(&[]),
            raw_input: Some(&serde_json::json!({"pattern": "foo"})),
        };
        assert_eq!(
            tools.announce("c1", "grep", "in_progress", &inputs, Instant::now()),
            Some("tool: grep {\"pattern\":\"foo\"}".to_string())
        );
    }

    #[test]
    fn peek_no_data_renders_title_alone() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        // "Search files \"foo\"" with no raw input and no locations: the
        // spec's no-derivable-peek case.
        assert_eq!(
            tools.announce(
                "c1",
                "Search files \"foo\"",
                "in_progress",
                &PeekInputs::default(),
                t0
            ),
            Some("tool: Search files \"foo\"".to_string())
        );
        assert!(tools.calls["c1"].peek.is_none());
    }

    #[test]
    fn peek_title_containment_suppresses_duplication() {
        // pi-acp style: the bash title is already the command, so the
        // peek must not append a duplicate.
        let mut tools = ToolFold::default();
        let inputs = PeekInputs {
            kind: Some(&ToolKind::Execute),
            locations: Some(&[]),
            raw_input: Some(&serde_json::json!({"command": "git status"})),
        };
        assert_eq!(
            tools.announce("c1", "git status", "in_progress", &inputs, Instant::now()),
            Some("tool: git status".to_string())
        );
        // A later line for the same call stays deduplicated (the check
        // runs at render time, against the current title).
        assert!(
            tools
                .update("c1", &fields_status("completed"), Instant::now())
                .unwrap()
                .starts_with("tool: git status (")
        );
    }

    #[test]
    fn peek_raw_input_arriving_only_on_an_update_mid_flow() {
        // Announcement carries only the kind; the raw input lands on an
        // update mid-flow (before any line rendered) and the start line
        // gains the peek; the peek sticks afterwards.
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        let announced = PeekInputs {
            kind: Some(&ToolKind::Execute),
            locations: Some(&[]),
            raw_input: None,
        };
        assert_eq!(
            tools.announce("c1", "bash", "pending", &announced, t0),
            None
        );
        // Statusless update carrying the input: folds the peek, renders
        // nothing.
        let raw = serde_json::json!({"command": "cargo test"});
        let fields = ToolCallUpdateFields::new().raw_input(raw);
        assert_eq!(
            tools.update("c1", &fields, t0 + Duration::from_millis(50)),
            None
        );
        assert_eq!(tools.calls["c1"].peek.as_deref(), Some("cargo test"));
        assert_eq!(
            tools.update(
                "c1",
                &fields_status("in_progress"),
                t0 + Duration::from_millis(100)
            ),
            Some("tool: bash cargo test".to_string())
        );
        // Stickiness: a later candidate never overwrites the set peek.
        let later = serde_json::json!({"command": "make check"});
        let fields = ToolCallUpdateFields::new()
            .raw_input(later)
            .status(tool_status("completed"));
        assert_eq!(
            tools.update("c1", &fields, t0 + Duration::from_millis(400)),
            Some("tool: bash cargo test (completed, 0.3s)".to_string())
        );
    }

    #[test]
    fn peek_is_truncated_to_the_shared_budget() {
        let long = "x".repeat(LINE_BUDGET + 50);
        let mut tools = ToolFold::default();
        let inputs = PeekInputs {
            kind: Some(&ToolKind::Execute),
            locations: Some(&[]),
            raw_input: Some(&serde_json::json!({"command": long})),
        };
        let line = tools
            .announce("c1", "bash", "in_progress", &inputs, Instant::now())
            .unwrap();
        assert_eq!(line, format!("tool: bash {}…", "x".repeat(LINE_BUDGET)));
    }

    #[test]
    fn shorten_path_covers_the_three_cases() {
        let cwd = Path::new("/home/u/repo");
        let home = Some(Path::new("/home/u"));
        // Under the session cwd.
        assert_eq!(
            shorten_path(Path::new("/home/u/repo/src/a.rs"), cwd, home),
            "src/a.rs"
        );
        // Outside the cwd but under home: ~-collapsed.
        assert_eq!(
            shorten_path(Path::new("/home/u/notes/todo.md"), cwd, home),
            "~/notes/todo.md"
        );
        // Outside home entirely.
        assert_eq!(
            shorten_path(Path::new("/tmp/build.log"), cwd, home),
            "/tmp/build.log"
        );
        // No home known: still cwd-relative, else as-is.
        assert_eq!(
            shorten_path(Path::new("/home/u/notes/todo.md"), cwd, None),
            "/home/u/notes/todo.md"
        );
    }
}
