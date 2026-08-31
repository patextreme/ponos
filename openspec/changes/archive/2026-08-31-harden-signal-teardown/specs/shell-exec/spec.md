## MODIFIED Requirements

### Requirement: In-flight execs are killed at teardown
When the script errors, calls `ptah.exit`, or the run is cancelled (e.g. Ctrl-C), any still-running `ptah.exec` child SHALL have its process group killed — no orphaned processes outlive the run. When the cancellation is an outer signal (SIGINT/SIGTERM forwarded by the composition root), the run SHALL exit with the shell-conventional 128+signal code (130/143) after teardown, rather than dying on the signal with teardown skipped. A second outer signal arriving while teardown is still in progress SHALL kill every still-running exec child's process group before the immediate exit, and that exit SHALL use the code matching the second signal (130 for SIGINT, 143 for SIGTERM) — the force escape never orphans a child.

#### Scenario: Script error kills running child
- **WHEN** a spawned script task has `ptah.exec("sleep 30")` in flight and the script's main body raises an error that ends the run
- **THEN** the in-flight exec's process group is killed during teardown

#### Scenario: ptah.exit kills running child
- **WHEN** a script calls `ptah.exit(0)` while an exec is in flight
- **THEN** teardown kills the exec's process group before the process exits

#### Scenario: Ctrl-C kills the running child and exits 130
- **WHEN** SIGINT interrupts a run while an exec is in flight (the exec child sits in its own process group, out of the terminal signal's reach)
- **THEN** teardown kills the exec's process group before the process exits, and the run exits 130 rather than orphaning the child (SIGTERM likewise runs teardown and exits 143)

#### Scenario: Second signal kills in-flight exec during teardown
- **WHEN** a first signal starts teardown while an exec is in flight and a second signal arrives before teardown has killed the exec's process group
- **THEN** the exec's process group is killed before the immediate exit, and the run exits with the code matching the second signal (130 for SIGINT, 143 for SIGTERM)

#### Scenario: Teardown cancellation does not change the run's outcome
- **WHEN** a script calls `ptah.exit(0)` while a spawned task is parked in `ptah.exec`
- **THEN** the run still exits 0 and nothing about the cancelled exec surfaces in the run's result — the cancellation is shutdown, not a script failure (a script-ending error likewise still reports as itself, not as a cancelled-exec error)
