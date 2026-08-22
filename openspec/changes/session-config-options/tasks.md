## 1. Mock agent

- [ ] 1.1 Implement `MOCK_CONFIG_OPTIONS` (advertise options in `session/new` response), `MOCK_CONFIG_REJECT` (fail `session/set_config_option` for the named id — a generic JSON-RPC error for plain `id`, method-not-found `-32601` for `id=notfound`; other ids mutate in-memory state), `MOCK_CONFIG_UPDATE` (push a `config_option_update` after the first prompt), and `MOCK_CONFIG_ECHO` (first turn chunk echoes the named option's current value). Verify: tests from tasks 2.2, 2.3, and 3.2 drive all four knobs.

## 2. ACP driver

- [ ] 2.1 Advertise the capability: set `clientCapabilities.session.configOptions` on the `InitializeRequest` in `src/acp/mod.rs`. Extend `SessionOptions`/`SessionHandle` plumbing as needed for per-session option state (`Arc<Mutex<Vec<SessionConfigOption>>>`, patterned on `TurnFold`). Verify: `cargo build`; handshake test asserts the capability bit reaches the wire (mock agent logs or echoes it).
- [ ] 2.2 Capture `config_options` from the `session/new` response into the session's option state. Verify: unit/integration test creating a session against mock with `MOCK_CONFIG_OPTIONS` set; state snapshot non-empty.
- [ ] 2.3 Handle `config_option_update`: add a `SessionUpdate::ConfigOptionUpdate` match arm to the existing `session/update` handler that replaces the option state wholesale and emits a lifecycle line naming each changed option id and its new value. Verify: `cargo test --test acp` with `MOCK_CONFIG_UPDATE`.

## 3. Session API

- [ ] 3.1 Add `SessionCmd::SetConfig` to the driver command loop: takes `turn_lock`, sends `SetSessionConfigOptionRequest`, folds the response into option state, replies over oneshot; maps wire errors to error strings carrying config id + agent message. Verify: `cargo build`.
- [ ] 3.2 Bind `configOptions()` and `setConfig(id, value)` on the session object in `src/script/mod.rs`; value typing (string→select id, boolean→boolean, else pre-send Lua error); result conversion for `configOptions()` (select vs boolean, `category` nil when absent). Emit a lifecycle line on successful set. Verify: `cargo test --test acp` covering read, set-success (asserting the effective value via `MOCK_CONFIG_ECHO` on a follow-up prompt), agent-reject (`MOCK_CONFIG_REJECT`), unsupported method (`MOCK_CONFIG_REJECT=id=notfound`), mid-turn serialization (`setConfig` during a `MOCK_DELAY_MS`-extended turn sends only after the turn completes), wrong-type, and empty-options cases.

## 4. Definitions and probe

- [ ] 4.1 Extend `types/ponos.d.luau` with `configOptions`/`setConfig` and the option-entry types. Verify: `cargo test --test typed_results` (defs sync).
- [ ] 4.2 Extend `tests/fixtures/types_probe.luau` to exercise both methods (against the mock with options advertised). Verify: probe test green.

## 5. Example and docs

- [ ] 5.1 Add `examples/model-fanout.luau`: two sessions from one agent, `setConfig("model", …)` before first prompt on each, prompt both, print results; include a comment noting option ids are agent-defined. Add its test function to `tests/examples.rs`. Verify: `cargo test --test examples`.
- [ ] 5.2 Update `README.md` (API table + a short section on per-session config, noting the env-var alternative and the agent-defined nature of ids). Verify: `cargo test` full suite green (offline).

## 6. Spec hygiene

- [ ] 6.1 Ordering guard at apply time: `add-typed-agent-results` and `rename-script-api-camelcase` have both archived (confirmed at review time); re-read the synced `openspec/specs/` (agent-sessions, type-definitions, scripting, typed-results) and rebase this change's deltas if their final text has drifted since — nothing from either may be reverted. Verify: `openspec validate session-config-options` passes.
