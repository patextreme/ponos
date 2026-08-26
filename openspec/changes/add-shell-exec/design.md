## Context

See `proposal.md` for motivation (anchor: deterministic `gh pr list --json` → Luau filter → agent fan-out). The constraints that shape this design: ponos-core is I/O-free and adapter-free, enforced by `crates/ponos-core/tests/deps_guard.rs`; the port set (`AgentTransport`, `ConfigSource`, `EventSink`, `InteractionPolicy`) is closed by design — a new port is "a design decision that gets its own change," which this change is; ponos-luau reaches the world only through capabilities injected via `RunConfig` (today: `transport`, `sink`); all capabilities are composed in ponos-cli. `ponos-luau` already depends on tokio and serde_json. The existing event vocabulary (`SessionEvent` in `ponos-core/src/events.rs`) is session-attributed by label at the sink boundary.

## Goals / Non-Goals

**Goals:**

- A minimal, opinionated `ponos.exec` whose contract mirrors the turn contract where they overlap (options table with `timeoutMs`, nil = no limit, expiry kills and raises).
- Keep core pure, the runtime crate capability-injected, and the composition root the only world-toucher.
- Make exec observable enough that a headless run never looks dead during a slow command.

**Non-Goals** (all deferred, none breaking):

- Task-model participation (`spawn`/`join` composition for execs). If wanted later, it is an additive `spawn` kind; `exec` returning a value does not preclude it.
- argv-array invocation, `cwd`/`env` overrides, live output streaming, gating flags, stdlib expansion beyond `ponos.json`.

## Decisions

### D1: A fifth port, `ProcessRunner`, funded deliberately

`trait ProcessRunner: Send + Sync` in `crates/ponos-core/src/ports.rs`: `fn run<'a>(&'a self, cmd: &'a str, timeout_ms: Option<u64>) -> Pin<Box<dyn Future<Output = Result<ExecOutcome, ExecError>> + Send + 'a>>` with pure data types — `ExecOutcome { exit_code: Option<i32>, stdout: String, stderr: String, timed_out: bool }`, `ExecError::Spawn(String)`. Pure trait, no I/O imports, so `deps_guard` stays green.

- Why a port with one impl forever: not swappability — visibility. "Who may touch the world" is decided at the composition boundary, in the file where the other four such decisions live, rather than buried in a Luau binding. ponos-luau keeps its story ("curated environment plus injected capabilities"); a consumer of `run()` that injects no runner gets a clean "no runner injected" error instead of ambient shell.
- Alternative rejected: ambient `tokio::process` call inside `bindings.rs`. Cheaper, but it would give the "sandboxed runtime" crate world powers silently and set the precedent that capabilities can be ambient.
- `RunConfig` gains `process_runner: Option<Arc<dyn ProcessRunner>>`; absent → `ponos.exec` raises a runtime error (the CLI always injects).

### D2: Tokio impl in ponos-cli, process groups via `setsid`

The impl lives at the composition root (ponos-cli) beside the ACP transport wiring: spawn `/bin/sh -c <cmd>` with `process_group(0)` (tokio's `Command::process_group` on Unix) so the whole pipeline is one killable group; stdin `Stdio::null()`, stdout/stderr piped and read concurrently (`tokio::join!` of the two reads plus the wait, so a child that fills a pipe can't deadlock); `tokio::time::timeout` around the whole thing; on expiry `kill_process_group()` (SIGKILL, then await reap). The mock-free test story: real `sh` builtins are offline and deterministic, so no new mock surface is needed.

### D3: The binding is a thin async callback that owns events and teardown registration

`bindings.rs` adds `ponos.exec` as an async callback mirroring `session:prompt`'s shape: parse args (string, optional table — a bare number for opts is a type error), read the runner from runtime state, emit a start event, register the exec in an in-flight set, await the port, deregister, emit an end event, build the camelCase result table (`exitCode`, `stdout`, `stderr` — matching `stopReason`/`cacheRead` idiom). Timeout and spawn errors raise `mlua::Error::runtime` with a message naming the command and budget.

