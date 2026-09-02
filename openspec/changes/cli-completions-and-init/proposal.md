## Why

Setting up a ptah workspace today is a manual multi-step ritual: `ptah types >
ptah.d.luau` for editor types, a hand-written `.ptah/config.toml` whose layering
and `${VAR}` rules live only in prose, and no shell completion for the CLI
itself. Both gaps push first-run friction onto every new user and every new
machine.

## What Changes

- New subcommand `ptah completions <shell>`: prints a completion script for
  `bash`, `zsh`, `fish`, `elvish`, or `powershell` to stdout, generated at
  runtime from the live clap command tree (via `clap_complete`), so emitted
  scripts always match the installed binary. No install heuristics, no
  `$SHELL` auto-detection; unknown shell is a clap usage error (exit 2).
- New subcommand `ptah init`: scaffolds `./.ptah/` in the current working
  directory with exactly two files — `ptah.d.luau` (byte-identical to
  `ptah types` stdout, version header included) and `config.toml` (fully
  commented registry skeleton that parses as a valid empty registry). Existing
  files are skipped with a message (idempotent, never destructive); hard
  failures (unwritable directory) exit 1. Init prints next-step hints (agent
  config, luau-lsp setup, completions, skill link) on every run.
- Docs: README editor-setup section leads with `ptah init` and points editor
  snippets at `.ptah/ptah.d.luau` (`ptah types` demoted to the refresh
  primitive); new README "Shell completions" section with per-shell install
  lines; `skills/ptah/SKILL.md` workflow step 6 becomes `ptah init`.

No breaking changes: `run`/`check`/`types` behavior, exit-code contract, and
the registry discovery/layering model are untouched.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `cli`: two ADDED requirements — the `completions` subcommand (stdout-only
  emission, shell set, exit codes) and the `init` subcommand (target
  directory, file set, overwrite-skipping semantics, hints, exit codes).
- `type-definitions`: the editor-setup documentation requirement is MODIFIED —
  `ptah init` becomes the documented front door for obtaining definitions,
  the documented definitions path becomes `.ptah/ptah.d.luau`, and
  `ptah types` redirection is documented as the refresh primitive.

## Impact

- `crates/ptah-cli`: new `completions` and `init` commands in
  `src/cli.rs` dispatch; skeleton/hint copy constants; init file-writing
  helper. New dependency `clap_complete` (workspace-level, pinned to the 4.x
  line alongside clap).
- Hidden `__bridge` subcommand must not appear in generated completions
  (verified during implementation).
- Docs: `README.md` (editor setup + new completions section), `skills/ptah/SKILL.md`.
- Tests: unit tests for CLI parse; e2e tests via the real binary in tempdirs
  (completions emit plausible per-shell scripts; init creates both files,
  skeleton parses as an empty registry, defs byte-identical to `ptah types`,
  idempotent re-run, existing-file skip). `Cargo.lock` refresh in the dev
  shell; `nix flake check` picks it up through crane.
