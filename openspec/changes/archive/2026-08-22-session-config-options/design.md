# Design

## Context

`agent-client-protocol` 1.3 (pinned) already types everything needed: `SetSessionConfigOptionRequest/Response`, `SessionConfigOption` (`id`, `name`, `category`, `kind: Select|Boolean`, select `options`), `ConfigOptionUpdate` (client→agent... agent→client notification), and `ClientSessionCapabilities.session.configOptions`. ponos's ACP driver (`src/acp/mod.rs`) owns the connection: it sends a bare `InitializeRequest`, drops `config_options`/`modes` from the `session/new` response, and its deny-all dispatch auto-errors agent→client requests while unhandled notifications vanish. Session state crosses to Luau via `SessionHandle` + `new_session_obj` in `src/script/mod.rs`; prompt turns are serialized per session by `turn_lock`.

The flagship adapter (`@agentclientprotocol/claude-agent-acp`) exposes `model` (select, category `model`), `mode`, Fast mode (boolean), effort, and subagent personas as config options; it answers `session/set_config_option` and pushes `config_option_update`. Value ids are agent-defined (`claude-opus-4-5`), and the ACP spec marks `category` as UX-only — never load-bearing.

## Goals / Non-Goals

**Goals**

- Scripts can discover and change per-session config (model above all) on agents that support it.
- ponos stays adapter-agnostic: no ponos-side knowledge of what `model` means, no alias layer.
- Option state surfaced to scripts is live, not a creation-time snapshot.

**Non-Goals**

- `setModel` sugar or any value aliasing/mapping.
- Create-time config in `session()` options (set-after-create keeps construction failure semantics clean).
- Native `session/set_mode` / modes API (the flagship adapter exposes mode as config id `mode`).
- A `timeoutMs` option on `setConfig` (fast request/response; connection-closed already errors).
- ACP v2 anything.

## Decisions

- **Capability advertisement**: `InitializeRequest.client_capabilities.session.config_options = SessionConfigOptionsCapabilities::new().boolean(BooleanConfigOptionCapabilities::new())`. The `boolean` sub-capability is load-bearing: per the pinned schema, omitting the `boolean` field means the client does not advertise boolean-option support, so conforming agents would never offer boolean options nor accept boolean `set_config_option` values — yet this change's surface (boolean entries in `configOptions()`, boolean `setConfig` values; e.g. the flagship adapter's Fast mode) requires them. This is still the sole declared capability (the sub-field is part of `session.configOptions`, not a second capability); the deny-all request dispatch stays byte-for-byte the same (requests still get -32601 — the capability gates what agents *offer*, not what ponos answers).
- **Option state lives in the driver, published via a shared `Arc<Mutex<Vec<SessionConfigOption>>>`** (same pattern as `TurnFold`): initialized from the `session/new` response; overwritten wholesale by `config_option_update` payloads (each carries the full option set — the protocol has no partial patch) and by `set_config_option` responses. `configOptions()` snapshots and converts it to Luau tables. No Lua-side event/callback surface in v1.
- **`setConfig` rides the existing command channel**: a new `SessionCmd::SetConfig { id, value, resp }` handled like `Prompt` — takes `turn_lock` first (serialized with turns, so changes apply strictly between turns), sends `SetSessionConfigOptionRequest`, folds the response into the option state, replies over a oneshot. Value mapping: Luau `string` → select value id, Luau `boolean` → boolean value; anything else errors before any wire traffic.
- **Errors are hard Lua errors** carrying the agent's message (config id + agent error text). Portability is the script's explicit job: check `configOptions()` first. A silent wrong-model fan-out is the failure mode we refuse.
- **Notification consumption**: `ConfigOptionUpdate` is a `SessionUpdate` variant, not a standalone notification method — verified in the pinned crates (agent-client-protocol 1.3 / agent-client-protocol-schema 1.4): it rides the existing `session/update` (`SessionNotification`) stream as the `config_option_update` discriminator, alongside `agent_message_chunk` and `usage_update`. It is handled by a new `SessionUpdate::ConfigOptionUpdate` match arm in the existing `on_receive_notification`(`SessionNotification`) handler in `src/acp/mod.rs`; no separate registration exists or is needed. It gets no reply path (it's a notification), so the deny-all request dispatch is untouched. Renderer emits one lifecycle line per agent-pushed change set naming each changed option id and its new value; a push arriving with no prior option state renders every advertised option as changed.
- **Requirement rename mechanism**: the `agent-sessions` delta uses a RENAMED entry (title-only, FROM/TO) plus a MODIFIED block under the new title. Verified empirically against openspec 1.10.0 (sandbox archive of the identical pattern): archive applies RENAMED and MODIFIED together — the final spec carries the new title and the modified content, and archive refuses any MODIFIED block that drops a scenario the current requirement still has. This change's MODIFIED block carries all five original scenarios plus the new one, so the pattern is safe as written; no fallback needed.
- **Mock agent**: four env knobs, consistent with existing `MOCK_*` scripting —
  - `MOCK_CONFIG_OPTIONS` (JSON array of options) → echoed in the `session/new` response `configOptions`;
  - `MOCK_CONFIG_REJECT` (`id` or `id=notfound`) → `session/set_config_option` for that id returns a JSON-RPC error — a generic error for plain `id`, method-not-found (-32601) for `id=notfound`; other ids succeed and mutate the mock's in-memory option state;
  - `MOCK_CONFIG_UPDATE` (JSON array of options, optional delay) → after the first prompt, push a `config_option_update` (as a `session/update` payload) carrying the new full option set;
  - `MOCK_CONFIG_ECHO` (`id`) → each turn's first message chunk echoes that option's current in-memory value, so tests observe end-to-end that prompts run under a changed config.
- **Types**: `ConfigOption` Luau conversion mirrors the schema (select vs boolean discriminant → `type` string + `currentValue` union). `setConfig` value type in the definitions is `string | boolean`.

## Risks / Trade-offs

- **Delta stacking with `rename-script-api-camelcase`**: that change has archived and also modified `agent-sessions` and `type-definitions` (different requirements than this change's); this change's deltas for the shared capabilities are rebased onto its post-archive text (the prior `add-typed-agent-results` content carried forward verbatim, plus the capability/config-option additions) so nothing is reverted. `add-typed-agent-results` has also archived; its text (permission auto-allow, `result` fields) is synced into the main specs. If the main specs drift further before this change applies, rebase these deltas at apply time (task 6.1 covers the check).
- **setConfig latency under mid-turn calls**: a script issuing `setConfig` during a long turn blocks on the turn lock — same discipline as prompt, and exactly the "between turns" semantics we chose. Documented in the requirement.
- **Adapter variance**: option ids/values are agent-defined; scripts hardcoding `"model"`/`"claude-…"` are coupled to a specific agent — that's inherent to ACP's design and the docs (example) should say so explicitly.

## Migration Plan

None — purely additive API surface; existing scripts unaffected (the capability bit changes the handshake, but only widens what agents may send).

## Open Questions

None.
