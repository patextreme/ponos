# AGENTS.md

Rust CLI embedding a sandboxed Luau runtime that drives ACP-speaking agents
(Claude Code, Gemini CLI, …) over stdio. Single crate, two binaries:
`ponos` (user-facing CLI) and `mock-agent` (test fixture only — never part of
the CLI surface). API/behavior details are in `README.md`; don't duplicate them here.

## Commands

- `nix develop` — the only source of the toolchain on this machine (no system
  rustc, no rustup). All Rust work happens inside the dev shell.
- `cargo build` / `cargo test` — plain cargo works inside the dev shell.
- Single suite/filter: `cargo test --test e2e`, `cargo test --test acp <name>`.
- `nix build` — release build via crane.
- `nix flake check` — full suite in the sandbox.

Toolchain is a **pinned nightly** in `rust-toolchain.toml` (with rustfmt,
clippy, rust-analyzer); the Nix oxalica overlay derives its toolchain from
the same pin. Don't update the pin casually.

## Testing

- The suite is **fully offline**: tests never spawn real agents or touch the
  network. Integration tests (`tests/`) drive the in-repo mock agent
  (`src/bin/mock-agent/`), located via `env!("CARGO_BIN_EXE_mock-agent")`.
- Mock behavior is scripted with env vars: `MOCK_CHUNKS`, `MOCK_HANG`,
  `MOCK_PERMISSION`, `MOCK_TOOL`, `MOCK_PLAN`, `MOCK_USAGE`, `MOCK_STDERR`,
  `MOCK_DELAY_MS`, … Need a new agent behavior in a test? Extend the mock,
  don't reach for a real agent.
- `tests/examples.rs` runs the bundled `examples/` scripts through the real
  binary against the mock agent. Discovery is explicit — adding an example
  means adding its test function there. The Nix check keeps `examples/` in
  the build source so these tests pass in the sandbox.

## Architecture

- `src/cli.rs` — clap CLI. Exit codes are a contract: `0` success, `1` script
  error, never-observed task error, or (for `check`/`run` pre-flight) findings,
  `2` usage — and for `ponos check` also "check could not run" (missing
  script, registry discovery failure, luau-lsp absent) — `n` via `ponos.exit(n)`.
- `src/config.rs` — TOML agent registry. Project `.ponos/config.toml` (found
  upward from the invocation dir) overrides `~/.config/ponos/config.toml`
  **per agent name**; `${VAR}` interpolates from ponos's env at resolve time.
- `src/script/` — mlua (Luau) runtime. Scripts run sandboxed (no I/O, network,
  debug); `require.rs` resolves `.luau` modules relative to the requiring file
  and rejects escapes. One deviation: a `coroutine` table with only `yield`
  stays visible because the async runtime needs it.
- `src/acp/` — ACP client over stdio. ponos declares exactly one client
  capability — the non-interactive `session.configOptions` (with its
  `boolean` sub-capability). `session/request_permission` is answered
  headless allow-all (first `AllowAlways`, else the first offered allow
  option — README has the contract), every other agent→client request
  gets method-not-found so turns never hang. Per-session config option
  state lives in the driver (captured at `session/new`, folded from
  agent pushes and `setConfig`), serialized with turns via the turn lock.
- `src/check*` — `ponos check` pipeline (compile pass via mlua, full-moon
  static lints over the literal require graph, luau-lsp analyze with the
  embedded definitions) and the `run` pre-flight. Zero-execution: nothing
  here may call a compiled chunk.
- `src/task.rs` — `ponos.spawn`/`join`/`map` concurrency primitives.
- `src/render/` — streaming output renderer (color/quiet/verbose modes).
- `skills/` — canonical skill docs (`skills/ponos/SKILL.md`) that consumers
  download and copy into their own agent setup; deployed copies
  (e.g. `~/.pi/agent/skills/ponos`) are read-only symlinks into the nix
  store. Skill-doc changes are in-repo edits inside the change's edit
  roots, never out-of-repo follow-ups.

## Workflow: OpenSpec

This repo uses spec-driven development (`openspec/`). Changes live in
`openspec/changes/<id>/` (proposal.md, design.md, tasks.md, specs/). Use the
`.pi/skills/openspec-*` skills for the propose → apply → verify → archive
lifecycle instead of freelancing. `openspec/specs/` holds the synced truth.

Scratch/artifact dirs `.work/`, `.pi/taskflows/`, `worktrees/` are gitignored.
