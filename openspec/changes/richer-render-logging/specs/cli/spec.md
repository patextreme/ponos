## MODIFIED Requirements

### Requirement: Rendered lines are timestamped
Every rendered output line — agent message chunks, tool lines, plan summaries, context-usage lines, lifecycle diagnostics, `ponos.log` lines, and `-vv` agent stderr passthrough — SHALL be prefixed with a local-time timestamp shaped `yyyy-mm-dd HH:MM:SS` (space-separated), per the `render-logging` capability's timestamp contract, ahead of the session attribution prefix. Timestamps SHALL be always on: no flag controls them. `--no-color` SHALL keep the timestamp as plain text, and `--quiet` SHALL continue to suppress all rendered output. Script `print` output does not pass through the renderer and SHALL NOT be timestamped or otherwise modified.

#### Scenario: Timestamp on rendered lines
- **WHEN** any rendered line is emitted (message chunk, tool line, plan, usage, lifecycle diagnostic, `ponos.log`, or agent stderr passthrough)
- **THEN** the line begins with a `yyyy-mm-dd HH:MM:SS` local-time timestamp

#### Scenario: No-color keeps plain timestamps
- **WHEN** a script runs with `--no-color`
- **THEN** rendered lines still carry the timestamp as plain text, without ANSI sequences

#### Scenario: Script print output is untouched
- **WHEN** a script calls `print("hello")`
- **THEN** the output line is exactly the script's text with no timestamp or prefix added
