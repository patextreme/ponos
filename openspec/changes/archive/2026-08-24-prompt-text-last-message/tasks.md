# Tasks: Prompt Text Is the Turn's Last Message

## 1. Mock fixture

- [x] 1.1 Add `MOCK_LEAD_CHUNKS` to `src/bin/mock-agent/main.rs`: stream `|`-separated chunks at turn start (before tool-call emissions, after client-request probes), honoring `MOCK_DELAY_MS` and the cancel check like other emissions; empty `MOCK_CHUNKS` must still emit one empty final chunk. Verify with a `MOCK_LEAD_CHUNKS=… MOCK_TOOL=1 MOCK_CHUNKS=…` e2e-style test below.

## 2. Turn fold

- [x] 2.1 Rework `TurnFold` in `src/acp/mod.rs` per design D1–D3: `text` (current run) + `prev_text` (last completed non-empty run); `break_message()` on `ToolCall`/`ToolCallUpdate` in `fold_update`; `settle_turn(discard)` returns `(text, submission)`, drains both fields, discards text when `discard`; `begin_turn` clears both. Update `run_turn` call sites (success + error path) and the existing `TurnFold` unit tests for the new signature. Verify `cargo test --lib` (unit tests) passes.
- [x] 2.2 Add unit tests in `src/acp/mod.rs` covering: chunks → tool → chunks yields the last run; chunks → tool with empty final run falls back to the earlier run; `settle_turn(true)` empties text; and a settled fold never leaks text into a second `begin_turn`/`settle_turn` cycle. Verify with `cargo test --lib turn_fold` (or the containing test module).

## 3. Integration tests

- [x] 3.1 In `tests/acp.rs`, add turn-outcome tests driving the mock: (a) `MOCK_LEAD_CHUNKS` + `MOCK_TOOL` + `MOCK_CHUNKS` → `outcome.text` equals the final message only; (b) `MOCK_LEAD_CHUNKS` + `MOCK_TOOL` + empty `MOCK_CHUNKS` → `outcome.text` equals the lead message (fallback); (c) a cancelled turn (`MOCK_HANG` + cancel, or timeout) followed by a clean turn on the same session → the second turn's `text` has no prefix from the first. Verify with `cargo test --test acp`.

## 4. Docs and validation

- [x] 4.1 Update `README.md`'s prompt-result table row (and any nearby prose) to define `text` as the turn's last agent message with the fallback rule. Verify by grepping README for the old phrasing being gone.
- [x] 4.2 Run the full suite (`cargo test`) plus `openspec validate prompt-text-last-message --strict` and confirm both pass.
