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
`std::sync`, `tokio::sync`, `tokio::task::spawn_local` (task.rs only),
mlua in task.rs, ACP schema types in the four modules above. The scanner
strips `//`, `///`, and `//!` comments before matching: `lib.rs`'s module
docs mention `mlua::Error` and `agent_client_protocol` in prose, and doc
text is not an import.

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

**Apply-time amendment (2.2): `unused_crate_dependencies` not adopted.**
The one-shot trial fired 273 warnings, all structural: the lint checks
per-*target* while `[dependencies]` are per-*package*, so every bin and
integration test that doesn't name every package dep fires (`deps_guard`
doesn't use `tokio`; `mock-agent` doesn't use `ponos_check`; …), and it
flags deps used only through `#[derive]` (`serde`). Adoption would
require ~273 `use x as _;` sprinkles — the `#[allow]`-class churn this
change forbids. Dropped; clippy stays green without it. (Dependency
hygiene at crate level remains the compiler's and crane's job.)

**Apply-time amendment (2.1).** Baseline inventory on the pin
(`--all-targets`): one default-on warning (`needless_borrows_for_generic_args`
in `tests/e2e.rs`) and otherwise clean. Adopted set: rustc
`unsafe_op_in_unsafe_fn`, `rust_2018_idioms`, `elided_lifetimes_in_paths`,
`missing_abi` (all fire zero times — floor documentation, not churn), plus
the mechanical clippy set `redundant_clone`, `assigning_clones`,
`map_unwrap_or`, `redundant_closure_for_method_calls`,
`semicolon_if_nothing_returned`, `uninlined_format_args`,
`unnecessary_join`, `unnecessary_debug_formatting`, `enum_glob_use`,
`needless_continue`, `if_not_else`, `bool_to_int_with_if` — ~45 fixups,
applied via `cargo clippy --fix` plus 5 hand edits (three `clone_from`
swaps in mock-agent, one dead `continue` in ponos-result, the guard
test's own panic formatting). Deliberate non-adoptions, with reasons:

- `clippy::unwrap_used` (256 fires) / `expect_used` (109) /
  `indexing_slicing` (114): not a mechanical pass — Cargo `[lints]`
  cannot scope to non-test code, so adoption would demand ~300 site
  rewrites (behavior risk) or `#[allow]` sprinkling; both breach this
  change's constraints. The D1 guard test covers the invariant these
  were a proxy for where it matters (I/O discipline).
- `clippy::pedantic` as a group: fires ~250 across docs lints
  (`missing_errors_doc` 34, `missing_panics_doc` 24, `must_use_candidate`
  48) and case-by-case judgment lints (`cast_*`, `too_many_lines`,
  `single_match_else`, …) — public-API docs ceremony is an explicit
  non-goal and the rest is not mechanical. Individual mechanical
  members were cherry-picked instead (see adopted set).
- `clippy::doc_markdown` (7): docs churn outside this change's docs
  mandate (paths only, no content rewrite).

Gate held: `cargo clippy --workspace --all-targets -- -D warnings` green
(stricter than the pinned `--workspace` form), zero new `#[allow]`
attributes, `cargo fmt --check` clean, full `cargo test` green.

### D3 — Docs: AGENTS.md architecture section rewritten around the workspace

Replace the current per-directory `src/…` map with: the composition root
(`ponos-cli`, binaries + `ponos` facade), the adapter crates with one
line each (acp, render, check, luau, config, result), `ponos-core` with
the four funded ports and the **closed-set rule** (new ports require
their own change), and the TUI-readiness note (structured
`SessionEvent`s + `EventSink`/`InteractionPolicy` ports exist so a TUI
is an adapter away — no current plan). Stale paths elsewhere in AGENTS.md
(the Testing section's `src/bin/mock-agent/`) are refreshed in the same
pass. README and `skills/ponos/SKILL.md`: fix stale paths
(`src/bin/mock-agent/` and any other root-relative `src/` references that
point into the repo tree) to `crates/…`; no content rewrite. Illustrative
output stays byte-identical — the README "Output format" sample block is
reproduced line-for-line by `crates/ponos-cli/tests/cli.rs`, and
SKILL.md's code-sample payload strings are fictional, not repo pointers.

### D4 — Straggler hunt with the facade carve-outs

`git grep` for dead paths from ①/② moves (`src/config.rs`, `src/task.rs`,
`crate::result_wire`, pre-split module paths) in code and docs; delete
dead modules. Two deliberate carve-outs:

1. The `ponos` facade's flat `pub use` list and the `config`/`task`
   compat re-exports are **kept** — tests exercise the system through
   them (settled Q4). Correspondingly, `crate::render`/`crate::check`/…
   inside `crates/ponos-cli` are live composition-root usage of that
   facade, not stragglers; the straggler grep excludes the crate.
2. `examples/` payload strings (`"src/main.rs"` &co. in
   `sequential_review.luau` and `workflow-1/main.luau`) stay
   byte-identical: fictional review targets in the test-pinned corpus —
   ② kept `examples/` byte-identical and no test asserts on the strings.
   ②'s task 3.3 deferred them to ③'s scope; this change resolves that
   handoff as won't-fix, recorded here rather than silently dropped.

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
