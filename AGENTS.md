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
  error or never-observed task error, `2` usage, `n` via `ponos.exit(n)`.
- `src/config.rs` — TOML agent registry. Project `.ponos/config.toml` (found
  upward from the invocation dir) overrides `~/.config/ponos/config.toml`
  **per agent name**; `${VAR}` interpolates from ponos's env at resolve time.
- `src/script/` — mlua (Luau) runtime. Scripts run sandboxed (no I/O, network,
  debug); `require.rs` resolves `.luau` modules relative to the requiring file
  and rejects escapes. One deviation: a `coroutine` table with only `yield`
  stays visible because the async runtime needs it.
- `src/acp/` — ACP client over stdio. ponos declares no client capabilities:
  every agent→client request gets method-not-found so turns never hang.
- `src/task.rs` — `ponos.spawn`/`join`/`map` concurrency primitives.
- `src/render/` — streaming output renderer (color/quiet/verbose modes).

## Workflow: OpenSpec

This repo uses spec-driven development (`openspec/`). Changes live in
`openspec/changes/<id>/` (proposal.md, design.md, tasks.md, specs/). Use the
`.pi/skills/openspec-*` skills for the propose → apply → verify → archive
lifecycle instead of freelancing. `openspec/specs/` holds the synced truth.

Scratch/artifact dirs `.work/`, `.pi/taskflows/`, `worktrees/` are gitignored.
