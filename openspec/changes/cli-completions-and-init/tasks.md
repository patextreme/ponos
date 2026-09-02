# Tasks

## 1. Groundwork

- [ ] 1.1 Add `clap_complete` to the workspace deps (`Cargo.toml`, pinned to the 4.x line) and to `crates/ptah-cli/Cargo.toml`; run `cargo build` in the dev shell to refresh `Cargo.lock`. Verify: build succeeds and `cargo tree -p ptah-cli` shows `clap_complete`.

## 2. Completions subcommand

- [ ] 2.1 Add the `Completions` variant to the clap tree in `crates/ptah-cli/src/cli.rs`: required positional `<shell>` as a `ValueEnum` over `bash`, `zsh`, `fish`, `elvish`, `powershell`; extend `Parsed` with `Completions(Shell)` dispatched in the early (pre-runtime) arm, generating from the live `Cli` struct to stdout and nothing else. Verify: unit tests — each shell parses; unknown shell and missing arg are usage errors (`clap::error::ErrorKind` not DisplayHelp/DisplayVersion).
- [ ] 2.2 Add `crates/ptah-cli/tests/completions.rs` e2e tests against `env!("CARGO_BIN_EXE_ptah")`: per-shell exit 0 with plausibility markers (bash `complete`, zsh `#compdef`, fish `complete`/`__fish`, powershell `Register-ArgumentCompleter`); visible subcommands `run`/`check`/`types`/`completions`/`init` present and `__bridge` absent in every emitted script; unknown shell exits 2 with usage on stderr. Verify: `cargo test --test completions`.
- [ ] 2.3 Add the README "Shell completions" section: `ptah completions <shell>` semantics plus per-shell install one-liners covering at least bash, zsh, and fish. Verify: README section reads standalone; grep finds all three install commands.

## 3. Init subcommand

- [ ] 3.1 Implement `ptah init` in `crates/ptah-cli/src/cli.rs` (+ a small init module if the copy outgrows it): `const` config.toml skeleton and next-step hints per the approved copy; `Parsed::Init` early dispatch; `create_dir_all("./.ptah")`; per-file exists-check → skip with a message or write (defs bytes = version header + `TYPE_DEFINITIONS`); print created/skipped lines then hints to stdout on every run; filesystem errors to stderr with exit 1. Verify: unit tests — skeleton parses via `ptah_config::from_parts` into an empty registry with no agents; parse tests for the new variant.
- [ ] 3.2 Add `crates/ptah-cli/tests/init.rs` e2e tests against the real binary in `tempfile` dirs: fresh init creates both files, exit 0, created lines on stdout; written `ptah.d.luau` byte-identical to captured `ptah types` stdout; re-run reports both files skipped, files byte-identical, exit 0, hints still printed; pre-existing `config.toml` with user content survives untouched while the missing defs file is created; unwritable target simulated by a *file* named `.ptah` → stderr error, exit 1. Verify: `cargo test --test init`.
- [ ] 3.3 Update docs: README editor-setup section leads with `ptah init`, editor snippets point at `.ptah/ptah.d.luau`, `ptah types > .ptah/ptah.d.luau` documented as the refresh primitive; `skills/ptah/SKILL.md` workflow step 6 becomes `ptah init` with the manual redirect demoted to the refresh one-liner. Verify: both docs mention `ptah init` and the `.ptah/ptah.d.luau` path consistently; no remaining instructions to write root-level `ptah.d.luau`.

## 4. Wrap-up

- [ ] 4.1 Full verification: `cargo test` (unit + all e2e) and `openspec validate cli-completions-and-init --strict` pass; `nix flake check` passes in the sandbox (crane picks up the refreshed `Cargo.lock`). Verify: all three commands exit 0.
