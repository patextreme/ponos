# Design

## Context

All three edits land in `src/script/mod.rs` (`new_agent_factory`'s session constructor and `bind_ponos`'s namespace table), with echo changes in `types/ponos.d.luau` (embedded verbatim, emitted by `ponos types`). Config application already exists as `SessionHandle::set_config`, which takes the session's turn lock and sends one `session/set_config_option`, folding the response into the session's live option state. `start_session` returns only after the ACP handshake and `session/new` complete, and the advertised option state is captured by then — so constructor-time config can reuse `set_config` directly with no new driver machinery.

## Goals / Non-Goals

**Goals:**
- Constructor config implemented by composing existing primitives (`start_session` + `set_config` per entry), not by a new ACP path.
- Hard renames with no compat aliases or error hints — old names simply stop existing.

**Non-Goals:**
- Any local validation of `config` keys/values against the session's advertised option state (agent responses are authoritative; matches `setConfig`).
- Ordered application of `config` entries (unspecified by spec; Luau table iteration order is unspecified anyway).
- Version bump or migration tooling for old script names.
- Per-session default `timeoutMs` or other constructor ergonomics beyond `config`/`resultSchema`.

## Decisions

**Constructor config reuses `SessionHandle::set_config` as-is.** The alternative — extending `SessionOptions` with config values applied inside the driver before it reports readiness — adds a second code path for the same wire request and complicates the driver's command loop. Calling `set_config` from the constructor closure after `start_session` returns is sequential with any later `setConfig` (same turn lock), needs no protocol changes, and fails with the exact error string `setConfig` already produces. On failure, close and join the handle and remove it from `state.sessions` before raising, so no zombie subprocess or session registry entry survives a failed constructor.

**`config` values are typed before the session spawns.** The constructor reads `config` first and rejects non-string/non-boolean values with a Lua error before `start_session` — same pre-send typing rule as `setConfig`, extended to the spawn boundary so an invalid table never costs a subprocess. Nil/empty table declares no config work.

**`resultSchema` rename is a key swap plus error-string touch-up.** `opts.get::<Option<Value>>("result")` becomes `"resultSchema"`; the compile-error prefixes already say "invalid result schema" and stay. The prompt-outcome field `r.result` is untouched. No "did you mean" hint for the legacy `result` key (decided: stock silent behavior — the session simply declares no contract, specified in the typed-results delta).

**`ponos.map` → `ponos.parallel` is a table-key rename.** `ponos.set("parallel", map)` with the same closure; `MapOptions` → `ParallelOptions` in the definitions. No poisoned `map` alias.

**Ordering across `config` entries: iterate the table as Luau gives it.** No sorted-keys or array-form alternative — the spec declares order unspecified, and agents with interdependent options can use `setConfig` calls in sequence.

**`config` is declared as a union of index signatures, not `{ [string]: string | boolean }`.** Implementation discovered that types imported from a definitions file check invariantly in luau-lsp: table literals never unify with a union-valued index signature, so `{ [string]: string | boolean }` rejects *every* `config` literal. Declaring `({ [string]: string } | { [string]: boolean })?` accepts homogeneous string and boolean literals and still flags non-string/non-boolean values (the type-definitions delta scenario). The cost — mixed string/boolean tables are flagged too — is documented as a known analyzer residual in the README; runtime accepts mixed tables freely (covered by the e2e accept test).

**Teardown observation needs a mock affordance (`MOCK_CONFIG_REJECT_DELAY_MS`).** A rejected constructor's agent lives for only tens of milliseconds — shorter than any reliable poll interval, making "the constructor reaped the process, not the end-of-run sweep" flaky to observe via /proc. The mock now optionally holds the rejection response (`MOCK_CONFIG_REJECT_DELAY_MS`), guaranteeing a wide alive window; the e2e test watches the tagged process disappear while the script is still sleeping (`runner.is_finished()` guard).

## Risks / Trade-offs

- [A legacy `result` key silently creates a plain session] → Spec'd explicitly (typed-results delta scenario "Legacy option name is not read"); typed-results example test makes the new name the visible path. Accepted: no hint machinery (decided).
- [Constructor failure after spawn leaks a subprocess if teardown is skipped] → The failure path closes + joins the handle and removes it from `state.sessions` before raising; an e2e test asserts no surviving agent process (mock agent PIDs are observable).
- [Agent rejects the second of N `config` entries] → First entry is already applied (agent state mutated). Accepted: unavoidable with sequential sets and identical to calling `setConfig` twice manually; the constructor still raises and tears down.
- [Definitions drift from runtime after rename] → Mitigated by the existing `type-definitions` spec requirements: the embedded file is the single source, examples and the types probe run through the binary, and `tests/analyze.rs` pins the analyzer-side scenarios with the real luau-lsp (sandbox-enforced via `PONOS_REQUIRE_REAL_LSP`).

## Open Questions

(none)
