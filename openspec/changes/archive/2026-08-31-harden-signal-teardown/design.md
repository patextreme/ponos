## Context

Signal handling shipped with `2026-08-26-add-shell-exec`: the composition
root installs a SIGINT/SIGTERM monitor (`crates/ptah-cli/src/cli.rs`,
`install_signal_monitor`) that forwards the first signal's code (130/143) into
a shutdown watch; `crates/ptah-luau/src/run.rs` races the script future against
that watch and, on cancel, runs the same teardown as script-error/exit paths.
Teardown drains in-flight execs (`kill_inflight_execs`: signal killed-flags,
wait for coroutines to drop their kill-on-drop port futures, ~2s deadline),
then closes sessions sequentially (each: `session/cancel` notification, driver
loop break, group SIGKILL + reap via `kill_and_reap`).

The gap is the **second** signal: the monitor hard-exits via
`std::process::exit(130)`, which runs no destructors. `GroupKillGuard` (exec)
and `kill_and_reap` (agents) are drop/await-driven, so every child teardown
has not yet reached is orphaned — in its own process group, with no killer.
The window is real: the exec drain alone can hold it open for up to ~2s.

## Goals / Non-Goals

**Goals:**

- No ptah-spawned child (agent group or exec group) outlives the ptah process
  on *any* end-of-run path, the force escape included.
- The force escape stays synchronous and runtime-independent — it must work
  exactly when the async runtime is too wedged to drain.
- Second-signal exit code matches the signal that fired (SIGTERM no longer
  reports 130).

**Non-Goals:**

- No grace window on teardown: instant group SIGKILL stays; the
  `session/cancel` notification teardown sends before the kill is kept but is
  vestigial by design (nobody reads agent output after teardown begins).
- No Windows path: unix-only contract; `cfg(not(unix))` keeps dropping the
  shutdown sender (never-cancel).
- No `kill -9` mitigation (`PR_SET_PDEATHSIG` would require patching the
  `agent-client-protocol` crate's spawn): accepted residual.
- No rework of the first-signal teardown path or `kill_inflight_execs`.

## Decisions

### D1: One registry for both child kinds; second-signal-only role

A single process-group registry — register the group-leader pid at spawn,
deregister at natural death — shared by the ACP transport (agent spawns) and
the CLI process runner (exec spawns). The monitor sweeps it **only** on the
second signal; teardown is unchanged.

*Alternative considered:* make the registry the universal kill primitive and
rewire `kill_inflight_execs`/teardown through it. Rejected: teardown does more
than kill (coroutine bookkeeping, joins, reaps, stderr-pump completion), works,
and is tested; the registry exists precisely for the path where none of that
machinery can run.

### D2: Registry type lives in `ptah-core`; injection at the composition root

The registry is pure data — `Mutex<Vec<pid>>`-shaped register / deregister /
snapshot, no syscalls inside the type — so it keeps core I/O-free and
`deps_guard` clean. Both adapters (`ptah-acp` transport, `TokioProcessRunner`)
gain a constructor taking the shared handle; `cli.rs` creates one instance,
injects it into both, and hands it to the monitor. The port set is untouched:
this is domain bookkeeping, not a sixth port — `AgentTransport` and
`ProcessRunner` signatures do not change.

*Alternatives considered:* two per-adapter registries the monitor holds both
of (duplicates the type for no benefit); defining it in `ptah-acp` (works —
`ptah-cli` sees everything — but inverts the natural ownership: the type
describes "children of this run", which is domain).

### D3: The sweep is synchronous, raw, and terminal

On the second signal the monitor snapshots the registry and issues
`kill(-pid, SIGKILL)` per entry — idempotent, `ESRCH` for already-dead entries
ignored — then exits with the code of whichever signal fired second (a second
`select!` over both streams). No async work, no reap loop: the escape hatch
must not depend on the runtime it is escaping. Reaping is unnecessary — the
children are re-parented to init, which reaps them; nothing observable leaks.

*Alternative considered:* run teardown-to-completion on the second signal with
a timeout. Rejected: a wedged runtime is the second signal's raison d'être.

### D4: Registration/deregistration points

- **Agents:** the ACP transport registers the spawned child's pid immediately
  after `spawn_process` succeeds; deregistration happens wherever the driver
  task disposes of the child (`kill_and_reap`'s callers cover every driver
  exit — Close, error, connection end).
- **Execs:** `GroupKillGuard` holds the registry handle; `Drop` deregisters
  (the single choke point every exec exit path already funnels through —
  natural completion calls `disarm`, then `Drop` still runs and deregisters).

### D5: Testing — unit-pin the sweep, e2e-pin the invariant

- **Unit** (in `ptah-cli`): spawn a real `sleep` child via `std::process`,
  register it, run the sweep function, assert the child dies. Pins the
  mechanism deterministically.
- **e2e:** (a) SIGINT during a hung agent turn (`MOCK_HANG`) → exit 130 and
  the agent process count reaches zero (SIGTERM → 143) — the agent leg
  today's signal tests never cover; (b) hung agent + tagged `sleep` exec in
  flight, first signal then immediate second signal → both children dead, exit
  code matches the second signal. (b) is invariant-pinning by design: if
  teardown finishes before the second signal lands the assertions still hold
  (children already dead); a regression only fails the test when the window
  was open — which the unit test guarantees is covered by the sweep.

All tests stay offline: signals go to locally spawned `ptah` processes against
the mock agent, same pattern as the existing `signal_cancels_the_run` helper.

### D6: Observable behavioral delta

A second signal that is SIGTERM now exits **143** (was hardcoded 130). This is
the intended correction to the 128+n convention, called out here because it is
externally observable.

## Risks / Trade-offs

- [Stale registry entry sweeps a reused pid] → pid reuse between deregistration
  and sweep is the standard TOCTOU of every pid-based kill (including
  `kill_and_reap` today); the window is milliseconds and requires the recycled
  pid to lead a process group. Accepted.
- [Registration leaks on an unanticipated exit path] → both deregistration
  points sit at choke points every path funnels through (driver child
  disposal, `GroupKillGuard::Drop`); the sweep tolerates stale entries
  (`ESRCH` ignored) — a leak degrades to a wasted syscall, not a wrong kill.
- [e2e second-signal test cannot deterministically hold the window open] →
  accepted and compensated: the unit test pins the sweep mechanism; the e2e
  pins the user-visible contract (no orphans, matching exit code).
- [Force exit abandons joins/reaps] → none observable: SIGKILLed groups die;
  init adopts and reaps orphans.

## Migration Plan

Additive; no config or interface migration. Rollback is revert. The only
externally observable change is D6 (second-SIGTERM exit code 130 → 143).
