# Exploration: What happens to agent subprocesses when the ponos runtime is interrupted?

**Date:** 2026-08-21
**Status:** Explored — no change proposal yet
**Trigger question:** "When the runtime is interrupted, does it kill the subprocess agent immediately?"

## TL;DR

No. On an external interrupt (SIGINT / SIGTERM), ponos dies instantly via the OS
default handler and the agent subprocess is **orphaned** — ponos never kills it,
and the terminal's signal never reaches it either. Agents are only killed along
the in-process teardown paths (script error, explicit exit, normal completion),
which SIGKILL the whole process group promptly.

## Findings

### 1. There is no signal handling anywhere

`rg "ctrl_c|SIGINT|SIGTERM|signal"` across `src/` comes up empty. `src/cli.rs::main`
builds a multi-thread tokio runtime, `block_on`s the LocalSet-driven `script::run`,
and returns the exit code. No `tokio::signal`, no ctrlc crate, no atexit.

Consequence: SIGINT/Ctrl-C takes the kernel default action — ponos is terminated
immediately. No Rust destructors run (no `Drop` for `ChildGuard`, no
`kill_and_reap`), and none of the `script::run` match arms are reached.

### 2. The kill paths only run on script-error / explicit-exit / normal-end

`src/script/mod.rs::run` calls `teardown(&state, cancel)` from exactly three places:

| Path | `cancel` | What teardown does |
|---|---|---|
| Uncaught script error | `true` | `session/cancel` each session, close, join |
| `ponos.exit(n)` (ExitSignal) | `true` | same |
| Normal script completion (after `wait_outstanding`) | `false` | close + join only |

`teardown` → `SessionHandle::close()` → driver breaks its command loop →
`kill_and_reap(child)` (`src/acp/mod.rs`):

```rust
async fn kill_and_reap(mut child: async_process::Child) {
    #[cfg(unix)]
    unsafe {
        let pid = child.id() as i32;
        // The child is its own process-group leader (spawn_process sets this).
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.status().await; // reap
}
```

That is immediate and uncompromising: **SIGKILL the whole process group**, no
grace period, no polite `session/stop` first. (The only grace period in the
codebase is `CANCEL_GRACE = 2s` (`src/acp/mod.rs:32`), used on the *turn-timeout*
path: send `session/cancel`, wait up to 2s for the turn to settle, then raise
the timeout error regardless.)

### 3. The agent lives in its own process group — terminal SIGINT misses it

`agent-client-protocol` 1.3.0 (`acp_agent.rs`) spawns the agent with
`std_cmd.process_group(0)`, making the child the leader of its **own** process
group. This is deliberate: agents are commonly launched via `npx`/`uvx` wrappers,
and group-kill is the only way to reach the real agent behind the wrapper. The
crate's `ChildGuard::Drop` also SIGKILLs the group.

Flip side: when you press Ctrl-C in a terminal, the tty delivers SIGINT to the
**foreground process group** — ponos's group. The agent's group is not in it.

## Behavior map

```
                        interrupt path                in-process teardown paths
                        ─────────────                 ────────────────────────
 terminal Ctrl-C ──▶ tty sends SIGINT
                        │
                        ▼
                   ┌──────────┐
                   │  ponos   │  kernel default: process dies NOW
                   │ (killed) │  no Drop, no teardown, no kill
                   └────┬─────┘
                        │ stdin pipe closes (EOF)
                        ▼
                   ┌──────────┐
                   │  agent   │  orphaned, re-parented to pid 1
                   │ survives │  MAY exit on stdin EOF — not guaranteed
                   └──────────┘  (npx/uvx wrappers often ignore it)

 script error /    ──▶ teardown(cancel=true) ──▶ session/cancel ──▶ group SIGKILL
 ponos.exit(n)                                                     (immediate)

 script ends       ──▶ wait_outstanding ──▶ teardown(cancel=false) ──▶ group SIGKILL
 normally                                                            (immediate)

 turn timeout      ──▶ session/cancel ──▶ wait ≤2s (CANCEL_GRACE) ──▶ error raised
                                                       (agent NOT killed;
                                                        session stays usable)
```

## The gap

External interrupts orphan agent subprocesses. The infrastructure to fix it
already exists — `teardown(&state, true)` is exactly what we'd want to run — but
nothing invokes it on a signal. Note the interesting asymmetry: in-process
teardown is *more* aggressive than users might expect (instant SIGKILL, no
grace), while the signal path is *less* than they'd expect (nothing at all).

## Open questions (for a future change)

1. **Scope of the fix**: install SIGINT/SIGTERM handlers that run
   `teardown(&state, true)` and exit with a conventional code (128+n)? Or go
   further — first `session/cancel` + short grace (mirroring CANCEL_GRACE), then
   SIGKILL?
2. **First vs second signal**: common CLI convention is Ctrl-C once = graceful
   teardown, Ctrl-C twice = immediate abort. Worth adopting?
3. **Exit code contract**: `AGENTS.md` pins the exit-code contract
   (0/1/2/n-via-`ponos.exit`). Signal deaths would want a defined code too
   (e.g. 130) — needs to slot into that contract deliberately.
4. **Where to hook it**: a `tokio::signal` watch inside `script::run` (needs a
   cooperative abort of the Lua evaluation), vs a signal-safe flag the driver
   tasks poll. mlua async eval doesn't trivially cancel mid-chunk — this is the
   main design question.
5. **stdin-EOF reliance**: should ponos instead *close stdin first* and let a
   well-behaved agent shut itself down (the ACP `session/stop`-ish route),
   keeping SIGKILL as the backstop?

## Pointers (verified at time of writing)

- `src/cli.rs::main` — runtime setup, no signal handling
- `src/script/mod.rs` — `teardown`, `wait_outstanding`, `run` match arms
- `src/acp/mod.rs:32` — `CANCEL_GRACE = 2s`
- `src/acp/mod.rs:205-216` — `kill_and_reap` (group SIGKILL + reap)
- `agent-client-protocol-1.3.0/src/acp_agent.rs:213` — `process_group(0)` at spawn
- `agent-client-protocol-1.3.0/src/acp_agent.rs` — `ChildGuard::terminate` (group SIGKILL on Drop)
