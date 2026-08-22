## 1. Dependencies and permission posture

- [ ] 1.1 Add `rmcp` and `jsonschema` to Cargo.toml. Verify: `cargo build` succeeds inside `nix develop` and versions resolve into Cargo.lock

## 2. Permission allow-all (spec: agent-sessions MODIFIED)

- [ ] 2.1 Replace the deny-all dispatch for `session/request_permission` in src/acp/mod.rs with a handler selecting the first `AllowAlways` option; when no allow option exists, respond with method-not-found. Verify: extend the existing `MOCK_PERMISSION` test in tests/ to assert the agent observes an allow selection (and a new test for the reject-only-options path if the mock can't express it yet — extend mock-agent if needed)
- [ ] 2.2 Rewrite the README permission-contract paragraph ("declares no client capabilities… never get permission prompts") to state the headless allow-all posture, the `AllowAlways` persistence side effect, and that elicitation/fs/terminal remain unsupported. Verify: README grep finds no stale deny-all wording

## 3. UDS result channel and slot (spec: typed-results slot/outcome requirements)

- [ ] 3.1 Add a per-session UDS listener module (bind `ponos-r-<32hex>.sock` in `$XDG_RUNTIME_DIR`/`$TMPDIR`, stale-socket unlink-and-rebind, unlink at session close, newline-JSON `submit`/verdict protocol). Verify: unit test binding two concurrent listeners and closing them cleans up paths
- [ ] 3.2 Extend `TurnFold`/`TurnOutcome` with the submission slot: cleared at prompt start, set on `ok` verdicts (last-wins), carried into `TurnOutcome.result` on completion, discarded on cancel/timeout; late submits dropped with one renderer lifecycle line. Verify: existing cancel/timeout tests still pass; new tests (with a stub connection) assert discard-on-cancel and late-drop behavior
- [ ] 3.3 Wire `TurnOutcome.result` through `session:prompt()`'s Lua outcome table as `result` (JSON→Luau conversion; `nil` when absent). Verify: script-level test asserting `out.result == nil` on plain sessions and on no-submit turns

## 4. Schema declaration and validation (spec: result contract declaration)

- [ ] 4.1 Accept `result` in `agent:session(options)` (src/script/mod.rs): convert the Luau table via `LuaSerdeExt`, compile eagerly with `jsonschema`, raise a Lua error at the call site on invalid schemas and on remote `$ref`; store the compiled validator with the session's ACP options. Verify: script tests for valid-schema acceptance, invalid-schema error text, and remote-`$ref` rejection (all offline)

## 5. Bridge subcommand (spec: submit tool injection, in-turn validation)

- [ ] 5.1 Add the hidden `ponos __bridge` subcommand (clap hidden arg or subcommand): rmcp MCP server over stdio named `ponos` exposing `result_submit` with `inputSchema = { value: <PONOS_RESULT_SCHEMA> }`; on `tools/call`, relay the `value` argument over `PONOS_BRIDGE_ADDR` and block for the verdict; map `ok:false` to a tool error carrying the violation text. Verify: unit test driving the bridge binary with an rmcp client (spawn `env!("CARGO_BIN_EXE_ponos")`) against a stub UDS listener asserting ok and error round-trips
- [ ] 5.2 Inject the server into `session/new` `mcpServers` when `result` is set (`current_exe()`, `__bridge` arg, env pair), alongside the user's own servers. Verify: integration test where mock-agent echoes the received mcpServers config
- [ ] 5.3 Append the fixed instruction sentence to prompt text when the session has a contract (decide exact wording against mock transcripts). Verify: `MOCK_ENV_DUMP`-style echo test asserting the augmented prompt ends with the sentence

## 6. mock-agent MCP client and flags (spec: all typed-results scenarios)

- [ ] 6.1 Give mock-agent an rmcp MCP client that spawns servers from the session's `mcpServers` (stdio only), performs the handshake, and tears them down with the session. Verify: mock-agent test spawning the real bridge and listing `result_submit`
- [ ] 6.2 Add `MOCK_SUBMIT=<json>`: during each prompt, call `result_submit` with the parsed value and include the tool-call outcome in the turn. Verify: e2e test asserting the script's `out.result` equals the submitted value (happy path, incl. non-object root schema like an enum string)
- [ ] 6.3 Add `MOCK_SUBMIT_BAD=<n>`: submit n invalid values first, assert each returns a tool error naming violations, then submit a valid value. Verify: e2e test asserting `out.result` is the corrected value (in-turn retry proof)
- [ ] 6.4 Add `MOCK_NO_MCP`: skip MCP client startup entirely. Verify: e2e test asserting turn completes, `out.result == nil`, exactly one lifecycle log line about missing typed results

## 7. Degradation, concurrency, example, docs

- [ ] 7.1 Concurrency coverage: e2e test with two result sessions of different schemas running prompts concurrently. Verify: each `out.result` validates against its own schema only
- [ ] 7.2 Add `examples/typed_results.luau` (schema module + review-style flow incl. a nil-check retry loop) and its explicit test entry in tests/examples.rs. Verify: `cargo test --test examples typed_results` passes offline via mock-agent
- [ ] 7.3 README: document `result` in the `ponos` namespace table, the outcome `result` field, the Luau↔JSON mapping notes (null→nil, `{}`-as-object, integer/number), `mcp__ponos__result_submit` naming, and the degradation behavior. Verify: README examples run as written
- [ ] 7.4 Full suite + spec sync: `cargo test` green offline, `nix flake check` green, `openspec validate add-typed-agent-results --strict` passes
