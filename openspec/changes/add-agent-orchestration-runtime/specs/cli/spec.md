# CLI Spec

## Purpose

Defines the user-facing command surface of the ponos binary: how scripts are invoked, how output is controlled, and how the process exits.

## ADDED Requirements

### Requirement: Run subcommand executes a script
The `ponos` CLI SHALL provide `ponos run <script.luau>` as its only subcommand, where `<script.luau>` is a positional required path to the entry Luau script.

#### Scenario: Successful run
- **WHEN** `ponos run script.luau` is invoked and the script completes without uncaught errors
- **THEN** the process exits with code 0

#### Scenario: Missing script argument
- **WHEN** `ponos run` is invoked without a positional path
- **THEN** the CLI prints a usage error and exits non-zero without executing anything

#### Scenario: Nonexistent script file
- **WHEN** the positional path does not exist on disk
- **THEN** the CLI prints an error naming the path and exits non-zero

### Requirement: Output control flags
The CLI SHALL accept output flags: `--quiet` suppresses all streaming render and diagnostics, `--verbose` shows runtime lifecycle diagnostics, a second verbosity level (`-vv`) additionally passes agent subprocess stderr through, and `--no-color` disables ANSI colors while keeping text prefixes.

#### Scenario: Quiet flag
- **WHEN** a script runs with `--quiet`
- **THEN** no streaming output from agents is printed (output produced by the script itself via `print` still passes through)

#### Scenario: No-color degradation
- **WHEN** a script runs with `--no-color`
- **THEN** session output is still attributed by its text prefix but contains no ANSI escape sequences

### Requirement: Version flag
The CLI SHALL support `--version`, printing the ponos version string and exiting 0.

#### Scenario: Print version
- **WHEN** `ponos --version` is invoked
- **THEN** the version string is printed and the process exits 0

### Requirement: Script end waits for outstanding tasks
WHEN the main script chunk finishes while spawned tasks are still running, the process SHALL wait for all outstanding tasks to complete before exiting, UNLESS the script calls `ponos.exit(code)`.

#### Scenario: Pending spawn at script end
- **WHEN** the main chunk returns while a `ponos.spawn` task is still awaiting an agent prompt
- **THEN** ponos waits for that task to finish before exiting

#### Scenario: Explicit exit
- **WHEN** the script calls `ponos.exit(3)` while tasks are pending
- **THEN** pending tasks and agent processes are torn down and the process exits with code 3

### Requirement: Uncaught script error fails the run
WHEN a script error escapes the main chunk uncaught, ponos SHALL cancel all in-flight prompt turns, terminate all agent subprocesses, print the error to stderr, and exit with a non-zero code. A task error that is never delivered to the script is treated the same way at script end: after all outstanding tasks complete, any task whose error was never observed (via `:await()`, `join`, or as a value in `ponos.map` results) SHALL fail the run with that error printed to stderr and a non-zero exit code — unless the script already terminated via `ponos.exit`, whose code wins.

#### Scenario: Error propagation
- **WHEN** a spawned task raises and the script awaits it without catching
- **THEN** the error is re-raised at the await site; if it escapes the main chunk uncaught, the run terminates non-zero

#### Scenario: Never-retrieved task error at script end
- **WHEN** a spawned task raises an error the script never observes via `await`/`join`, and the main chunk finishes normally
- **THEN** ponos waits for outstanding tasks, prints the task's error to stderr, and exits non-zero

### Requirement: Session cwd defaults to invocation directory
The default working directory for agent sessions SHALL be the directory from which `ponos run` was invoked, unless the script overrides it per session.

#### Scenario: Default cwd
- **WHEN** a session is created without an explicit `cwd`
- **THEN** the session's working directory is ponos's invocation directory
