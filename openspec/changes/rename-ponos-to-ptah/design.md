# Design: rename ponos → ptah

## Context

See `proposal.md` — Why. The rename is mechanical in content but wide: ~160
files, ~1,500 occurrences, spanning Rust identifiers, Luau API surface, wire
names, filesystem paths, Nix attrs, and docs. The pre-proposal inventory
(sub-agent report, summarized in the proposal's Impact section) is the
authoritative surface list; this document fixes *how* the sweep executes.

Two structural constraints shape everything:

1. **Wire-format names are internal contracts.** `PONOS_BRIDGE_ADDR`,
   `PONOS_RESULT_SCHEMA`, the `ponos-r-<hex>.sock` prefix, and the MCP server
   name `"ponos"` are produced and consumed only by this repo's binaries
   (bridge ↔ CLI ↔ mock-agent). Nothing outside the tree can observe them
   except an agent's transcript cosmetics (`mcp__ponos__result_submit`).
2. **The type definitions are compiled in.** `crates/ponos-check` embeds
   `.ponos/ponos.d.luau` via `include_str!("../../../.ponos/ponos.d.luau")`;
   the Nix source filter special-cases `/.ponos` paths; `.helix/languages.toml`
   points editors at the same file. Renaming the directory without flipping
   all three breaks the build, the sandbox check, and editor analysis
   respectively.

## Goals / Non-Goals

**Goals**

- One commit in which every surface speaks `ptah`; no intermediate state where
  any two components disagree about a name.
- Zero compatibility shims: no `ponos` global alias, no old-path fallback, no
  old env-var acceptance (decision ratified in proposal).
- The tree is grep-clean afterwards: `ponos` survives only where deliberately
  kept (see Decisions — README provenance note).

**Non-Goals**

- Publishing to crates.io (the `ptah-cli` package name merely keeps that door
  open; see Decisions).
- GitHub repo rename, local directory rename, re-pointing the deployed
  `~/.pi/agent/skills/ponos` symlink, moving the user's `~/.config/ponos/` —
  manual same-day follow-ups, listed in tasks as a checklist, not automation.
- Any behavioral change. If the sweep appears to require one, stop and
  reconsider: the deltas assert semantics are untouched.

## Decisions

### D1: Scripted case-sensitive sweep + `git mv`, not hand editing
A `python3`/`sed` sweep applying `PONOS→PTAH`, `Ponos→Ptah`, `ponos→ptah`
(in that order, case-sensitively — no Greek script exists in the tree outside
README, verified) over text files, preceded by `git mv` for the directory
renames (`crates/ponos-*` → `crates/ptah-*`, `skills/ponos` → `skills/ptah`,
`.ponos` → `.ptah`). History: `git mv` preserves rename tracking; a pure
content sweep over 160 files by hand is unreviewable.
*Alternative*: editor-driven manual rename — rejected: unverifiable at this
scale.

### D2: Package layout — `ptah-cli` package, `ptah` lib and bin
Crates map 1:1: `ptah-core`, `ptah-acp`, `ptah-luau`, `ptah-check`,
`ptah-render`, `ptah-config`, `ptah-result`, and the composition root becomes
package `ptah-cli` with `[lib] name = "ptah"` and `[[bin]] name = "ptah"`
(plus `mock-agent`, unchanged). Rationale: bare `ptah` is taken on crates.io
(dormant, 2022); `ptah-cli`/`ptah-core` are free; the facade import path
(`use ptah::…`, compat re-exports `ptah::config`/`ptah::task`) and the binary
name both stay clean. *Alternative*: lib named `ptah_cli` — rejected, breaks
the facade path for no benefit while unpublished.

### D3: Sweep-then-fix pass for non-string surfaces
The blind sweep cannot know these; each gets an explicit task:
- workspace/member `Cargo.toml` names, `Cargo.lock` (regenerate via build),
  `CARGO_BIN_EXE_ponos_` → `CARGO_BIN_EXE_ptah_` in test files
- `deps_guard.rs` crate-name pins (`ponos_acp` → `ptah_acp`, …) — identifiers,
  not strings
- `include_str!` relative path (same depth: `../../../.ptah/ptah.d.luau`)
- `nix/source.nix` `/.ponos` filter rules → `/.ptah`, `ponosSrc` → `ptahSrc`,
  flake `pname`/packages/checks attrs
- `ponos-luau/src/bindings.rs` global registration (`globals.set("ptah", …)`),
  error-message prefixes (`"ptah.json.stringify: …"`),
  `ponos-check/src/lint.rs:289` AST name check (`name == "ptah"`), sandbox
  references
- `ponos_line` / `ponos-line` Rust + render identifiers → `ptah_line`
- mock-agent literal client name and stdio name checks; test literals
  `mcp__ponos__result_submit` → `mcp__ptah__result_submit`
- README name-origin block: rewritten for Ptah, keeping one deliberate
  "formerly ponos" sentence for discoverability — the single allowed `ponos`
  survivor in the tree (the grep gate's allowlist entry).

### D4: Sequencing — quiet tree (ratified)
`add-shell-exec` archives first; its spec deltas sync into `openspec/specs/`
(including the new `shell-exec` capability), and this change's deltas —
already generated against that simulated post-state — then apply cleanly at
archive time. Task 0 verifies the precondition. Same pattern as the archived
`2026-08-22-rename-script-api-camelcase` change (which sequenced behind
`add-typed-agent-results` identically).

### D5: Archived OpenSpec changes are swept, main-spec Purpose prose is swept in-tree
Archive deltas sync requirement blocks only; `## Purpose` prose in
`openspec/specs/*/spec.md` and the 16 archived change directories are edited
directly in this change's sweep (git history preserves original wording).
No requirement names change (none start with `ponos`; verified), so no
RENAMED sections are needed anywhere.

## Risks / Trade-offs

- [Blind sweep mangles a word containing "ponos"] → Verified none exist in
  English/Greek-ASCII text in-tree; post-sweep `git diff` review gates the
  commit, and `cargo test` + `nix flake check` catch code damage.
- [`include_str!` path or Nix filter missed → build breaks late] → Both are
  explicit D3 tasks; `nix flake check` is a gate precisely because plain
  `cargo build` does not exercise the source filter.
- [Half-renamed commit pushed] → Single-commit rule; the grep gate
  (`rg -i ponos` → only the README allowlist hit) runs before commit.
- [Users' deployed skill symlink / configs dangle] → Manual follow-up
  checklist in tasks.md; AGENTS.md documents that deployed copies are nix-store
  symlinks, so a `nix build` + re-link fixes them.
- [crates.io `ptah` owner reactivates] → Unpublished today; if publishing
  ever happens, `ptah-cli` is the publishable name and the binary name is
  unaffected.

## Migration Plan

1. Verify precondition: `add-shell-exec` archived (task 0).
2. `git mv` directory renames; run sweep; apply D3 fixes.
3. Gates: `cargo test` (offline suite incl. e2e/acp/examples via mock-agent),
   `nix flake check`, grep-clean check.
4. Commit; archive this change (sync applies the ten deltas); manual
   follow-ups (repo, dir, symlink, user config) same day.
5. Rollback: `git revert` of the single commit restores everything except the
   GitHub repo name (renamed manually, reversible in settings).
