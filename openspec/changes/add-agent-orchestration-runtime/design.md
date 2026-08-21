# Design: Agent Orchestration Runtime (v1)

## Context

Greenfield repo (LICENSE + OpenSpec scaffolding only). Everything is new; the settled product decisions live in `proposal.md` and the four specs. External constraints that shape this design:

- **ACP** (v1) is JSON-RPC over the agent's stdio; the client role spawns the agent. The official Rust crate `agent-client-protocol` (Zed) provides typed `Client`/`Agent` traits and connection plumbing — it powers Zed's external-agent integration, so it is battle-tested for exactly this use.
- **mlua 0.12** supports Luau via the `luau` feature and composes with the `async` feature: async Rust functions yield the running Luau coroutine while a tokio future is pending, and a top-level chunk runs as an async coroutine (`eval_async`). Sandboxing is `Lua::sandbox(true)` + selective stdlib loading; Luau `require` is customizable through the `Require` trait (default `FsRequirer` does relative resolution).
- Luau is deliberately **not** a coroutine-first language for users: scripts look synchronous; concurrency must be provided by the host.

## Goals / Non-Goals

**Goals:**
- A small, stable, honestly-documented Luau surface where the 90% case (sequential + fanned-out prompts) reads like plain script.
- Deterministic, offline-testable: all ACP behavior testable against a mock agent binary; no network in CI.
- Clean layering so later capabilities (fs/http modules, modes, resume, race) are additive.

**Non-Goals** (design-level, beyond proposal non-goals):
- No TUI/curses rendering — plain stdout writes with ANSI color codes only.
- No attempt to multiplex multiple sessions over one agent process (per-session processes are the settled model).
- No cross-run persistence or caching of anything.

## Decisions

### D1: Crate layout — single binary, module-per-concern

```
src/
  main.rs        # clap CLI, tokio runtime bootstrap, exit-code plumbing
  config.rs      # registry TOML load/merge/interpolation
  script/        # mlua setup: sandbox, stdlib, require, ponos bindings
  acp/           # client wiring: spawn, handshake, session driver, update fan-in
  render/        # stdout streaming renderer (prefix + color, flag handling)
  task.rs        # spawn/join/map/sleep runtime on tokio
tests/           # integration tests driving the mock agent
fixtures/mock-agent/  # standalone ACP agent binary used by tests
```

One crate for v1; split only when a second consumer appears. *Alternative:* cargo workspace — rejected, overhead without benefit yet.

### D2: The async bridge — Luau coroutines over tokio

The runtime creates one `Lua` instance with `luau` + `async` features, `Lua::sandbox(true)`, and a curated stdlib set. The entry chunk runs via `eval_async` inside a tokio runtime. Every blocking-looking API (`prompt`, `sleep`, `task:await`) is a `create_async_function` that awaits a tokio future; mlua yields the Luau coroutine while waiting, so other tasks (each their own coroutine) progress.

Task bookkeeping: `ponos.spawn(fn)` wraps `fn` in a Lua coroutine, registers it in a task registry (id, coroutine, result/error slot, completion flag), and resumes it immediately — no tokio task per Lua task needed; resumption happens from the event loop as futures complete. `join`/`map` await completion flags. `map` schedules up to `concurrency` coroutines at once. Each task's error slot also tracks whether the error was ever delivered (observed via `await`/`join` or carried in `map` results); at script end, after draining, any never-delivered task error is printed to stderr and the run exits 1 — unhandled task errors are fatal (settled decision), and an earlier `ponos.exit(code)` overrides with its own code.

*Alternative:* callback/event-driven API — rejected in design tree (Q2): sync-looking calls are the least surprising scripting model.

### D3: ACP session driver — per-session actor

Each session owns: a spawned child process, an `agent-client-protocol` connection, and a driver task. The driver receives update notifications, folds them into the in-flight turn's accumulator (text chunks, usage, tool/plan display events), and forwards display events to the renderer. `prompt` sends the JSON-RPC request, then awaits the response future while the driver keeps consuming notifications (JSON-RPC allows interleaved notifications with an outstanding request).

