## MODIFIED Requirements

### Requirement: Prompt turns render a prompt line
ptah SHALL render exactly one prompt line per prompt turn, at the moment the
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
- **WHEN** ptah runs with `--quiet` and a session is prompted
- **THEN** no prompt line is rendered

### Requirement: Rendered lines carry a full date timestamp
Every rendered line (session-attributed and `ptah` lines alike) SHALL be
prefixed with a local timestamp shaped `yyyy-mm-dd HH:MM:SS` (space-separated).
The date SHALL appear on every line, not as a session banner.

#### Scenario: Timestamp shape
- **WHEN** any render output line is emitted
- **THEN** the line begins with a `yyyy-mm-dd HH:MM:SS` local timestamp before the `[label]` prefix

### Requirement: Exec lines render command and outcome
The renderer SHALL render one line when an exec starts (carrying the command string) and one line when it ends (carrying the exit code and duration, or the timeout/spawn-failure marker), using the same timestamped line format as session lines but attributed so they read as script activity rather than a named session. Captured child stdout/stderr SHALL NOT be rendered. `--quiet` SHALL suppress exec lines entirely (they are session-event-like, not `ptah.log` script logs).

#### Scenario: Color mode shows both lines
- **WHEN** a script calls `ptah.exec("printf hi")` in default (color) output mode
- **THEN** the terminal shows a start line containing the command `printf hi` and an end line containing exit code 0 and a duration, interleaved at the moment each fires

#### Scenario: Quiet suppresses exec lines
- **WHEN** the same script runs with `--quiet`
- **THEN** no exec lines are printed; a `ptah.log` call from the script still prints

#### Scenario: Failed exec end line carries the code
- **WHEN** a script calls `ptah.exec("sh -c 'exit 4'")` in color mode
- **THEN** the end line shows exit code 4 (and the run continues; the failure is not a render error)
