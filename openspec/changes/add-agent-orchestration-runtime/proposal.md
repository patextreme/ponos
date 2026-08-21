# Proposal: Agent Orchestration Runtime (v1)

## Why

Scripted multi-agent workflows (fan-out reviews, pipeline drafts, judge loops) currently require either a full editor integration or hand-rolled subprocess plumbing around each agent. There is no standalone runtime where an orchestration is just a small, versionable script. ponos fills that gap: a Rust CLI that runs Luau scripts which drive any ACP-speaking agent over stdio.

## What Changes

- New greenfield Rust CLI crate (`ponos`) with a single subcommand `ponos run <script.luau>`.
- Embedded Luau scripting (mlua, sandboxed) exposing a `ponos` namespace: `agent`, `session`, `prompt`, `spawn`, `join`, `map`, `sleep`, `log`, `exit`.
- ACP client implementation (Zed `agent-client-protocol` crate) that spawns agents as subprocesses and drives prompt turns; declares **no client capabilities** (agent→client requests are denied).
- Synchronous-looking script calls that yield under the hood (mlua async + tokio): `reply = session:prompt("...")` blocks the script, not the runtime, enabling parallel fan-out.
- Agent registry configuration in TOML (project `.ponos/config.toml` overriding user `~/.config/ponos/config.toml`), with inline spec override from scripts.
- Live terminal rendering of streaming agent output with per-session attribution (prefix + ANSI color), controlled by `--quiet` / `--verbose` / `-vv` / `--no-color`.
- Mock ACP agent fixture (in-repo Rust binary) for deterministic offline integration tests.
- Nix flake (flake-parts, modules under `./nix/`) with oxalica nightly toolchain via `rust-toolchain.toml` and crane packaging for x86_64-linux, aarch64-linux, aarch64-darwin.

Out of scope for v1 (explicit non-goals): thin single-prompt driver mode; session resume/load; mode switching; multimodal prompt blocks; script access to fs/http/subprocess; client-side permission prompts / fs / terminal / elicitation; `race` primitives; REPL; package ecosystem; crates.io publishing.

## Capabilities

### New Capabilities

- `cli`: The `ponos run` command surface — argument parsing, output flags, exit codes, script-end semantics.
- `scripting`: The Luau scripting environment — sandboxed stdlib, module resolution, the `ponos` namespace (agents, sessions, tasks, concurrency, errors, cancellation).
- `agent-sessions`: ACP session lifecycle — agent process spawning, session creation, prompt turns, streaming updates, cancellation, process teardown.
- `agent-registry`: Agent configuration discovery — TOML registry format, project/user precedence, environment interpolation, inline script override.

### Modified Capabilities

None (greenfield — no existing specs).

## Impact

- Entirely new codebase: `src/` (CLI, Luau bindings, ACP client wiring, rendering), `tests/` plus a mock agent fixture binary, `nix/` modules.
- New dependencies: `clap`, `mlua` (`luau` + `async` features), `agent-client-protocol`, `tokio`, `serde`/`toml`, `tracing`.
- External dependency: ACP-compatible agent binaries (e.g. `@agentclientprotocol/claude-agent-acp`, `gemini-cli`) resolved via user config; not vendored.
- Developer toolchain pinned to a nightly Rust via `rust-toolchain.toml` consumed by the oxalica overlay; builds must also work with plain `cargo build`.