ponos's `Client` trait implementation answers every agent-to-client request with a JSON-RPC `-32601` (method not found) error — capabilities are simply never declared in `initialize`, so well-behaved agents won't ask, and misbehaving ones get a prompt error rather than a hang.

Cancellation: `session:cancel()` sends `session/cancel` and marks the turn cancelled; the pending response resolves with `stopReason = "cancelled"`. `timeout_ms` wraps the prompt future in `tokio::time::timeout`, firing the same cancel path before raising.

### D4: Renderer — attributed lines, no TUI

Renderer receives `(session_label, event)` and writes prefixed lines: `[claude/s1] …` with a per-session ANSI color assigned round-robin from a small palette (distinct hues, `--no-color` drops the codes). Message chunks are line-buffered so prefixes land on real lines. Tool calls render as one-line summaries (`tool: name (status)`); plan updates as a compact status list. `-vv` passes agent stderr through with the same prefix scheme.

*Alternative:* per-session panes (TUI) — rejected: complexity, pipe-hostility.

### D5: Config — resolved once at startup

`config.rs` loads user then project TOML, merges (project wins per-entry, wholesale entry replacement), and interpolates `${VAR}` eagerly. Resolved specs are immutable for the run. Inline spec tables skip lookup. `env` merges over the inherited process environment at spawn time.

### D6: Testing — mock agent as first-class fixture

`fixtures/mock-agent` is a small binary implementing the agent side of ACP with scriptable behavior: echo prompts, configurable chunk streams/delays, tool-call updates, cancellation compliance, and a mode to request a permission (asserting ponos denies). Integration tests spawn it via `ponos run`-equivalent in-process entry points. Unit tests cover config resolution, require resolution, and task semantics. Real adapters (claude-agent-acp etc.) are manual smoke checks only.

### D7: Nix — flake-parts with per-concern modules

`flake.nix` is a thin flake-parts shell; behavior lives in `./nix/*.nix` modules: `nix/toolchain.nix` (oxalica rust-overlay reading the pinned `rust-toolchain.toml` nightly channel, shared by devshell and crane), `nix/package.nix` (crane lib building the workspace, cargo-auditable off for v1), `nix/devshell.nix`, `nix/apps.nix`. Outputs: `packages.default`, `devShells.default`, `apps.default` for x86_64-linux, aarch64-linux, aarch64-darwin. Mock agent fixture builds inside the same derivation (`cargo build --bins`), used by `checks` via a nix test that runs the integration suite against it.

*Alternative:* monolithic flake — rejected per settled decision (modularity under `./nix`).

## Risks / Trade-offs

- [mlua async + Luau edge cases (yielding across pcall boundaries, error re-raise fidelity)] → Mitigation: pin mlua version; add focused tests for pall-around-prompt and error re-raise at await; worst case, wrap risky yields with pcall-safe trampolines.
- [`agent-client-protocol` crate API churn (pre-1.0)] → Mitigation: isolate all crate usage in `src/acp/`; upgrade is a one-module change.
- [Adapters violate ACP (hang on unsupported requests, misframe JSON)] → Mitigation: per-request timeout on agent→client requests before answering unsupported; stderr capture for diagnostics; mock agent encodes the spec-exact behavior so ponos's side stays provably correct.
- [Per-session processes cost memory with wide fan-out] → Accepted per decision tree (Q15=B); document expected ceiling; process pooling is a later additive change.
- [Nightly toolchain drift breaks builds] → Mitigation: pinned exact nightly version in `rust-toolchain.toml`; CI/devshell both consume the same pin; plain `cargo build` on stable is best-effort, not supported.

## Migration Plan

Greenfield — no migration. Rollback = delete the built artifact; the repo has no prior behavior.

## Open Questions

None — the design tree was fully settled during grilling (Q1–Q27 + assumptions). Later additive unknowns (e.g. exact color palette, prefix separator glyph) do not affect specs, approach, or tasks.
