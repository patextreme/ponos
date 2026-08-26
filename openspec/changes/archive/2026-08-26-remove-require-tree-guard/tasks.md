## 1. Runtime navigator

- [x] 1.1 In `src/script/require.rs`, delete `guard`, `escapes`, and their call sites in `reset`/`to_parent`/`to_child`; keep `normalize`, `jump_to_alias` rejection, and the `to_child` module-not-found error. Verify `cargo test --lib require` passes with unit tests replaced: drop `escaping_paths_rejected`/`pure_helpers_normalize_and_escape` escape assertions, add a test navigating `../sibling` modules successfully plus the retained non-relative rejection
- [x] 1.2 Add an e2e/integration test (mock agent, temp dirs) where an entry script requires `../shared/helper` outside its directory and the run succeeds. Verify with `cargo test --test e2e` (or the suite housing require runtime tests)

## 2. Static analysis

- [x] 2.1 In `src/check/lint.rs`, drop the escape branch from `resolve_edge` and the `escapes` helper (keep `normalize`/`resolve_file`). Update lint tests: remove the escapes-the-script-tree finding test (`tmp_project("escapes")` case and the "escapes the script directory" assertion), add a cross-tree require producing no finding. Verify `cargo test --lib check` and the check-suite filter pass
- [x] 2.2 Confirm pre-flight shares the lint path (no separate escape logic remains anywhere: `grep -rn "escapes\|script.tree\|script directory" src/` returns only comments/tests that were updated). Verify `cargo test` full suite passes

## 3. Example

- [x] 3.1 Add `examples/shared/helper.luau` and two entries (`examples/workflow-1/main.luau`, `examples/workflow-2/main.luau`) each requiring `../shared/helper` and driving the mock agent; register a test function in `tests/examples.rs`. Verify `cargo test --test examples` passes offline

## 4. Documentation

- [x] 4.1 README: update the require paragraph (~line 270) to state unbounded relative resolution with non-relative rejection; drop the require-tree residual from the editor-setup residuals list (~line 446); add the agent-scoped trusted-code sentence (scripts are trusted; the sandbox limits blast radius of bugs, not malice). Verify by re-reading the two sections for consistency with the `scripting` and `type-definitions` delta specs
- [x] 4.2 Update the canonical skill doc `skills/ponos/SKILL.md` require section (~line 64): unbounded relative resolution, non-relative rejection, and the same trusted-code sentence; remove the require-tree wording at ~line 312 context if present. Verify wording matches README

## 5. Final verification

- [x] 5.1 Run `cargo build && cargo test` and `nix flake check`; confirm zero findings on `ponos check` over the new example entry (`nix develop -c bash -c 'cargo run -- check examples/workflow-1/main.luau'` exits 0)