- Teardown: `RuntimeState` grows an in-flight exec registry (a `RefCell<HashSet<child guard>>` or equivalent keyed by a kill handle returned by the port). On script end — normal exit, error, `ponos.exit`, or outer cancel — teardown kills every live process group. The port returns a kill-capable handle or the outcome future exposes `abort()` semantics; concretely, the runner's future is wrapped so that dropping/cancelling it kills the group (cancel-safety: the tokio impl performs the kill on drop, making the registry a bookkeeping optimization over "cancel the task" rather than the kill mechanism itself).

### D4: Events — a non-session-scoped shape

`SessionEvent` grows `ExecStart { command: String }` and `ExecEnd { command: String, exit_code: Option<i32>, timed_out: bool, duration_ms: u64 }`. Attribution wrinkle: `EventSink::emit(label, event)` attributes by session label, but an exec belongs to the script, not a session. Decision: emit with a reserved pseudo-label `"exec"` (not a legal session label — script labels are `ponos.log`-scoped; session labels are user-set or `s1, s2, …`), and the renderer treats the pseudo-label as script-level attribution. This avoids widening the `EventSink` port signature (a breaking change to every impl) for one event class; a future TUI can special-case the same label. Alternative rejected: a second sink method or a `Source` enum parameter on `emit` — both break the port's existing implementors for marginal gain.

### D5: `ponos.json` as pure bindings, not a port

`ponos.json.parse`/`stringify` are pure data transforms (`serde_json` already in tree via mlua); they need no port and no injection. `parse` maps JSON arrays to Luau tables with consecutive integer keys, objects to string-keyed tables, `null` to `nil` (same mlua serde options the prompt `result` field already uses — `serialize_none_to_null(false)`), and raises on malformed input. `stringify(value, { indent = n })` uses serde's pretty printer with configurable indent; non-string table keys are rejected with a clear error (JSON objects have string keys only).

### D6: Hygiene surface

- Definitions: `ponos.exec` typed as `exec(cmd: string, opts: ExecOptions?) -> ExecResult` with `ExecOptions = { timeoutMs: number? }`; `ponos.json` typed as a table with `parse`/`stringify`. Both added to the embedded definitions file, the runtime probe test, and strict-mode analysis of examples/fixtures (per the "Definitions stay synchronized" requirement).
- README: API-table rows for `ponos.exec` and `ponos.json`; the sandbox paragraph updated to state the injected-capability story honestly (ambient globals still expose no subprocess execution; `ponos.exec` is the deliberate door, ungated, because every run already implies arbitrary shell via the headless allow-all posture).
- Example: `examples/exec-pipeline.luau` (name TBD) — offline, using `git log`/`printf` pipelines + `ponos.json.parse`, no agent-less variant needed but at least one mock-agent turn to show interleaving; registered in `crates/ponos-cli/tests/examples.rs`.

## Risks / Trade-offs

- [Shell-string invocation invites quoting bugs with dynamic data] → Document the pattern of building arg lists with `string.format("%q", …)`-style quoting in the README; argv form remains an additive later option.
- [Blocking exec parks a coroutine; authors may expect workflow-wide blocking] → Document explicitly: spawned tasks keep progressing; exec participates in no join/parallel (Non-Goal).
- [Captured output is invisible during long commands] → Lifecycle lines give start/end visibility; live streaming is an explicit Non-Goal; scripts can `ponos.log` progress around calls.
- [`/bin/sh` varies across platforms (dash vs bash)] → Contract is POSIX sh semantics; document that bashisms are not guaranteed. (Nix CI pins a shell; a documented limitation elsewhere.)
- [Pseudo-label `"exec"` could collide with a user-set session id] → Reserve it: session id validation rejects `"exec"` going forward (existing scripts using that exact id are unaffected beyond needing a rename; called out in README).
- [Zombie reaping on kill paths] → The runner awaits the child after SIGKILL (tokio `wait` on the killed child) before returning; teardown waits briefly for all in-flight kills.

## Migration Plan

Purely additive: new port, new `RunConfig` field (with a default of `None`), new namespace members, new events. No existing API, wire protocol, or exit-code behavior changes. The `scripting` spec's sandbox sentence is amended in the same change. Rollback is revert; no persisted state involved.

## Open Questions

None blocking. (Deferred and non-breaking: argv-array invocation; `cwd`/`env` options; live streaming; task-model participation.)
