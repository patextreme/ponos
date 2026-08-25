## Context

`agent:session({ config = … })` applies a config table after session creation,
iterating with `pairs()` (`src/script/mod.rs:341-434`): the table is typed
before spawn, applied entry-by-entry after the ACP session is ready, and a
rejection tears the session down inside the constructor. The proposal (see
Why) removes the option because unspecified iteration order cannot coexist
with agents whose option sets have internal dependencies. This design covers
the removal mechanics and the surface left behind.

Related facts established during investigation:

- The constructor does no unknown-key validation (`src/script/mod.rs:314-380`,
  five targeted `.get`s) — plain deletion would silently ignore old scripts'
  `config` keys.
- Luau/luau-lsp 1.67 does no excess-property checking on table literals
  (verified with both solvers and flag variations), so removing the field from
  `types/ponos.d.luau` produces no static signal — and none is added
  (proposal Q7b).
- `types/ponos.d.luau:78` declares `config` as a union of index signatures
  (`{ [string]: string } | { [string]: boolean }`) with a doc comment noting
  the mixed-table limitation; both die with the option.

## Goals / Non-Goals

Goals:

- Make every use of the removed option fail loudly, pre-spawn, with a message
  that teaches the replacement pattern.
- Preserve `setConfig` semantics exactly, with sequencing now the script
  author's explicit responsibility (spec delta adds the sequencing scenario).

Non-Goals:

- No static detection (no tombstone type, no lint) — settled in planning.
- No general unknown-option-key rejection; only `config` errors (misspelling
  protection is a possible future change).
- No change to ACP driver internals: `SessionHandle::set_config`, the
  `SessionCmd::SetConfig` path, and the turn-lock serialization stay as they
  are.

## Decisions

### D1: Key-presence rejection, checked with the other option reads

While reading the options table (alongside `id`/`cwd`/`mcpServers`/
`resultSchema`), do one targeted `opts.get::<Option<Table>>("config")`; when
`Some`, raise `mlua::Error::runtime` immediately — before the result-contract
compile and before `start_session` spawns anything. A populated and an empty
table behave identically: the key itself signals the removed API. This
reuses the existing "typed before the spawn boundary" pattern the removed
block already followed.

Alternative considered: deleting the read entirely (silent ignore) — rejected
in planning (worst failure mode: config quietly not applied).

### D2: Error text carries the migration inline

Message shape:

    config session option was removed: a config table cannot express
    application order, which matters for agents with dependent options
    (e.g. opencode resets `effort` when `model` is set). Apply config with
    session:setConfig(...) after session creation — set driving options
    (like `model`) first.

Single sentence-cluster, names the replacement API, carries the sequencing
hazard. It is a runtime error, catchable like every other `session()` error;
no distinct error kind is introduced (matches how bad `mcpServers`/
`resultSchema` values already fail).

### D3: Definitions edited in place, comment block removed wholesale

Delete the `config` field and its seven-line doc comment (including the
union-of-index-signatures rationale and the mixed-table caveat) from
`types/ponos.d.luau`. The type-definitions spec delta keeps the
analyzer-residual note (excess keys stay unflagged) so nobody "rediscovers"
the missing static signal as a bug.

### D4: The mock agent already covers the sequencing contract

The sequencing scenario (set `model`, then `effort`, agent re-derives
`effort`) needs an agent that folds `set_config_option` responses with a
dependent-option reset. Extend `src/bin/mock-agent/` with a scripted behavior
(env var, e.g. `MOCK_CONFIG_DEPENDENT`) rather than any real-agent test —
the suite is fully offline by policy. The existing `MOCK_*` config-option
scripting shows the pattern.

## Risks / Trade-offs

- [Scripts in the wild break with a runtime error instead of a type error]
  → accepted in planning (Q7b); the error fires pre-spawn and carries the
  full migration, so the cost is one run, not a debugging session.
- [Constructor atomicity loss: a rejected `setConfig` leaves the session
  alive] → the error propagates as before and the run-end sweep still reaps
  every subprocess; documented in the README as the
  configure-immediately-after-creation pattern.
- [Empty `config = {}` errors, which may surprise authors pruning entries]
  → deliberate (the key itself is the removed API); covered by a spec
  scenario so the behavior is pinned.

## Migration Plan

1. Implement in one pass (code, defs, README, examples, tests) — no release
   sequencing for a 0.1.0 CLI.
2. Rollback = revert the commit; no persisted state or wire-format change is
   involved.
3. `skills/ponos/SKILL.md` (the canonical in-repo copy consumers download)
   is updated with the rest of the docs: drop the `config` constructor option
   from the session-options table and the "pin at creation" example, remove
   the mixed-table luau-lsp workaround, teach sequential `setConfig` with the
   driving-option-first hint.

## Open Questions

(none)
