# Render Logging Specification

## Purpose

Defines the observability contract of ponos's streaming stdout log: what is
rendered for each prompt turn and tool call, how long or short those lines
are, and which flags gate them. The render output is ponos's only log; this
capability is what makes a multi-agent fan-out followable.

## Requirements

### Requirement: Prompt turns render a prompt line
ponos SHALL render exactly one prompt line per prompt turn, at the moment the
prompt is sent to the agent, attributed to the sending session's label. The
line SHALL consist of the prefix `prompt: ` followed by the prompt text with
runs of whitespace collapsed to single spaces, truncated to a visible-char
budget (approximately 120 characters, shared with the tool peek budget) with
a trailing `…` marker when truncation occurs. The prompt line SHALL render
under default flags on every session and SHALL be suppressed by `--quiet`.

#### Scenario: Prompt line rendered
- **WHEN** a session is prompted with a multi-line prompt beginning `review the auth module`
- **THEN** one line like `prompt: review the auth module …` is rendered with that session's label before the turn's other output

#### Scenario: Long prompt truncated
- **WHEN** a session is prompted with prompt text longer than the visible-char budget
- **THEN** the prompt line shows the first budget's worth of collapsed text followed by `…` and no more

#### Scenario: Quiet suppresses the prompt line
- **WHEN** ponos runs with `--quiet` and a session is prompted
- **THEN** no prompt line is rendered

### Requirement: Tool lines carry an input peek
Tool call start and terminal lines SHALL append an input peek after the
title, chosen kind-aware from the tool call data, when the title does not
already contain the peek text (substring match, case-sensitive). The peek
SHALL be selected in priority order:

1. `execute` kind: the `command` or `cmd` string from the tool call's raw
   input object, when a non-empty string is present;
2. `read`, `edit`, `move`, or `search` kind (or `fetch`, `delete`): the first
   location's path, with `:line` appended when a line number is present;
3. otherwise: the raw input object serialized as compact JSON.

When no candidate is derivable, the line renders the title alone, exactly as
before. Peeks apply the same visible-char budget and `…` truncation as the
prompt line. The peek SHALL render on both the start line and the terminal
line of a tool call.

#### Scenario: Execute kind shows the command
- **WHEN** a tool call with kind `execute` and raw input `{"command": "git status"}` is announced (title `bash`)
- **THEN** the start line renders as `tool: bash git status` and the terminal line as `tool: bash git status (completed, …)`

#### Scenario: Read kind shows the location path
- **WHEN** a tool call with kind `read`, location `/home/u/repo/src/a.rs` line 12, and title `read` is announced with the session's cwd `/home/u/repo`
- **THEN** the rendered line names `tool: read src/a.rs:12`

#### Scenario: Title already contains the peek
- **WHEN** a tool call has title `git status` and an `execute` peek candidate `git status`
- **THEN** the peek is not appended; the line renders `tool: git status` with no duplication

#### Scenario: Unknown tool falls back to compact raw input
- **WHEN** a tool call has kind `other`, title `grep`, and raw input `{"pattern": "foo"}`
- **THEN** the rendered line names `tool: grep {"pattern":"foo"}`

#### Scenario: No derivable peek
- **WHEN** a tool call has title `Search files "foo"` with no raw input and no locations
- **THEN** the line renders the title alone, as before this change

### Requirement: Peek paths render session-relative
Location paths in peeks SHALL render relative to the session's cwd when the
path is under it, collapsed to `~` when under the user's home directory but
not under the session cwd, and otherwise as received.

#### Scenario: Path under session cwd
- **WHEN** a peek location is `/home/u/repo/src/a.rs` and the session's cwd is `/home/u/repo`
- **THEN** the path renders as `src/a.rs`

#### Scenario: Path outside session cwd but under home
- **WHEN** a peek location is `/home/u/notes/todo.md` and the session's cwd is `/home/u/repo`
- **THEN** the path renders as `~/notes/todo.md`

#### Scenario: Path outside home
- **WHEN** a peek location is `/tmp/build.log`
- **THEN** the path renders as `/tmp/build.log`

### Requirement: Rendered lines carry a full date timestamp
Every rendered line (session-attributed and `ponos` lines alike) SHALL be
prefixed with a local timestamp shaped `yyyy-mm-dd HH:MM:SS` (space-separated).
The date SHALL appear on every line, not as a session banner.

#### Scenario: Timestamp shape
- **WHEN** any render output line is emitted
- **THEN** the line begins with a `yyyy-mm-dd HH:MM:SS` local timestamp before the `[label]` prefix
