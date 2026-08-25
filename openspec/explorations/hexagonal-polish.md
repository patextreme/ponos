# Exploration: Hexagonal restructure, change ③ — polish, enforcement, docs

**Date:** 2026-08-25
**Status:** Planned — blocked on the workspace split (change ②, see
`hexagonal-workspace-split.md`) landing
**Trigger question:** Final leg of the three-change hexagonal sequence
agreed in the design session.

## TL;DR

After ① (internal restructure) and ② (workspace split), make the
architecture **self-enforcing and documented**: dependency-direction lint
coverage, workspace lint policy, and updated contributor docs. No behavior
change.

## Work items when picked up

1. **Dependency-direction enforcement.** Options to evaluate at pick-up time
   (in rough preference order):
   - crate-level: `unused_crate_dependencies` + `deny(missing_docs)` off
     (private), plus the workspace split itself already makes the big arrows
     compiler-enforced;
   - module-level (within `ponos-core`): `cargo modules` / `cargo-deps`
     checks in CI if the toolchain supports it cleanly on the pinned nightly;
   - a tiny custom test that greps `use crate::` in core and fails on
     forbidden imports (mlua/tokio-I/O/fs) — cheap, zero deps, runs in the
     offline sandbox.
2. **Workspace lint policy** in root `Cargo.toml` (`[workspace.lints]`,
   shared `[lints] workspace = true`), aligned with what crane builds.
3. **Docs**: update `AGENTS.md` architecture section (crate map, the four
   funded ports, the closed-set rule — new ports require their own change);
   `README.md` pointer refresh; note the TUI readiness rationale (structured
   events, interaction-policy port) where the event types live.
4. **Straggler hygiene**: any leftover re-export shims from ①/② removal,
   dead module-tree artifacts, `git grep` for stale paths.
5. Acceptance: `cargo test`, `cargo clippy -- -D warnings`,
   `nix flake check` green; `git diff tests/ examples/` empty.

## Settled decisions to respect

- The four funded ports (`AgentTransport`, `EventSink`, `ConfigSource`,
  `InteractionPolicy`) are a **closed set**; adding a port is its own change.
- Extension axes funded: transport, output targets (TUI eventually — no
  current plan, but events stay structured), config sources. Not funded:
  script host, check backends.
- No publishability ceremony for lib crates (private workspace members).
