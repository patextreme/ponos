## ADDED Requirements

### Requirement: Exec lines render command and outcome
The renderer SHALL render one line when an exec starts (carrying the command string) and one line when it ends (carrying the exit code and duration, or the timeout/spawn-failure marker), using the same timestamped line format as session lines but attributed so they read as script activity rather than a named session. Captured child stdout/stderr SHALL NOT be rendered. `--quiet` SHALL suppress exec lines entirely (they are session-event-like, not `ponos.log` script logs).

#### Scenario: Color mode shows both lines
- **WHEN** a script calls `ponos.exec("printf hi")` in default (color) output mode
- **THEN** the terminal shows a start line containing the command `printf hi` and an end line containing exit code 0 and a duration, interleaved at the moment each fires

#### Scenario: Quiet suppresses exec lines
- **WHEN** the same script runs with `--quiet`
- **THEN** no exec lines are printed; a `ponos.log` call from the script still prints

#### Scenario: Failed exec end line carries the code
- **WHEN** a script calls `ponos.exec("sh -c 'exit 4'")` in color mode
- **THEN** the end line shows exit code 4 (and the run continues; the failure is not a render error)
