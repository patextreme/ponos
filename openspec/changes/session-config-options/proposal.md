## Why

ACP agents increasingly expose per-session configuration — most importantly model selection — via the protocol's session config options (`session/new` response `configOptions` + `session/set_config_option` + `config_option_update`), and the flagship adapter (`@agentclientprotocol/claude-agent-acp`) exposes model, mode, effort, and more that way. ponos currently declares no client capabilities and drops the option state on the floor, so scripts can only pick models at the process level (env vars in the registry) — which categorically cannot do the fan-out case: one script, one agent, different models per session (opus reviews, haiku summarizes).

## What Changes

- Advertise the `session.configOptions` client capability in `initialize` (the only client capability ponos declares; interactive capabilities remain unadvertised and requests still get deny-all -32601).
- Capture `configOptions` from the `session/new` response and keep the live option state per session; fold `config_option_update` notifications and `set_config_option` responses into it.
- New session API: `session:configOptions()` returns the live option list (empty table when the agent offers none); `session:setConfig(id, value)` sets a string (select value id) or boolean option, hard-erroring on agent rejection or unsupported method.
- `setConfig` is serialized with prompt turns via the existing turn lock: config changes apply strictly between turns.
- Renderer lifecycle lines on `setConfig` success and on agent-pushed config changes (session-attributed).
- Mock agent gains env-scripted knobs: `MOCK_CONFIG_OPTIONS` (advertise options in `session/new`), `MOCK_CONFIG_REJECT` (fail `session/set_config_option`), `MOCK_CONFIG_UPDATE` (push a mid-session `config_option_update`).
- New bundled example `examples/model-fanout.luau` with its explicit entry in `tests/examples.rs`.
- Type definitions extended with `configOptions`/`setConfig` and the option-table types; probe exercises them.

## Capabilities

### New Capabilities

- `session-config-options`: per-session configuration surface exposed to scripts — option discovery, mutation, live state, and the client capability that unlocks it.

### Modified Capabilities

- `agent-sessions`: the "No client capabilities are exposed" requirement becomes "no *interactive* capabilities": ponos SHALL advertise `session.configOptions` in `initialize`; agent-to-client *requests* remain denied with -32601 (deny-all dispatch unchanged). `config_option_update` notifications are consumed, not replied to.
- `type-definitions`: definitions cover the new methods and option-table types; probe exercises them. (The `scripting` capability needs no delta: the new session methods are specified wholly by the new capability.)

## References

- Ordering: `add-typed-agent-results` (in progress) archives first, then `rename-script-api-camelcase`, then this change. This change's deltas are written against the post-`add-typed-agent-results` main-spec text (its `result` additions and permission auto-allow are carried forward) and use the camelCase names.
- ACP schema 1.4 (pinned via `agent-client-protocol` 1.3) types all needed messages; no dependency change.
- Out of scope by design: model aliasing sugar (`setModel`), create-time config in `session()` options, native `session/set_mode` (the flagship adapter exposes mode as config id `mode`, covered by `setConfig`), timeouts on `setConfig`.
