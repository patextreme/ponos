## 1. Turn path and tool description

- [x] 1.1 In `src/acp/mod.rs`, delete `RESULT_SUBMIT_INSTRUCTION` (line 56) and the append in `run_turn` (the `let text = if result_contract { format!(...) } else { text }` block); prompts now use `text` as passed. Verify: `cargo build` succeeds with no unused-parameter warning.
- [x] 1.2 Delete the now-dead `result_contract` param from `run_turn` and its threading (local + argument in the driver's `Prompt` arm, ~lines 679/689/823). Verify: `cargo build` and `cargo clippy` are clean.
- [x] 1.3 In `src/bridge.rs` `tool_for`, extend the description to carry the
      submit timing per design ("Call this when your work on the task is
      complete, with the final result as the `value` argument…"). Verify:
      `cargo build` succeeds; the wording is pinned by the test added in 1.4.
- [x] 1.4 In `src/bridge.rs`, add a unit test for `tool_for()` asserting the
      description contains "when your work" and names the `value` argument,
      so the "Tool description carries the submit guidance" scenario has a
      regression guard against the guidance being edited away. Verify:
      `cargo test bridge` runs and passes the new assertion.

## 2. Tests and docs

- [x] 2.1 In `tests/typed_results.rs`, replace `prompt_on_result_session_carries_submit_instruction` with `prompt_on_result_session_passes_text_verbatim`: on a result session the echoed prompt equals `"hello"` exactly (no suffix); keep the plain-session assertion. Verify: `cargo test --test typed_results` passes.
- [x] 2.2 In `README.md`, update the typed-results bullet that says ponos "appends one fixed sentence to each prompt": prompts are sent verbatim and submit guidance lives in the `result_submit` tool description; note that authors can mention submission in the prompt for agents that don't submit on their own. Verify: grep for the old sentence in README/src/tests returns only history-free results (no live references).
- [x] 2.3 Check `src/bin/mock-agent/` and examples for any reliance on the appended sentence (echo-based tests). Verify: `cargo test` (full suite) passes offline.

## 3. Validation

- [x] 3.1 Run `openspec validate remove-prompt-injected-submit-instruction --strict` and `nix flake check` (or `cargo test` in the dev shell if the sandbox is unavailable); both must pass before sync/archive.
