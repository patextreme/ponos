## MODIFIED Requirements

### Requirement: Processes are torn down at run end
At script end (normal, error, or explicit `ptah.exit`), ptah SHALL terminate and reap every still-running agent subprocess. When the run is cancelled by an outer signal (SIGINT/SIGTERM forwarded by the composition root), teardown SHALL likewise terminate and reap every agent subprocess before the run exits with the shell-conventional 128+signal code (130/143). A second outer signal arriving while teardown is still in progress SHALL kill every not-yet-reaped agent's whole process group before the immediate exit — no agent subprocess, nor any process in its group, outlives the ptah process on any end-of-run path, including the force escape.

#### Scenario: Normal exit cleanup
- **WHEN** the script finishes with sessions left open
- **THEN** all agent subprocesses are terminated and reaped before the ptah process exits

#### Scenario: Error exit cleanup
- **WHEN** an uncaught script error aborts the run
- **THEN** in-flight turns are cancelled and all agent subprocesses are terminated and reaped

#### Scenario: Ctrl-C kills the agent and exits 130
- **WHEN** SIGINT interrupts a run while an agent session is in flight (the agent sits in its own process group, out of the terminal signal's reach)
- **THEN** teardown kills the agent's process group before the process exits, and the run exits 130 rather than orphaning the agent (SIGTERM likewise runs teardown and exits 143)

#### Scenario: Second signal kills not-yet-reaped agents
- **WHEN** a first signal starts teardown with a live agent and a second signal arrives before teardown has killed it
- **THEN** the agent's process group is killed before the immediate exit, and the exit code matches the second signal (130 for SIGINT, 143 for SIGTERM)
