## MODIFIED Requirements

### Requirement: Prompt turns drive the full update stream
During a prompt turn ponos SHALL render exactly one prompt line when the
prompt is sent (per the `render-logging` capability's prompt-line
contract), receive `session/update` notifications and: accumulate
`agent_message_chunk` into the final message text, render tool call and plan
updates for display per the tool-line contract below, and render
`usage_update` context-window information for display. Token counts SHALL be
taken from the `session/prompt` response and returned as the result's
`usage` (zero when the response reports none). The turn SHALL complete when
the `session/prompt` response arrives, returning its `stopReason`.

Tool-line contract: ponos SHALL maintain, per session, a map of tool call id
to title learned from `tool_call` notifications, and SHALL resolve titles
for lines derived from `tool_call_update` notifications through that map
(falling back to the raw call id only when an update arrives for a call that
was never announced). A tool call SHALL render at most one line when it
enters `in_progress` (the start line: the title with the input peek appended
per the `render-logging` capability, and no status suffix) and at most one
line when it reaches a terminal status (`completed` or `failed`; the same
peek appended). `pending` status SHALL NOT render, and a status transition
that repeats the last-rendered status for the same call SHALL NOT render.
Terminal lines SHALL include the call's elapsed wall-clock duration,
measured from the call's first rendered activity (`in_progress` transition,
or first observation when no `in_progress` ever arrives).

#### Scenario: Chunks assemble final text
- **WHEN** an agent streams two `agent_message_chunk` updates ("Hel", "lo") and ends its turn
- **THEN** the prompt result's `text` is `"Hello"`

#### Scenario: Usage accounting
- **WHEN** an agent reports token usage on its `session/prompt` response
- **THEN** the result's `usage` reflects the reported token counts

#### Scenario: Context-window usage rendered
- **WHEN** an agent emits a `usage_update` during a turn
- **THEN** the reported context-window usage is rendered with session attribution

#### Scenario: Tool call renders start and terminal lines
- **WHEN** an agent announces a tool call (`tool_call`, status `pending`), updates it to `in_progress`, then to `completed`
- **THEN** exactly two lines are rendered for that call: the title (with peek, when one is derivable) alone at start, and the title with peek, terminal status and elapsed duration at completion

#### Scenario: Update lines use titles, not raw call ids
- **WHEN** an agent announces a tool call titled `Search files "foo"` and then sends a `tool_call_update` changing its status
- **THEN** the rendered line names `Search files "foo"` and never the raw call id

#### Scenario: Repeated statuses do not flood the log
- **WHEN** an agent sends the same status for the same tool call more than once (e.g. two consecutive `in_progress` updates, or a re-sent terminal status)
- **THEN** each repeated status renders nothing beyond the line its first occurrence already rendered

#### Scenario: Pending is silent
- **WHEN** an agent announces a tool call with `pending` status
- **THEN** no line is rendered for the `pending` announcement

#### Scenario: Unannounced update falls back to the call id
- **WHEN** an agent sends a `tool_call_update` for a call id that was never announced by a `tool_call`
- **THEN** the rendered line falls back to the raw call id

#### Scenario: Direct completion without progress
- **WHEN** an agent announces a tool call (`pending`) and updates it straight to `completed` without any `in_progress` update
- **THEN** only the terminal line is rendered, with duration measured from first observation
