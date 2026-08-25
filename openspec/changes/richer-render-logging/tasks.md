## 1. Renderer foundations

- [x] 1.1 Change `hhmmss()` to a full `yyyy-mm-dd HH:MM:SS` local timestamp (via jiff) and update renderer unit references; verify every rendered line starts with the date-shaped timestamp by running an existing e2e test with output visible (or a new renderer unit test asserting the prefix shape)
- [x] 1.2 Add a shared visible-char truncation helper (budget 120, `…` suffix, Unicode-safe via `char_indices`) as a free function with unit tests covering: short text untouched, multi-byte chars cut safely, boundary lengths

## 2. Prompt line

- [x] 2.1 In `run_turn`, use the currently-unused renderer/label params to render `prompt: <collapsed text>` at send time (whitespace runs collapsed to single spaces, truncation applied); verify a new e2e test sees exactly one `prompt:` line per turn with session attribution, suppressed under `--quiet`

## 3. Peek synthesis in the fold

- [x] 3.1 Extend `ToolCallDisplay` with a `peek: Option<String>` and implement kind-aware selection (execute → command/cmd from raw_input; read/edit/move/search/fetch/delete → locations[0] as `path[:line]`; fallback → compact `serde_json::to_string` of raw_input), first non-empty candidate sticky; verify unit tests on the fold for each kind and for the no-data case
- [x] 3.2 Implement the title-containment check (`title.contains(peek)` suppresses appending) and apply the peek to both start and terminal lines in `transition()`; verify unit tests: pi-acp-style bash (title = command) renders no duplicate, bare `read` title gains the path
- [x] 3.3 Implement path shortening `(path, cwd, home)` as a pure helper (cwd-relative → `~`-collapsed → as-is) with unit tests for the three cases; thread the session cwd from `session/new` into `ToolFold` construction
- [x] 3.4 Fold `kind`/`locations`/`raw_input` from both `tool_call` announcements and `tool_call_update` fields into the peek state; verify a unit test where raw_input arrives only on an update mid-flow

## 4. Mock agent

- [x] 4.1 Add `MOCK_TOOL_KIND`, `MOCK_TOOL_LOCATIONS` (comma-separated `path[:line]`), `MOCK_TOOL_RAW_INPUT` (JSON) knobs wired into the `MOCK_TOOL`/`MOCK_TOOL_FLOW` emissions; verify by driving one scripted turn with each knob set and observing the rendered peek in an e2e test

## 5. End-to-end and regression tests

- [x] 5.1 Add e2e coverage: prompt line (plain, truncated, quiet-suppressed); execute/read/other peeks; title dedup; path shortening under and outside cwd; date-prefixed lines; verify `cargo test --test acp` and `cargo test --test e2e` pass
- [x] 5.2 Update existing assertions in `tests/cli.rs` and `tests/typed_results.rs` that hard-code `tool:` lines or timestamp shapes; verify full `cargo test` is green

## 6. Docs and specs

- [x] 6.1 Rewrite README's Output format section (new example block with dated timestamps, prompt lines, peeks; document the truncation budget and quiet behavior); verify doc example lines match the e2e test output exactly
- [x] 6.2 Add the `cli` capability delta (`specs/cli/spec.md`): MODIFIED "Rendered lines are timestamped" → `yyyy-mm-dd HH:MM:SS` shape delegated to the `render-logging` capability; verify the MODIFIED header matches the current `openspec/specs/cli/spec.md` requirement verbatim (so archive resolves it) and the full requirement block including all three scenarios is restated
- [x] 6.3 Run `openspec validate richer-render-logging --strict` and fix any findings; verify delta specs parse and scenarios are complete
