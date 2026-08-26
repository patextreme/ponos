## 1. Precondition

- [ ] 1.1 Verify `add-shell-exec` is archived and `openspec/changes/` holds no other active change; verify `openspec/specs/shell-exec/` now exists (its requirements sync before this change's deltas apply). If not archived, stop — sequencing is ratified in the proposal.

## 2. Directory renames

- [ ] 2.1 `git mv` the eight crates: `crates/ponos-{cli,core,acp,check,config,luau,render,result}` → `crates/ptah-*`, plus `skills/ponos` → `skills/ptah` and `.ponos` → `.ptah` (with `ponos.d.luau` → `ptah.d.luau` inside it); verify `git status` shows pure renames (R100 or near)
- [ ] 2.2 Rename the workspace references that make the tree build again at the package level: workspace `Cargo.toml` member paths/`publish` metadata if any, each crate's `package.name` (`ponos-cli` → `ptah-cli`, etc.), inter-crate dependencies, and the composition root's `[lib] name = "ptah"` + `[[bin]] name = "ptah"` per design D2; verify `cargo metadata --no-deps` resolves and `cargo build` succeeds

## 3. Mechanical sweep

- [ ] 3.1 Run the case-sensitive scripted sweep (`PONOS→PTAH`, `Ponos→Ptah`, `ponos→ptah`) over all tracked text files (exclude `.git`, `target`, `Cargo.lock`, `result*` symlinks); verify `git diff` shows only name substitutions
- [ ] 3.2 Sweep the Luau-facing sources explicitly and confirm the global registers as `ptah`: `bindings.rs` (`globals.set("ptah", …)`, error prefixes like `"ptah.json.stringify: …"`), `lint.rs` AST name check (`name == "ptah"`), sandbox references; verify `cargo test -p ptah-luau -p ptah-check` passes
- [ ] 3.3 Sweep every test file's literals: `CARGO_BIN_EXE_ponos_` → `CARGO_BIN_EXE_ptah_` (3 files), `mcp__ponos__result_submit` → `mcp__ptah__result_submit` constructions, mock-agent client/stdio name `"ponos"` → `"ptah"` (`mock-agent/main.rs`), `PONOS_TEST`/`PONOS_TEST_MODEL`/`PONOS_EXEC_TEST_TOKEN`/`PONOS_REQUIRE_REAL_LSP` → `PTAH_*`; verify `cargo test` (full offline suite) passes

## 4. Non-string surface fixes (design D3)

- [ ] 4.1 Fix `crates/ptah-check/src/defs.rs` `include_str!` path to `../../../.ptah/ptah.d.luau`; verify a `ptah check` invocation against a bundled example in a test passes (definitions found and parsed)
- [ ] 4.2 Update Nix: `pname = "ptah"` (package.nix), checks attrs `ptah-tests`/`ptah-smoke`/`ptah-analyze`, `packages.ptah`, `meta.mainProgram`, `source.nix` `ponosSrc` → `ptahSrc` and the `/.ponos` → `/.ptah` filter suffix rules, flake description; verify `nix flake check` passes in full
- [ ] 4.3 Update `deps_guard.rs` crate-name pins to `ptah_acp`/`ptah_luau`/… ; verify `cargo test -p ptah-core --test deps_guard` passes
- [ ] 4.4 Rename render identifiers `ponos_line`/`ponos-line` → `ptah_line` and the rendered `[ponos]` prefix → `[ptah]`; verify an e2e test asserting the `[ptah]` diagnostic line passes
- [ ] 4.5 Update `.helix/languages.toml` definitions path to `.ptah/ptah.d.luau`; verify the referenced path exists

## 5. Docs and meta

- [ ] 5.1 Rewrite the README name-origin block for Ptah (Egyptian god of craftsmen, creator by word), keep exactly one "formerly ponos" sentence as the sole allowed tree-wide survivor; update every command/URL to `github.com/patextreme/ptah`; verify `grep -ri ponos` over the tree returns only that sentence
- [ ] 5.2 Update AGENTS.md crate map and exit-code prose to the new crate/binary names; verify names match `ls crates/`
- [ ] 5.3 Sweep `openspec/` prose: `## Purpose` sections of all synced specs (direct edits per design D5) and the 16 archived change directories; verify `grep -ri ponos openspec/` returns only the archived-history exceptions if any were deliberately kept (default: none)
- [ ] 5.4 Update `skills/ptah/SKILL.md`: API references, `.ptah/config.toml`, `ptah.d.luau`, exit codes, GitHub URL; verify `cargo test --test examples` (examples run through the renamed skill-referenced API) passes

## 6. Final gates

- [ ] 6.1 Full gate run: `cargo test` (offline, incl. e2e/acp/examples), `nix flake check`, and the grep gate (`rg -i ponos` outside `.git`/`target`/`Cargo.lock`/`result*` → README "formerly ponos" only); all green in one commit — no half-renamed state is ever pushed

## 7. Closeout

- [ ] 7.1 Archive this change (`openspec archive rename-ponos-to-ptah`); verify the ten delta specs sync (spot-check `openspec/specs/scripting/spec.md` now says `ptah` namespace and `openspec/specs/shell-exec/` renamed members)
- [ ] 7.2 Manual same-day follow-ups (not automated, verify done by hand): rename the GitHub repo and local working directory, re-point the deployed `~/.pi/agent/skills/ponos` symlink at the new store path (`nix build` + re-link), move `~/.config/ponos/config.toml` → `~/.config/ptah/config.toml`, delete the stale root `result*` symlinks
