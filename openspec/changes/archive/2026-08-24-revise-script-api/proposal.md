# Revise Script API

## Why

The script API grew organically and three rough edges have surfaced: the session option `result` is ambiguous (it holds a schema, while the prompt-outcome field `result` holds a value — same name, different things), `ponos.map` undersells what the function does (parallel fan-out, with mapping as an incidental detail), and per-session settings require a `setConfig` call after construction, which is noise for the common case of pinning a model at session creation.

## What Changes

- **BREAKING** — Session option `result` is renamed to `resultSchema`. The prompt-outcome field `r.result` (the accepted submission value) keeps its name. Scripts passing `result = …` to `agent:session()` fail: the constructor no longer reads the old key, so no contract is declared and the session behaves as a plain session.
- **BREAKING** — `ponos.map` is renamed to `ponos.parallel`, same shape: `ponos.parallel(items, fn, { concurrency = n })` returning per-item outcome entries in item order. No alias is kept; `ponos.map` becomes a nil read.
- `agent:session()` accepts a `config` option: a Luau table mapping config-option ids to string (select value id) or boolean values. It is sugar for one `setConfig` call per entry, applied after `session/new` completes and before the constructor returns. Application order across keys is unspecified. If the agent rejects any value, the constructor raises a Lua error carrying the config id and the agent's message, and the spawned subprocess is torn down. No local validation against the session's advertised options: the agent's response is authoritative. `setConfig` remains for between-turn changes.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `scripting`: "Agent and session API" requirement — session options list changes (`result` → `resultSchema`, new `config`); "Task and concurrency primitives" requirement — `ponos.map` → `ponos.parallel` with `concurrency` option unchanged.
- `typed-results`: "Result contract declaration" requirement — the declaring option is `resultSchema`. The prompt-outcome `result` field and all other requirements are unchanged.
- `session-config-options`: new requirement — constructor-time `config` option semantics (application, ordering, failure, teardown); `setConfig` requirements unchanged.
- `type-definitions`: "Definitions cover the script API" requirement — surface list updated (`parallel`, `resultSchema`, `config`, `ParallelOptions`).
- `cli`: "Uncaught script error fails the run" requirement text mentions `ponos.map` results; updated to the new name (no behavior change).

## Impact

- `src/script/mod.rs` — rename the constructor option read and the `ponos` binding; apply constructor config after `start_session` (reusing `SessionHandle::set_config`); teardown on rejection.
- `types/ponos.d.luau` — `SessionOptions` (`resultSchema`, `config`), `ponos.parallel`, `ParallelOptions`; embedded in the binary via the `types` subcommand.
- `README.md` — API tables, typed-results section, config-options section, examples.
- `examples/` — `model-fanout.luau` converts to the constructor `config` form; `typed_results.luau` and `fanout.luau` switch to `resultSchema` / `ponos.parallel`.
- `tests/` — suite-wide rename; new tests for constructor config (accept, reject + teardown, order-independence) and the hard rename (old names absent).
- No version bump: the project is pre-release experimental and free to break the script surface.
