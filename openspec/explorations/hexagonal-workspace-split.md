# Exploration: Hexagonal restructure, change ② — physical workspace split

**Date:** 2026-08-25
**Status:** Planned — blocked on `hexagonal-internal-restructure` (change ①) landing
**Trigger question:** "Structure the crate in a modular way that is easy to
extend, for longevity" → settled as a three-change sequence; this is ②.

## TL;DR

After change ① (`hexagonal-internal-restructure`) has rebuilt the module tree
inside the single crate, split it into a virtual cargo workspace under
`crates/`. Because ① maps every module boundary 1:1 onto a future crate, this
change is a mechanical file move + nix update, not a redesign. Zero behavior
change; `tests/` stay byte-identical and green.

## Settled decisions (from the design session — do not relitigate)

- **Private workspace members.** No publishing, no semver/docs ceremony; the
  `ponos` binary is the only artifact that matters. Tighten later if embedding
  by third parties ever matters.
- **Both binaries (`ponos`, `mock-agent`) live in the `ponos-cli` package** so
  `env!("CARGO_BIN_EXE_mock-agent")` keeps working and all 129 integration
  tests stay byte-identical in `ponos-cli/tests/`. "Not part of the CLI
  surface" is a behavioral contract, not a packaging one.
- Luau host is a fixed fixture (no port), but still gets its own crate for
  dependency isolation.

## Crate map (target)

| Crate | Contents | Depends on |
|---|---|---|
| `ponos-core` | pure domain: turns/folds, task semantics, `ResultContract`, config model+interp, structured events, error types, **ports** (`AgentTransport`, `EventSink`, `ConfigSource`, `InteractionPolicy`) | std, serde, jsonschema |
| `ponos-acp` | ACP-over-stdio adapter (process/proto/driver from ①) | core, agent-client-protocol, async-process |
| `ponos-luau` | mlua sandbox, `ponos.*` bindings, require, task bridging | core, mlua |
| `ponos-check` | check/preflight pipeline; `TYPE_DEFINITIONS`; luau-lsp shell-out | core, mlua, full-moon |
| `ponos-config` | TOML/fs discovery — the only `ConfigSource` impl (317 lines today; collapsible into core's `config::fs` if it bothers anyone) | core |
| `ponos-render` | terminal line renderer consuming structured events | core |
| `ponos-cli` | composition root: clap, wiring, both binaries, bridge + result UDS channel | everything |

## Work items when picked up

1. Virtual root `Cargo.toml` (workspace-only, no root package), packages under
   `crates/<name>/`; move modules from ①'s tree into their crates.
2. Workspace-level dependency/lint inheritance (`[workspace.dependencies]`).
3. Nix/crane update: source filtering per crate; **keep `examples/` in the
   build source** so `tests/examples.rs` still passes in the sandbox.
4. Both `[[bin]]`s (`ponos`, `mock-agent`) in `ponos-cli`; `tests/` moves with
   them untouched.
5. Acceptance: `cargo test`, `cargo clippy -- -D warnings`,
   `nix flake check` green; `git diff tests/ examples/` empty.

## Interactions to remember

- Must land **after** ①; ideally after `richer-render-logging` rebases onto
  ①'s structured events (that change builds on today's `DisplayEvent`).
- Followed by change ③ (lint enforcement + docs) — see
  `hexagonal-polish.md`.
