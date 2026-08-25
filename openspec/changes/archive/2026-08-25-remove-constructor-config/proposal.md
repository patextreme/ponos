## Why

The `config` session-constructor option applies a Luau table of config-option
settings after session creation, iterating with `pairs()` in unspecified order.
That order is load-bearing for agents with dependent options: opencode
re-derives the `effort` option from the model on every `model` set, so
`config = { model = "zai-coding-plan/glm-5.3", effort = "high" }` deterministically
lands on `effort = "low"` when iteration applies `model` last (traced on the
wire: the reset rides the model-set response). A string-keyed table cannot
express ordering, so the sugar cannot be fixed in place — but `setConfig` can,
by putting the sequencing under the script author's control. Removing the
option removes a silent-config-drift failure class instead of papering over it.

## What Changes

- **BREAKING**: `agent:session({ config = … })` is removed. A `config` key in
  the session-options table raises a catchable Lua error before any agent
  subprocess spawns, with a message naming the removal and the replacement:
  apply config via `setConfig` calls after `session()` returns, setting driving
  options (e.g. `model`) first when the agent has dependent options.
- The `config` field and its doc comment are deleted from
  `types/ponos.d.luau` (`SessionOptions`). No static detection replaces it —
  no excess-property checking exists in Luau/luau-lsp today, and no bespoke
  lint is added (Q7: option b).
- Only the `config` key is rejected; all other unknown option keys keep
  today's silent-ignore behavior (scope guard, Q6).
- README: the constructor-`config` section is replaced by a `setConfig`
  sequencing paragraph documenting the dependent-option hazard (opencode's
  `model` → `effort` reset as the concrete instance) and the
  configure-before-first-prompt pattern; the atomicity loss is noted (a
  rejected `setConfig` after `session()` returns no longer tears the session
  down inside the constructor).
- `examples/model-fanout.luau` and the type-definitions probe fixture are
  rewritten to sequential `setConfig` calls (never a `pairs()` loop — that
  would reintroduce the same ordering bug in userland).
- Tests: the three constructor-config e2e tests are rewritten or removed per
  the new contract (pre-spawn rejection; sequencing).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `session-config-options`: constructor `config` requirements are removed and
  replaced by a pre-spawn "config key rejected" requirement; sequencing
  guidance for `setConfig` becomes normative documentation.
- `scripting`: the `config` key is dropped from the session-options
  enumeration in "Agent and session API"; the option's semantics were
  already delegated to session-config-options, which now specifies the
  key's pre-spawn rejection.
- `type-definitions`: the `SessionOptions` type loses the `config` field; the
  "Constructor config type-checks" scenario is updated accordingly.

## Impact

- Code: `src/script/mod.rs` (constructor option parsing: remove the
  config-table block, add the key-presence rejection), `types/ponos.d.luau`.
- Specs: the three capability deltas above.
- Tests: `tests/e2e.rs` (constructor-config tests), `tests/examples.rs`
  (unchanged driver, updated example), `tests/fixtures/types_probe.luau`.
- Docs: README config section; `examples/model-fanout.luau`.
- Docs: README config section; `examples/model-fanout.luau`;
  `skills/ponos/SKILL.md` (the in-repo canonical copy consumers
  download — remove the `config` constructor option and the mixed-table
  workaround, teach sequential `setConfig` with the driving-option-first
  hint).
