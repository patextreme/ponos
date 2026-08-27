## Purpose

Defines how ponos scripts execute deterministic shell work directly: the `ponos.exec` binding (invocation, result contract, timeout, environment), the injected process-execution capability that funds it, and the teardown and observability behavior surrounding a running command.

## ADDED Requirements

### Requirement: Exec runs a shell command and returns a result table
The scripting environment SHALL provide `ponos.exec(cmd: string, opts?: table)` that executes `cmd` via `/bin/sh -c` and returns a table `{ exitCode: number, stdout: string, stderr: string }`. The call blocks the script coroutine that invoked it; other in-flight work (spawned agent tasks, concurrently awaited turns) SHALL continue to progress while an exec runs. `opts` SHALL accept `timeoutMs` (a number; absent or nil means no timeout) and no other option in v1. A nonzero exit code is data, not an error: the result table is returned normally.

#### Scenario: Successful command
- **WHEN** a script calls `ponos.exec("printf hello")`
- **THEN** the call returns `{ exitCode = 0, stdout = "hello", stderr = "" }`

#### Scenario: Pipeline through the shell
- **WHEN** a script calls `ponos.exec("printf 'a\\nb' | wc -l")`
- **THEN** the pipeline runs under `/bin/sh` and the call returns the pipeline's exit code and the final command's stdout

#### Scenario: Nonzero exit is data
- **WHEN** a script calls `ponos.exec("sh -c 'echo boom >&2; exit 3'")`
- **THEN** the call returns `{ exitCode = 3, stdout = "", stderr = "boom\\n" }` without raising

#### Scenario: Spawned agents keep progressing
- **WHEN** a script awaits a `ponos.spawn`ed agent task and, before awaiting it, calls `ponos.exec("sleep 0.2")`
- **THEN** the spawned task continues to progress during the exec and both complete

### Requirement: Exec raises only on could-not-run, timeout, and teardown
`ponos.exec` SHALL raise a Lua error when the command could not be executed at all (e.g. the shell binary cannot be spawned), and when `timeoutMs` elapses. No other condition raises while the run continues. On timeout, the command's process group SHALL be killed before the error is raised. The raised timeout error SHALL identify the command and the elapsed budget, so a script can distinguish it from a turn result. One further raise exists only at teardown: when the run itself is ending (script error, `ponos.exit`, or outer cancellation), an in-flight exec raises a catchable error naming the command and the run's end after its process group is killed — that error is the run's shutdown, not the command's outcome, and it never changes the run's own result (a `ponos.exit(0)` with an exec in flight still exits 0).

#### Scenario: Timeout kills the process group and raises
- **WHEN** a script calls `ponos.exec("sleep 5", { timeoutMs = 100 })`
- **THEN** the sleep and any children it spawned are killed, the call raises an error naming the command and timeout, and the run does not hang

#### Scenario: A pcall can catch a timeout
- **WHEN** a script wraps `ponos.exec(cmd, { timeoutMs = 100 })` in `pcall`
- **THEN** the pcall returns `false` plus the error message, and the script continues

#### Scenario: No timeout waits indefinitely
- **WHEN** a script calls `ponos.exec(cmd)` with no options
- **THEN** the call waits until the command exits on its own (bounded only by outer cancellation)

### Requirement: Exec child environment is inherited and non-interactive
A command run via `ponos.exec` SHALL inherit ponos's environment variables and working directory; v1 provides no `cwd` or `env` override options. The child's stdin SHALL be closed (reads return EOF): exec is non-interactive, and a child that prompts on stdin fails fast instead of hanging or touching the user's terminal.

#### Scenario: Environment inheritance
- **WHEN** ponos is running with `EXAMPLE_TOKEN=x` in its environment and a script calls `ponos.exec("printf $EXAMPLE_TOKEN")`
- **THEN** the result's stdout is `x`

#### Scenario: Working directory inheritance
- **WHEN** ponos is invoked from a directory containing `marker.txt` and a script calls `ponos.exec("cat marker.txt")`
- **THEN** the file is read from the invocation directory's cwd

#### Scenario: Interactive child gets EOF, not the terminal
- **WHEN** a script calls `ponos.exec("cat")` (a child that reads stdin until EOF)
- **THEN** the child reads EOF and exits immediately; no input is consumed from ponos's own stdin

### Requirement: Exec is an injected capability, not a sandbox global
Process execution SHALL reach the scripting environment only through a process-runner capability injected by the composition root; the ambient sandbox globals SHALL remain free of subprocess execution (no `os.execute`, no `io`). `ponos.exec` raises a runtime error when no runner was injected. The capability is always injected by the `ponos` CLI — there is no gating flag or config switch; running a ponos script already implies arbitrary shell through the headless allow-all agent posture.

#### Scenario: Ambient globals stay clean
- **WHEN** a script accesses `os.execute` or `io`
- **THEN** the access resolves to nil (or raises on call) exactly as before this capability existed

#### Scenario: Exec is available in every run
- **WHEN** any script run via the `ponos` CLI calls `ponos.exec`
- **THEN** the command runs; no flag, registry entry, or config opt-in is required

### Requirement: In-flight execs are killed at teardown
When the script errors, calls `ponos.exit`, or the run is cancelled (e.g. Ctrl-C), any still-running `ponos.exec` child SHALL have its process group killed — no orphaned processes outlive the run. When the cancellation is an outer signal (SIGINT/SIGTERM forwarded by the composition root), the run SHALL exit with the shell-conventional 128+signal code (130/143) after teardown, rather than dying on the signal with teardown skipped.

#### Scenario: Script error kills running child
- **WHEN** a spawned script task has `ponos.exec("sleep 30")` in flight and the script's main body raises an error that ends the run
- **THEN** the in-flight exec's process group is killed during teardown

#### Scenario: ponos.exit kills running child
- **WHEN** a script calls `ponos.exit(0)` while an exec is in flight
- **THEN** teardown kills the exec's process group before the process exits

#### Scenario: Ctrl-C kills the running child and exits 130
- **WHEN** SIGINT interrupts a run while an exec is in flight (the exec child sits in its own process group, out of the terminal signal's reach)
- **THEN** teardown kills the exec's process group before the process exits, and the run exits 130 rather than orphaning the child (SIGTERM likewise runs teardown and exits 143)

#### Scenario: Teardown cancellation does not change the run's outcome
- **WHEN** a script calls `ponos.exit(0)` while a spawned task is parked in `ponos.exec`
- **THEN** the run still exits 0 and nothing about the cancelled exec surfaces in the run's result — the cancellation is shutdown, not a script failure (a script-ending error likewise still reports as itself, not as a cancelled-exec error)

### Requirement: Exec lifecycle is observable
Each `ponos.exec` call SHALL emit lifecycle events through the event sink: a start event carrying the command string, and an end event carrying the exit status (or timeout/spawn-failure marker) and duration. A call cancelled by teardown emits no end event — the run is ending and the kill, not the command's outcome, is what ended it. Captured stdout/stderr are not streamed to the terminal; they belong to the script.

#### Scenario: Lifecycle events fire
- **WHEN** a script calls `ponos.exec("printf hi")`
- **THEN** a start event (with the command) and an end event (with exit code 0 and a duration) are emitted to the sink around the call

#### Scenario: Output is not echoed
- **WHEN** a command writes to stdout and stderr and the script does not log them
- **THEN** the terminal shows only lifecycle lines, never the child's captured output
