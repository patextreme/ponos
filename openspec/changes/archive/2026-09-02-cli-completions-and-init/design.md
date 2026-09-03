# Design

## Context

The CLI is a clap 4.6 derive tree (`run`, `check`, `types`, hidden `__bridge`)
dispatched through a `Parsed` enum in `crates/ptah-cli/src/cli.rs`: subcommands
needing no runtime setup parse, dispatch early, and return an `ExitCode`
(`types` is the precedent both new commands follow). Type definitions live as a
single `include_str!` const in `ptah-check` (`defs::TYPE_DEFINITIONS`) with no
version header; `ptah types` prepends `-- ptah {VERSION} type definitions`.
Registry skeleton knowledge (layers, `${VAR}`, field optionality) lives in
prose today. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- Two new zero-setup subcommands that reuse the existing early-dispatch path
  and exit-code contract, with all new user-facing copy pinned in tests.

**Non-Goals:**
- Dynamic (Tab-time) completion, completion auto-install, `$SHELL` detection.
- Starter scripts, editor config generation, `.luaurc` writing, user-level
  (`~/.config/ptah`) init, a `--force` flag, a `ptah types --write` mode.
- Any change to `run`/`check`/`types` behavior, registry semantics, or the
  embedded definitions content.

## Decisions

**D1 — `clap_complete` at runtime, in `ptah-cli`.** Generation happens in the
composition root from the live `Cli` struct, so emitted scripts cannot drift
from the binary. Alternative: committed static scripts (drift, packaging
burden) or build-time codegen (same output, extra build step). `clap_complete`
is added at the workspace level pinned to the 4.x line matching clap. `Shell`
is surfaced as a clap `ValueEnum` on the `Completions` variant — clap rejects
unknown shells as usage errors (exit 2) for free, matching the existing
`Err(e)` branch in `main`.

**D2 — early dispatch, no runtime construction.** `Parsed::Completions(Shell)`
and `Parsed::Init` join `Types`/`Check` in the pre-runtime match arm: no tokio
runtime, no tracing subscriber, no registry discovery. Both commands must work
on a machine with no agents configured.

**D3 — hidden `__bridge` stays hidden.** Verified during implementation:
clap_complete 4.x does *not* omit `hide`-flagged subcommands (checked
against 4.6) — generating from the raw `Cli` tree would leak `__bridge`
into every emitted script. Generation therefore runs over a copy of the
live command tree with hidden subcommands stripped (`completion_command()`);
everything else (args, help text, new visible subcommands) still derives
from the live struct, so emitted scripts cannot drift from the binary.
The e2e test asserts both directions (`run`/`check`/`types`/`completions`/`init`
present, `__bridge` absent) so neither a clap_complete regression nor a
stripping regression can silently leak the internal command.

**D4 — init writes exactly two files, copy lives in `ptah-cli`.** The skeleton
and next-step hints are `const` strings in the CLI crate (`config.toml`
skeleton, hint block). The skeleton is *not* re-exported through the `ptah`
facade and does not belong in `ptah-config`: it is init's scaffold, not
registry parsing. `ptah-config::from_parts` already parses the all-comments
file into an empty registry — the e2e test asserts this rather than adding a
new parse path. The defs file is written from the same source as `ptah types`
prints (header + `TYPE_DEFINITIONS`), so byte-identity is structural, tested by
comparing `ptah init` output against captured `ptah types` stdout.

**D5 — skip-don't-clobber, per file.** Each of the two files is checked with
`exists()` and independently created-or-skipped; `create_dir_all` for
`.ptah/` happens first. There is no transactional rollback across the two
files — a partial scaffold is a valid, idempotently-completable state (pinned
by a scenario). Filesystem errors surface on stderr with exit 1 per the
exit-code contract.

**D6 — output stream discipline.** Both commands write their payload and
progress to stdout only (completions: the script, nothing else; init:
created/skipped lines then hints). Errors go to stderr. This keeps
`ptah completions bash > …` redirection clean and init re-runs informative.

## Risks / Trade-offs

- [clap_complete emits shell-script dialects we don't hand-review each release]
  → Mitigation: e2e tests assert per-shell plausibility (bash `complete`,
  zsh `#compdef`, fish `complete`/`__fish`, powershell `Register-ArgumentCompleter`)
  rather than golden files, so a format change that keeps the registration
  contract passes while a broken one fails.
- [Skeleton copy drifts from real registry syntax over time] → Mitigation: the
  e2e test parses the written skeleton through `ptah_config::from_parts` (empty
  registry, no error), so at minimum it stays valid TOML in the current format;
  field docs are additionally covered by the README.
- [Static completions can't suggest dynamic values (agent names, script paths)]
  → Accepted: agent names live inside scripts, not argv; positional script
  paths fall back to shell file completion. Revisit only if the CLI ever gains
  agent-name flags.
- [Two-file scaffold written non-atomically] → Accepted: skip-per-file plus
  idempotence makes partial state safe; users re-run init to complete it.

## Migration Plan

Additive only; no existing surface changes. Rollback = remove the two
subcommands. `Cargo.lock` gains `clap_complete` (dev-shell `cargo build`
refresh, crane picks it up in `nix flake check`).

## Open Questions

None — all decisions settled during the grilling session that produced this
change.
