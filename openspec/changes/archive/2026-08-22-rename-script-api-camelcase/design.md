# Design

## Context

The Luau-visible field names (`stop_reason`, `cache_read`, `cache_write`, `timeout_ms`, `mcp_servers`) are produced in one place (`src/script/mod.rs`, `new_session_obj` and the session factory), declared in one place (`types/ponos.d.luau`), exercised by the runtime probe (`tests/fixtures/types_probe.luau`), the integration tests, and one bundled example. The mock agent and `src/acp/mod.rs` mention the names only in test-scripting/comments and Rust-side struct fields — the ACP wire is untouched. The repo is pre-release with no tags; there are no external consumers to migrate.

## Goals / Non-Goals

**Goals**

- Every multi-word field visible to Luau scripts is camelCase.
- Zero behavior change: same values, same error messages, same wire traffic.

**Non-Goals**

- Any aliasing/deprecation period (old names must simply stop existing; the probe test enforces the definitions contract, and a stale snake_case field would fail it once the definitions are renamed).
- Renaming Rust-side identifiers (`TurnOutcome.stop_reason` etc.) — internal naming follows Rust conventions and is out of scope, except where a comment references the Luau field name.

## Decisions

- **Renames in one pass, verified by `ponos types` sync.** The definitions file is emitted verbatim by `ponos types` (byte-identical modulo the header), and the probe script exercises every declared field against the mock agent — the existing sync checks turn a missed rename into a test failure rather than a silent gap.
- **Old names become type errors, not runtime nils.** Because the definitions are strict-typed and the probe asserts actual values, a script using `r.stop_reason` post-change fails analysis (field does not exist) and returns `nil` at runtime. That is the intended breaking behavior; no shim.

## Risks / Trade-offs

- **Breaking change** for any in-flight scripts. Accepted: pre-release, single example to update, and the follow-up `session-config-options` change assumes the new names.
- **Delta ordering**: `add-typed-agent-results` (in progress) archives before this change. Its MODIFIED blocks replace whole requirements, so this change's deltas are written against its post-archive text (`result` option/field carried forward, `typed-results` capability present) — a delta based on today's main specs would silently revert those additions. Task 4.2 re-verifies against the synced specs at apply time in case its text drifted before archiving.

## Migration Plan

None needed beyond updating the in-repo callers listed in tasks.md (runtime binding, definitions, probe, tests, example, mock-agent env-var consumers if any reference the fields — `MOCK_*` env vars are unaffected).

## Open Questions

None.
