# Exploration: Hexagonal restructure, change ③ — polish, enforcement, docs

**Date:** 2026-08-25
**Status:** Planned — blocked on the workspace split (change ②,
`hexagonal-workspace-split`, now proposed) landing
**Trigger question:** Final leg of the three-change hexagonal sequence
agreed in the design session.

## TL;DR

After ① (internal restructure) and ② (workspace split), make the
architecture **self-enforcing and documented**: dependency-direction lint
coverage, workspace lint policy, and updated contributor docs. No behavior
change.

## Work items when picked up

1. **Dependency-direction enforcement.** Floor pre-committed in the
   pre-② design session: a small custom integration test that greps
   `ptah-core` for forbidden imports (mlua beyond `task`'s data level,
   tokio I/O, fs, adapter crates) + workspace `[workspace.lints]` with
   `[lints] workspace = true` — zero deps, offline-safe, cannot fail to
   land. Nice-to-haves to evaluate at pick-up time, demoted:
   - crate-level: `unused_crate_dependencies` (if the pinned nightly
     behaves); the split itself already makes the big arrows
     compiler-enforced;
   - `cargo modules` / `cargo-deps` in CI if the toolchain supports it
     cleanly in the offline nix sandbox.
2. **Workspace lint policy** in root `Cargo.toml` (`[workspace.lints]`,
   shared `[lints] workspace = true`), aligned with what crane builds.
3. **Docs**: update `AGENTS.md` architecture section (crate map, the four
   funded ports, the closed-set rule — new ports require their own change);
   `README.md` pointer refresh; note the TUI readiness rationale (structured
   events, interaction-policy port) where the event types live.
4. **Straggler hygiene**: any leftover path shims from ①/② moves, dead
   module-tree artifacts, `git grep` for stale paths. **Carve-out:** the
   `ptah` facade in `ptah-cli` (member-crate re-exports + core compat
   re-exports of `config`/`task`) is load-bearing API the tests depend on —
   it stays. Also fix stale doc paths (e.g. `src/config.rs` references in
   AGENTS.md, owned by item 3).
5. Acceptance: `cargo test`, `cargo clippy -- -D warnings`,
   `nix flake check` green; `git diff tests/ examples/` empty.

## Settled decisions to respect

- The four funded ports (`AgentTransport`, `EventSink`, `ConfigSource`,
  `InteractionPolicy`) are a **closed set**; adding a port is its own change.
- Extension axes funded: transport, output targets (TUI eventually — no
  current plan, but events stay structured), config sources. Not funded:
  script host, check backends.
- No publishability ceremony for lib crates (private workspace members).
