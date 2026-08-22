## 1. Timestamps

- [x] 1.1 Add `jiff` (`default-features = false, features = ["tz-system"]`) to `Cargo.toml` and a `hhmmss() -> String` helper in `src/render/mod.rs`; verify `cargo build` succeeds inside `nix develop`
- [x] 1.2 Prepend the timestamp in `Renderer::prefixed_line` (dimmed `\x1b[2m` when color is on, plain under `--no-color`), ahead of the `[label]` prefix; verify manually that `line`, `chunk`, `event`, `agent_stderr`, `lifecycle`, and `script_log` output all carry `HH:MM:SS` and that `--quiet` still suppresses everything
- [x] 1.3 Add a test-harness helper (shared across `tests/`) that strips a leading `\d{2}:\d{2}:\d{2} ` from rendered lines, and update existing output assertions to use it; verify the full suite passes (`cargo test`)

## 2. Tool-line policy

- [x] 2.1 In `src/acp/mod.rs`, extend the per-session driver state with a tool call map (id → title, first-activity `Instant`, last-rendered status); verify with a unit test around the new fold helper (pending seeds the map only; repeated statuses are suppressed; `in_progress` renders start; terminal renders status + duration)
- [x] 2.2 Change `DisplayEvent::Tool` to carry the fully formatted line body; `fold_update` resolves titles through the map (raw-id fallback for unannounced updates) and applies the transition policy; verify existing ACP tests pass with updated assertions
- [x] 2.3 Format durations (`X.Ys`, `Mm SS.Ss` above a minute) in the terminal line; verify via the unit tests from 2.1

## 3. Mock agent + integration coverage

- [x] 3.1 Add `MOCK_TOOL_FLOW` to `src/bin/mock-agent/` replaying a comma-separated status sequence (titled `tool_call` for the first entry, `tool_call_update`s after); verify a mock-driven test exercises the full pending → in_progress → in_progress → completed sequence
- [x] 3.2 Add e2e/integration tests covering the agent-sessions delta scenarios: start + terminal lines exactly, title resolution (no raw call id), repeated-status silence, pending silence, unannounced-update id fallback, direct completion without `in_progress`; verify each named scenario has a passing test
- [x] 3.3 Add integration test asserting timestamps: rendered lines match `^\d{2}:\d{2}:\d{2} \[`, `--no-color` keeps them plain, and script `print` output is unmodified; verify against the cli delta scenarios

## 4. Docs and validation

- [x] 4.1 Update `README.md` output-format section (timestamped lines, tool-line shapes, duration); verify no stale description of the old `tool: … (status)` format remains
- [x] 4.2 Run `cargo test` and `openspec validate improve-log-rendering --strict`; verify both pass clean
