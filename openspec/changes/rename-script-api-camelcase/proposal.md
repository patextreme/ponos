## Why

The script API mixes conventions: option-table and result-table fields are snake_case (`stop_reason`, `cache_read`, `cache_write`, `timeout_ms`, `mcp_servers`) while all method names are single words. The Luau ecosystem (the official type definitions at luau.org/types, the Roblox API) uses camelCase for fields; adopting it now — pre-release, before more surface accretes (e.g. the planned session config options) — makes the API one consistent convention instead of a permanent two-style split.

## What Changes

- **BREAKING** Rename the prompt result fields `stop_reason`, `cache_read`, `cache_write` to `stopReason`, `cacheRead`, `cacheWrite` (`text`, `usage`, `input`, `output` are single words; unchanged).
- **BREAKING** Rename the prompt option `timeout_ms` to `timeoutMs`.
- **BREAKING** Rename the session option `mcp_servers` to `mcpServers`.
- Update `types/ponos.d.luau` to the renamed fields.
- Update the runtime probe (`tests/fixtures/types_probe.luau`), integration tests, the bundled example (`examples/watchdog.luau`), and mock-agent test scripting to the new names.
- The rename applies to the post-`add-typed-agent-results` spec surface (that change is in progress and archives first): the deltas here also rename field mentions in the `typed-results` capability it introduces and carry its `result` option/field additions forward unchanged. This change's deltas SHALL NOT revert any content `add-typed-agent-results` syncs into the main specs.
- No behavior change of any kind: same values, same errors, same wire protocol. The renames are visible only inside Luau scripts.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `scripting`: prompt result table and option-table field names change (`stop_reason` → `stopReason`, `cache_read`/`cache_write` → `cacheRead`/`cacheWrite`, `timeout_ms` → `timeoutMs`, `mcp_servers` → `mcpServers`), on top of the `result` additions from `add-typed-agent-results`.
- `agent-sessions`: the "Prompt turns drive the full update stream" requirement's field mention changes.
- `type-definitions`: the definitions file declares the renamed fields; the probe exercises them.
- `typed-results` (introduced by `add-typed-agent-results`, in progress): field mentions in three requirements' scenarios change to camelCase.

## References

Per-session model selection via ACP session config options is planned next (`session-config-options`); that change is written against the camelCase names, which is why this rename lands first.
