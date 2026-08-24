## MODIFIED Requirements

### Requirement: Uncaught script error fails the run
WHEN a script error escapes the main chunk uncaught, ponos SHALL cancel all in-flight prompt turns, terminate all agent subprocesses, print the error to stderr, and exit with a non-zero code. A task error that is never delivered to the script is treated the same way at script end: after all outstanding tasks complete, any task whose error was never observed (via `:await()`, `join`, or as a value in `ponos.parallel` results) SHALL fail the run with that error printed to stderr and a non-zero exit code — unless the script already terminated via `ponos.exit`, whose code wins.

#### Scenario: Error propagation
- **WHEN** a spawned task raises and the script awaits it without catching
- **THEN** the error is re-raised at the await site; if it escapes the main chunk uncaught, the run terminates non-zero

#### Scenario: Never-retrieved task error at script end
- **WHEN** a spawned task raises an error the script never observes via `await`/`join`, and the main chunk finishes normally
- **THEN** ponos waits for outstanding tasks, prints the task's error to stderr, and exits non-zero
