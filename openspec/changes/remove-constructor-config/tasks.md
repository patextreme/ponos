## 1. Runtime removal

- [ ] 1.1 In `src/script/mod.rs`, remove the constructor `config` parsing block (the `constructor_config` loop and its typing) and add the key-presence rejection: reading `opts.get::<Option<Table>>("config")` alongside the other option reads; `Some` raises `mlua::Error::runtime` with the design's error text (removed, ordering rationale, `setConfig` migration, driving-option-first hint). Verify with a run of `.work/repro-config.luau`-style script: the error fires and no subprocess is spawned.
- [ ] 1.2 Remove the post-spawn apply/teardown loop (the `for (id, value) in constructor_config` block with its session-teardown error path). Verify `cargo build` and that no `constructor_config` references remain (`rg constructor_config src/` is empty).

## 2. Mock agent

- [ ] 2.1 Extend `src/bin/mock-agent/` with a scripted dependent-option behavior (env var, e.g. `MOCK_CONFIG_DEPENDENT=1`): handling a `session/set_config_option` for `model` re-derives `effort` to its default in the returned option state, so tests can pin the sequencing contract. Verify with a new e2e test asserting: `setConfig("model", …)` then `setConfig("effort", "high")` ends with `effort == "high"`.

## 3. Types and docs

- [ ] 3.1 Delete the `config` field and its whole doc-comment block from `types/ponos.d.luau`. Verify `ponos types` output no longer contains `config` in `SessionOptions`.
- [ ] 3.2 README: replace the constructor-`config` paragraphs with the removal note (error message, `setConfig` migration) and add the dependent-option sequencing paragraph (opencode `model` → `effort` reset as the concrete instance, configure-before-first-prompt pattern, atomicity note: a rejected `setConfig` no longer tears the session down inside the constructor). Verify by reading the config section end-to-end.
- [ ] 3.3 Rewrite `examples/model-fanout.luau` to sequential `setConfig` calls (never a `pairs()` loop). Verify `ponos check examples/model-fanout.luau` passes and its `tests/examples.rs` entry runs green.
- [ ] 3.4 Update `tests/fixtures/types_probe.luau` to drop `config` from the session-options use. Verify the definitions probe test passes.
- [ ] 3.5 Update `skills/ponos/SKILL.md` (the in-repo copy consumers download): remove the `config` constructor option from the session-options table and the "pin at creation" example, remove the mixed-table luau-lsp workaround, teach sequential `setConfig` with the driving-option-first hint. Verify only `setConfig`/`configOptions` references remain (`rg -n 'config' skills/ponos/SKILL.md`).

## 4. Tests

- [ ] 4.1 Rewrite the constructor-config e2e tests in `tests/e2e.rs`: `config = {…}` raises pre-spawn (assert the error message names `config` and `setConfig`), empty `config = {}` raises identically, and the old "applies before first prompt" / "agent rejection tears down" / "bad value fails before spawn" scenarios are removed with the feature. Verify with `cargo test --test e2e`.
- [ ] 4.2 Add the sequencing test from task 2.1 to the e2e suite if not already there. Verify `cargo test --test e2e` is green.

## 5. Verification

- [ ] 5.1 Full local suite: `cargo test` inside `nix develop` (all tests, offline). Verify zero failures.
- [ ] 5.2 Sandbox parity: `nix build` succeeds (crane keeps `examples/` and `tests/fixtures/` in the build source).
- [ ] 5.3 End-to-end sanity on a real agent: the `.work/repro-config.luau` script now errors pre-spawn with the migration message; the `setConfig`-ordered equivalent (`model` first, then `effort`) holds `effort = high` on opencode. Record both outputs.
