## Why

The hexagonal sequence ① (internal restructure) and ②
(`hexagonal-workspace-split`) make the dependency direction
compiler-enforced at crate level, but the finer-grained invariants are
still convention: nothing stops `ponos-core` from growing a `tokio::fs`
call or a stray adapter import, the workspace lint baseline is minimal,
and the contributor docs (AGENTS.md architecture section, README
pointers) still describe the pre-① tree — they reference `src/task.rs`
and `src/config.rs`, which no longer exist. This change closes the
sequence: enforcement that cannot drift, and docs that match the landed
crate map. Zero behavior change.

**Dependency:** ② has landed and is archived
(`2026-08-26-hexagonal-workspace-split`). Artifacts here are written
against ②'s design D1–D4 (crate map, facade, workspace mechanics);
task 0.1 reconciles this change's design/tasks against ②'s recorded
apply-time deviations before any code is touched.

## What Changes

- **Dependency-direction guard test**: an offline integration test in
  `ponos-core` that scans its own source for forbidden imports
  (`std::fs`/`std::process`/`std::net`/`std::io`, `tokio::io`/`fs`/`net`/
  `process`/`time`, mlua outside `task`'s data level, any adapter crate)
  with a pinned allowlist for the settled exceptions (`tokio::sync`,
  `tokio::task::spawn_local`, `std::env` reads, `agent_client_protocol`
  schema types). Zero external deps; runs in `cargo test -p ponos-core`
  and the nix sandbox.
- **Workspace lint policy**: tighten `[workspace.lints]` beyond ②'s
  baseline as far as the pinned nightly supports cleanly; evaluate the
  demoted nice-to-haves (`unused_crate_dependencies`, `cargo modules`)
  and adopt only if they work offline.
- **Docs**: rewrite AGENTS.md's architecture section around the crate
  map (composition root + facade, adapters, core, the four funded ports
  and the closed-set rule — new ports require their own change; TUI
  readiness rationale with the event types) and refresh stale paths
  elsewhere in AGENTS.md; refresh stale paths in README and
  `skills/ponos/SKILL.md` (e.g. `src/bin/mock-agent/` →
  `crates/ponos-cli/src/bin/mock-agent/`).
- **Straggler hygiene**: sweep for stale paths/dead modules left by ①/②
  moves — explicitly **not** the `ponos` facade in `ponos-cli`, which is
  load-bearing API, and **not** the `examples/` payload path strings,
  which are fictional review targets in the test-pinned corpus (design
  D4 records both carve-outs).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. Tooling/docs/hygiene only: no user-visible behavior, CLI surface,
exit codes, or output bytes change. `skip_specs: true` is set in
`.openspec.yaml`.

## Impact

- **Code**: one new test file in `crates/ponos-core/tests/`; lint
  attribute edits in workspace/member manifests (may surface fixups in
  member crates where new lints fire).
- **Docs**: `AGENTS.md`, `README.md`, `skills/ponos/SKILL.md`.
- **Preceded by**: ① `hexagonal-internal-restructure` (archived),
  ② `hexagonal-workspace-split` (archived 2026-08-26).
