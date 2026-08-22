## 1. Runtime binding

- [x] 1.1 Rename the Luau-visible fields in `src/script/mod.rs`: prompt result `stop_reason`→`stopReason`, `cache_read`→`cacheRead`, `cache_write`→`cacheWrite`; prompt option `timeout_ms`→`timeoutMs`; session option `mcp_servers`→`mcpServers`. Update adjacent comments referencing the old names. Verify: `cargo build`.
- [x] 1.2 Update mock-agent test scripting in `src/bin/mock-agent/main.rs` if any comment/log references the renamed fields. Verify: `cargo build`. — No change needed: remaining hits are `MOCK_USAGE` env-format doc, Rust identifiers, and ACP wire names, all out of scope.

## 2. Definitions and examples

- [x] 2.1 Rename the fields in `types/ponos.d.luau`. Verify: `cargo test --test typed_results` (definitions sync + strict-mode analysis fixtures). — Sync + analysis tests pass; the 9 failing tests are runtime fixtures updated in 3.1/3.2.
- [x] 2.2 Update `examples/watchdog.luau` to the new field names. Verify: `cargo test --test examples`.

## 3. Tests

- [x] 3.1 Update `tests/fixtures/types_probe.luau` (runtime probe) to read the renamed fields. Verify: probe test in `cargo test --test typed_results`.
- [x] 3.2 Update `tests/acp.rs`, `tests/e2e.rs`, `tests/typed_results.rs` fixtures asserting `stop_reason`/`timeout_ms`/`mcp_servers`. Verify: `cargo test --test acp --test e2e --test typed_results`. — acp.rs hits were Rust-side identifiers (out of scope); e2e.rs/typed_results.rs Luau fixtures updated.
- [x] 3.3 Full suite green: `cargo test` (offline, mock agent only).

## 4. Docs

- [x] 4.1 Update `README.md` API table/sections if they mention the renamed fields. Verify: `grep -rn 'stop_reason\|cache_read\|cache_write\|timeout_ms\|mcp_servers' README.md examples/ types/ tests/ src/` returns only irrelevant hits (wire-protocol names like `session/update` comments are fine).
- [x] 4.2 Ordering guard: confirm `add-typed-agent-results` has archived and re-read the synced `openspec/specs/` (scripting, agent-sessions, type-definitions, typed-results); if its final text drifted from what this change's deltas assumed, rebase the deltas so nothing from it is reverted (especially the `result` option/field and the permission auto-allow wording). Verify: `openspec validate rename-script-api-camelcase` passes and `grep -rn 'stop_reason\|cache_read\|cache_write\|timeout_ms\|mcp_servers' openspec/specs/` returns no hits. — Archived ✓; two drifted spots rebased (typed-results "lifecycle diagnostic (a `[ponos]` line shown under `--verbose`)", type-definitions refined analyzer-residual scenario wording); deltas then synced into main specs, all delta blocks verified present verbatim; `result` option/field carried forward; permission auto-allow requirement untouched.
