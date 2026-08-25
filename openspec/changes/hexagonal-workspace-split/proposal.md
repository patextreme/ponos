## Why

Change ① (`hexagonal-internal-restructure`) rebuilt the module tree so every
module boundary maps 1:1 onto a future crate; the dependency-direction rules
(inward arrows, adapter isolation, the four-port closed set) are currently
convention and grep-audits only. Splitting the single crate into a cargo
workspace makes those arrows compiler-enforced and unblocks change ③ (lint
enforcement + docs). This is the mechanical second leg of the three-change
hexagonal sequence; zero behavior change.

## What Changes

- Convert the single `ponos` package into a **virtual cargo workspace** with
  eight private member crates under `crates/`: `ponos-core`, `ponos-acp`,
  `ponos-luau`, `ponos-check`, `ponos-config`, `ponos-render`, `ponos-result`
  (new home for `src/result_wire.rs`), and `ponos-cli` (composition root).
- `ponos-cli` keeps **both binaries** (`ponos`, `mock-agent`) and the
  integration tests (`tests/` move with them), so `env!("CARGO_BIN_EXE_mock-agent")`
  keeps resolving and the test surface stays intact.
- `RunConfig` gains an **injected transport** field; the `script → acp`
  composition line (`default_transport()`) moves from `ponos-luau` into
  `ponos-cli`. This kills the last adapter→adapter arrow at crate level —
  the one deviation ① recorded as reserved for this change.
- The `ponos` library name survives as the **permanent facade** of
  `ponos-cli`: `pub use` re-exports (`ponos::acp`, `ponos::render`,
  `ponos::script`, `ponos::config`, `ponos::task`, …) so all existing
  imports — tests included — keep working unchanged.
- Workspace-level `[workspace.dependencies]` and `[workspace.lints]`
  inheritance; nix/crane updated for per-crate source filtering with
  `examples/` kept in the build source.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. Pure packaging/refactor change: no user-visible behavior, CLI surface,
exit codes, or output bytes change. `skip_specs: true` is set in
`.openspec.yaml`.

## Impact

- **Code**: every module under `src/` moves to `crates/<crate>/src/` per the
  crate map in design.md; `src/main.rs` + `src/bin/mock-agent/` land in
  `crates/ponos-cli/`. Module contents are moved, not rewritten.
- **Tests**: `tests/` move to `crates/ponos-cli/tests/` unchanged except the
  two `RunConfig` literal sites (`tests/script.rs`, `tests/e2e.rs`) gain the
  mechanical `transport:` line; zero expectation changes. `examples/`
  untouched.
- **Build**: root `Cargo.toml` becomes workspace-only; nix flake/crane
  source filtering updated per crate; pinned nightly toolchain unchanged.
- **Followed by**: change ③ (`hexagonal-polish` exploration) — dependency
  lint floor, workspace lint policy, contributor docs.
