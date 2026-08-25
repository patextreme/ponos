## Context

After ② lands, the workspace is eight private crates with compiler-enforced
arrows; ②'s design D1/D3 define the crate map and the permanent `ponos`
facade in `ponos-cli`. What remains unenforced is the *inside* of
`ponos-core`: its I/O-freedom and adapter-freedom are the invariants ①
audited by hand (task 1.6 grep). The docs still describe the pre-① tree.
This change was scoped in the pre-② design session: enforcement floor
pre-committed (Q5i), facade carve-out settled (Q4).

Precedent for precision: ①'s audit already tolerates settled exceptions —
core reads `std::env` (`HOME` in `turn`, `${VAR}` interpolation in
`config`), uses `tokio::sync` primitives and `tokio::task::spawn_local`
(task.rs drives mlua coroutines), carries data-level mlua in `task`, and
folds `agent_client_protocol` schema types in `turn`/`session`/`ports`.
The guard test's value is exactly that these exceptions are pinned in
code, not in memory.

## Goals / Non-Goals

**Goals:**

- Any commit that adds forbidden I/O or adapter coupling to `ponos-core`
  fails `cargo test -p ponos-core` — no code review vigilance required.
- The nightly-supported lint baseline is uniform across members via
  workspace inheritance, aligned with what crane/clippy build.
- AGENTS.md's architecture section describes the workspace truth (crate
  map, ports, closed-set rule) so ③'s knowledge doesn't live only in
  openspec archives.

**Non-Goals:**

- No behavior change of any kind; no new public API; no spec deltas.
- No enforcement beyond `ponos-core`'s boundary — crate-level arrows are
  the compiler's job post-②; per-module enforcement inside other crates
  is not funded.
- No `cargo modules`/CI tooling adoption unless it runs cleanly in the
  offline nix sandbox on the pinned nightly (evaluate, don't commit).
- No publishing ceremony, `deny(missing_docs)`, or public-API docs.

## Decisions

### D1 — Guard test: self-scanning integration test in `ponos-core`

`crates/ponos-core/tests/deps_guard.rs` walks
`CARGO_MANIFEST_DIR/src/**.rs` (no external crates, no network) and fails
on:

- `std::fs`, `std::process`, `std::net`, `std::io`, `std::thread`
  (path-form and `use`-form);
- `tokio::io`, `tokio::fs`, `tokio::net`, `tokio::process`,
  `tokio::time`;
- `use mlua` outside `src/task.rs` (data-level exception) — task.rs
  itself is allowed only `MultiValue`/`Function`/`Lua`-value-level items
  the fold needs (verified list from ①'s landed imports);
- `ponos_acp`/`ponos_render`/`ponos_check`/`ponos_luau`/`ponos_config`/
  `ponos_result`/`ponos_cli` in any form;
- `agent_client_protocol` outside `turn`/`session`/`ports`/`events`
  (schema-data exception), and never its connection/client types.

Allowlist (explicit paths + reasons in the test file): `std::env` reads,
`tokio::sync`, `tokio::task::spawn_local` (task.rs only), mlua in task.rs,
ACP schema types in the four modules above.

Rationale over alternatives: `cargo modules`/`cargo-deps` add toolchain
deps to the offline sandbox and break on nightly pins; nightly lints
(`unused_crate_dependencies`) enforce unused deps, not direction, and
their behavior on the pin is unverified. The grep test cannot fail to
land, runs everywhere cargo test runs, and its allowlist doubles as
documentation. It is the floor; the others stay optional enhancements.

### D2 — Lint policy: tighten inherited baseline, evaluate before adopting

② lands `[workspace.lints]` baseline + `[lints] workspace = true` members.
③ tightens within what the pinned nightly accepts (e.g. promote
`unsafe_op_in_unsafe_fn`, `clippy::unwrap_used` in non-test code — pick
at apply time from clippy's current warnings on the workspace) and
records any deliberate non-adoption. `unused_crate_dependencies` gets a
one-shot trial on the pin; adopted only if warning-free and offline-clean.

### D3 — Docs: AGENTS.md architecture section rewritten around the workspace

Replace the current per-directory `src/…` map with: the composition root
(`ponos-cli`, binaries + `ponos` facade), the adapter crates with one
line each (acp, render, check, luau, config, result), `ponos-core` with
the four funded ports and the **closed-set rule** (new ports require
their own change), and the TUI-readiness note (structured
`SessionEvent`s + `EventSink`/`InteractionPolicy` ports exist so a TUI
is an adapter away — no current plan). README: fix stale paths
(`src/bin/mock-agent/` and any other `src/` references) to `crates/…`;
no content rewrite.

### D4 — Straggler hunt with the facade carve-out

`git grep` for dead paths from ①/② moves (`src/config.rs`, `src/task.rs`,
`crate::result_wire`, pre-split module paths) in code and docs; delete
dead modules. The `ponos` facade's flat `pub use` list and the
`config`/`task` compat re-exports are **kept** — tests exercise the
system through them (settled Q4).

## Risks / Trade-offs

- [Guard test allowlist too tight → false failures on legitimate core
  growth] → every allowlisted exception carries its ①-era reason in a
  comment; widening is a one-line test edit, visible in review.
- [②'s apply deviates from its design (crate renames, extra moves) →
  this change's paths/allowlist stale] → tasks start with a
  reconciliation step against ②'s landed deviations; openspec
  update-change is the tool.
- [Nightly lint tightening fires on existing code → churn] → adopt only
  lints that pass or whose fixups are mechanical; record non-adoption
  rather than sprinkling `#[allow]`.
- [Grep-based test is lexical, not semantic ( fooled by macro-generated
  paths )] → acceptable: it guards human-written drift, same class as ①'s
  audit; the compiler guards the big arrows.

## Migration Plan

Single PR after ② is archived: guard test + lint tightening + docs +
straggler sweep, gated on `cargo test`, `cargo clippy --workspace --
-D warnings`, `nix flake check`. Rollback is revert; nothing persists
outside the repo.

## Open Questions

None blocking. The concrete lint set to promote (D2) is deliberately an
apply-time choice from clippy's live output; it cannot change the
approach or task breakdown.
