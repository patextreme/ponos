## 1. Runtime binding

- [ ] 1.1 Rename the Luau-visible fields in `src/script/mod.rs`: prompt result `stop_reason`→`stopReason`, `cache_read`→`cacheRead`, `cache_write`→`cacheWrite`; prompt option `timeout_ms`→`timeoutMs`; session option `mcp_servers`→`mcpServers`. Update adjacent comments referencing the old names. Verify: `cargo build`.
- [ ] 1.2 Update mock-agent test scripting in `src/bin/mock-agent/main.rs` if any comment/log references the renamed fields. Verify: `cargo build`.

## 2. Definitions and examples

- [ ] 2.1 Rename the fields in `types/ponos.d.luau`. Verify: `cargo test --test typed_results` (definitions sync + strict-mode analysis fixtures).
- [ ] 2.2 Update `examples/watchdog.luau` to the new field names. Verify: `cargo test --test examples`.

## 3. Tests

- [ ] 3.1 Update `tests/fixtures/types_probe.luau` (runtime probe) to read the renamed fields. Verify: probe test in `cargo test --test typed_results`.
- [ ] 3.2 Update `tests/acp.rs`, `tests/e2e.rs`, `tests/typed_results.rs` fixtures asserting `stop_reason`/`timeout_ms`/`mcp_servers`. Verify: `cargo test --test acp --test e2e --test typed_results`.
- [ ] 3.3 Full suite green: `cargo test` (offline, mock agent only).

## 4. Docs

- [ ] 4.1 Update `README.md` API table/sections if they mention the renamed fields. Verify: `grep -rn 'stop_reason\|cache_read\|cache_write\|timeout_ms\|mcp_servers' README.md examples/ types/ tests/ src/` returns only irrelevant hits (wire-protocol names like `session/update` comments are fine).
- [ ] 4.2 Ordering guard: confirm `add-typed-agent-results` has archived and re-read the synced `openspec/specs/` (scripting, agent-sessions, type-definitions, typed-results); if its final text drifted from what this change's deltas assumed, rebase the deltas so nothing from it is reverted (especially the `result` option/field and the permission auto-allow wording). Verify: `openspec validate rename-script-api-camelcase` passes and `grep -rn 'stop_reason\|cache_read\|cache_write\|timeout_ms\|mcp_servers' openspec/specs/` returns no hits.
