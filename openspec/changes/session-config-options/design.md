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

- **Capability advertisement**: `InitializeRequest.client_capabilities.session.config_options = SessionConfigOptionsCapabilities::new()`. This is the sole declared capability; the deny-all request dispatch stays byte-for-byte the same (requests still get -32601 — the capability gates what agents *offer*, not what ponos answers).
- **Option state lives in the driver, published via a shared `Arc<Mutex<Vec<SessionConfigOption>>>`** (same pattern as `TurnFold`): initialized from the `session/new` response; overwritten wholesale by `config_option_update` notifications (the notification carries the full option set — the spec has no partial patch) and by `set_config_option` responses. `configOptions()` snapshots and converts it to Luau tables. No Lua-side event/callback surface in v1.
- **`setConfig` rides the existing command channel**: a new `SessionCmd::SetConfig { id, value, resp }` handled like `Prompt` — takes `turn_lock` first (serialized with turns, so changes apply strictly between turns), sends `SetSessionConfigOptionRequest`, folds the response into the option state, replies over a oneshot. Value mapping: Luau `string` → select value id, Luau `boolean` → boolean value; anything else errors before any wire traffic.
- **Errors are hard Lua errors** carrying the agent's message (config id + agent error text). Portability is the script's explicit job: check `configOptions()` first. A silent wrong-model fan-out is the failure mode we refuse.
- **Notification consumption**: `ConfigOptionUpdate` is a distinct notification type from `SessionNotification`; it needs its own `on_receive_notification` handler alongside the existing one. It gets no reply path (it's a notification), so the deny-all request dispatch is untouched. Renderer emits one lifecycle line per agent-pushed change set naming changed ids.
- **Requirement rename mechanism**: the `agent-sessions` delta uses a RENAMED entry (title-only, FROM/TO) plus a MODIFIED block under the new title. Validate accepts both; if archive turns out to apply MODIFIED before RENAMED (ordering not documented), the fallback is to drop the RENAMED entry and keep the original title "No client capabilities are exposed" with the rebased content unchanged — the title is cosmetic, the content is normative.
- **Mock agent**: three env knobs, consistent with existing `MOCK_*` scripting —
  - `MOCK_CONFIG_OPTIONS` (JSON array of options) → echoed in the `session/new` response `configOptions`;
  - `MOCK_CONFIG_REJECT` (`id` or `id=value`) → `session/set_config_option` for that id returns a JSON-RPC error; other ids succeed and mutate the mock's in-memory option state;
  - `MOCK_CONFIG_UPDATE` (JSON array of options, optional delay) → after the first prompt (or on env-scripted trigger), push a `config_option_update` carrying the new full option set.
- **Types**: `ConfigOption` Luau conversion mirrors the schema (select vs boolean discriminant → `type` string + `currentValue` union). `setConfig` value type in the definitions is `string | boolean`.

## Risks / Trade-offs

- **Delta stacking with `add-typed-agent-results`**: that change is in progress and archives first; it also modifies `agent-sessions`' "No client capabilities are exposed" (permission auto-allow) and `type-definitions`' "Definitions cover the script API" (adds `result`). This change's deltas for those two requirements are already rebased onto its post-archive text (carried forward verbatim, plus the capability/config-option additions) so nothing it adds is reverted. Confirmed sequence: `add-typed-agent-results` → `rename-script-api-camelcase` → this change. If `add-typed-agent-results` text drifts before archiving, rebase these deltas at apply time (task 6.1 covers the check).
- **setConfig latency under mid-turn calls**: a script issuing `setConfig` during a long turn blocks on the turn lock — same discipline as prompt, and exactly the "between turns" semantics we chose. Documented in the requirement.
- **Adapter variance**: option ids/values are agent-defined; scripts hardcoding `"model"`/`"claude-…"` are coupled to a specific agent — that's inherent to ACP's design and the docs (example) should say so explicitly.

## Migration Plan

None — purely additive API surface; existing scripts unaffected (the capability bit changes the handshake, but only widens what agents may send).

## Open Questions

None.
