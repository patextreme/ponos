## Why

A second SIGINT/SIGTERM — the "press again to force" escape while teardown is
still draining — calls `std::process::exit(130)` immediately. Drop guards never
run on that path, so any agent subprocess teardown has not reached yet, and any
in-flight `ptah.exec` child (teardown's exec drain alone can hold the window
open for up to ~2s), is orphaned in its own process group with no killer. That
is the exact bug signal handling was built to fix, surviving one signal later.
Two further gaps ride along: the agent-kill-on-signal leg has no test (the
existing signal tests exercise only exec children), and the agent-side teardown
contract — agents never outlive the run — is not specified anywhere.

## What Changes

- A process-group **kill registry** (pure pid bookkeeping: register at spawn,
  deregister at natural death) shared by both child-spawning sites: agent
  sessions (`ptah-acp`) and `ptah.exec` (`ptah-cli` process runner).
- The signal monitor in the composition root, on the **second** signal,
  synchronously SIGKILLs every registered process group (bounded, idempotent
  raw-`kill` sweep; no async) before exiting.
- The hard-exit code now matches the **second** signal (130 for SIGINT, 143
  for SIGTERM) instead of the hardcoded `130`.
- The first-signal teardown path is deliberately unchanged: the registry is
  the escape hatch, not the mechanism.
- New e2e tests: SIGINT during a hung agent turn kills the agent and exits
  130 (SIGTERM/143 likewise, as today for execs); first + second signal in
  quick succession kills agents and execs despite teardown still draining.
- README's cancellation paragraph amended (second signal now kills children,
  not just exits) plus a unix-only note; the stale exploration doc
  `openspec/explorations/signal-handling-and-agent-orphaning.md` is removed —
  its content is superseded by this change and the specs it amends.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `agent-sessions`: gains a teardown requirement — agent subprocesses (their
  whole process groups) are killed and reaped at every run end, outer-signal
  cancellation included; no agent outlives the run; a second signal during
  teardown kills not-yet-reaped agents before the hard exit.
- `shell-exec`: the existing "In-flight execs are killed at teardown"
  requirement gains second-signal scenarios — the hard exit kills in-flight
  exec groups via the same sweep and exits with the code matching the second
  signal.

## Impact

- `ptah-core`: new small registry type (data only — register / deregister /
  snapshot of process-group leader pids; no I/O, no new port; the five-port
  set is untouched).
- `ptah-acp`: `Transport` gains construction-time injection of the registry;
  session spawn/teardown register and deregister the agent's pid.
- `ptah-cli`: `TokioProcessRunner` holds the registry (exec spawn/deregister);
  `install_signal_monitor` sweeps it on the second signal and exits with the
  matching code; composition root wires one registry instance into both.
- `crates/ptah-cli/tests/`: new signal tests against the mock agent
  (`MOCK_HANG`), fully offline.
- Docs: README cancellation paragraph; exploration doc removed.
- Accepted residuals (documented in design.md, not fixed): `kill -9` of ptah
  itself can never clean up (unfixable in-process); teardown stays instant
  SIGKILL with no grace window after `session/cancel` (by design); non-unix
  platforms remain forever uncancellable (unix-only contract).
