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
- Grep hygiene in two stages: after the commit, everything outside `openspec/`
  carries only the deliberate README provenance survivor; after archive (when
  main-spec bodies sync from these deltas), all of `openspec/specs/` is clean
  too. `openspec/changes/` — this change's own record and the archive — is a
  standing carve-out: rename history keeps its before→after wording verbatim
  for readability (see D5).

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
  survivor outside `openspec/changes/` (see D5 for the carve-out).

### D4: Sequencing — quiet tree (ratified)
`add-shell-exec` archives first; its spec deltas sync into `openspec/specs/`
(including the new `shell-exec` capability), and this change's deltas —
already generated against that simulated post-state — then apply cleanly at
archive time. Task 0 verifies the precondition. Same pattern as the archived
`2026-08-22-rename-script-api-camelcase` change (which sequenced behind
`add-typed-agent-results` identically).

### D5: `openspec/changes/` is carved out verbatim; main-spec Purpose prose is swept in-tree
Archive deltas sync requirement blocks only. The `## Purpose` prose in
`openspec/specs/*/spec.md` is edited directly (task 5.3), because delta sync
never touches Purpose prose; requirement *bodies* flip only at archive-time
sync (D6), so between commit and archive `openspec/specs/` legitimately still
contains `ponos` tokens — hence the two-stage grep gate (Risks). This
change's own record under `openspec/changes/rename-ponos-to-ptah/` and every
previously archived change keep their `ponos` tokens verbatim: transition
prose like "`PONOS_BRIDGE_ADDR` → `PTAH_*`" collapses into noise if swept, so
the directories document the mapping instead. `openspec/changes/` is a
standing exclusion in the grep gate, alongside `.git`/`target`/`Cargo.lock`/
`result*`. No requirement names change (none start with `ponos`; verified),
so no RENAMED sections are needed anywhere.

### D6: Scenario-heading rename rides via a one-line main-spec pre-sync
OpenSpec's MODIFIED guard — enforced identically by `openspec validate` and at
archive time in `specs-apply` — compares scenario headings between the current
spec and the delta block as literal strings; there is no syntax for renaming a
heading inside a MODIFIED requirement (RENAMED operates on requirement headers
only). Renaming `#### Scenario: ponos.exit kills running child` through the
delta alone therefore makes this change invalid to validate and impossible to
archive. Resolution: pre-apply exactly that one heading in
`openspec/specs/shell-exec/spec.md` ahead of archive. The delta still carries
the full renamed block, so `specs-apply` sees normalized-equal content for that
requirement and skips cleanly (its documented early-sync pattern). Every other
main-spec requirement block continues to flip only at archive, per D5. The body
of that scenario keeps speaking `ponos.exit` until the sweep; only its heading
token flips early. *Alternative rejected*: carrying the literal `ponos.exit`
heading until after archive — it would guarantee a stale `ponos` marker inside
the freshly synced spec and contradict the grep gate.

## Risks / Trade-offs

- [Blind sweep mangles a word containing "ponos"] → Verified none exist in
  English/Greek-ASCII text in-tree; post-sweep `git diff` review gates the
  commit, and `cargo test` + `nix flake check` catch code damage.
- [`include_str!` path or Nix filter missed → build breaks late] → Both are
  explicit D3 tasks; `nix flake check` is a gate precisely because plain
  `cargo build` does not exercise the source filter.
- [Half-renamed commit pushed] → Single-commit rule; the grep gate runs in two
  stages: pre-commit over everything outside `openspec/` (README-only
  survivors), then post-archive re-run including `openspec/specs/`, always
  excluding `openspec/changes/` per D5.
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
   `nix flake check`, and the pre-commit grep gate (outside `openspec/`;
   README-only survivors).
4. Commit; archive this change (sync applies the ten deltas); re-run the grep
   gate including `openspec/specs/` (`openspec/changes/` carve-out unchanged);
   manual follow-ups (repo, dir, symlink, user config) same day.
5. Rollback: `git revert` of the single commit restores everything except the
   GitHub repo name (renamed manually, reversible in settings).
