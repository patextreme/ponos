# Tasks

## 1. Session constructor: `resultSchema` + `config`

- [ ] 1.1 In `src/script/mod.rs` (`new_agent_factory`), rename the constructor option read from `"result"` to `"resultSchema"` (error prefixes unchanged). Verify: `cargo test --test typed_results` after the suite-wide rename in task 3.1, plus a new case asserting `result = …` yields a plain session (`r.result == nil`).
- [ ] 1.2 In the same constructor, read `config`: a table mapping option ids to string/boolean values. Reject non-string/non-boolean values with a Lua error naming the entry **before** `start_session` (no subprocess on bad input). Verify: new e2e cases for the typed rejection and for spawn-not-touched.
- [ ] 1.3 After `start_session` succeeds, apply each `config` entry via `SessionHandle::set_config`, in table iteration order (order unspecified by spec). On agent rejection: close + join the handle, remove it from `state.sessions`, then raise the `setConfig`-style error from the constructor. Verify: e2e cases using the mock's config affordances (`MOCK_CONFIG_OPTIONS`, `MOCK_CONFIG_REJECT`) — accept path (option folded before first prompt), reject path (error names the id, no surviving mock-agent process).

## 2. Namespace rename: `ponos.parallel`

- [ ] 2.1 In `bind_ponos` (`src/script/mod.rs`), rename the binding key `map` → `parallel` (closure unchanged); no alias, no poison. Verify: updated script tests pass; a new case asserts `ponos.map` reads as nil (script that calls it errors).

## 3. Definitions, examples, docs

- [ ] 3.1 Update `types/ponos.d.luau`: `SessionOptions` gains `resultSchema: { [string]: any }?` (replacing `result`) and `config: { [string]: string | boolean }?`; `MapOptions` → `ParallelOptions`; `ponos.parallel` in the global declaration; `PromptResult.result` comment updated to reference `resultSchema`. Verify: `cargo test --test types` (probe + embedded-file sync checks) and `cargo test --test examples`.
- [ ] 3.2 Convert `examples/model-fanout.luau` to the constructor form (`config = { model = … }`, no `setConfig`), switch `examples/fanout.luau` to `ponos.parallel`, `examples/typed_results.luau` to `resultSchema` + `ponos.parallel` where used. Update example comments accordingly. Verify: `cargo test --test examples`.
- [ ] 3.3 Update `README.md`: API tables (`agent:session` options row, `ponos.parallel` row, prompt-outcome row wording), typed-results section (declaration + snippets), session-config-options section (add constructor `config`, keep `setConfig` as the between-turns path). Verify: manual read; `ponos types` output matches the file (covered by tests/types.rs).

## 4. Suite-wide rename and regression sweep

- [ ] 4.1 Sweep `tests/` (script, typed_results, acp, e2e, cli, check, examples, types, fixtures) for `ponos.map` / `result =` constructor usage; rename to `ponos.parallel` / `resultSchema`. Verify: `cargo test` green (offline suite).
- [ ] 4.2 Add the negative-path tests from the deltas: legacy `result` key → plain session (`typed-results`), `ponos.map` nil (`scripting`), config rejection teardown with no surviving agent process (`session-config-options`). Verify: full `cargo test` and `nix flake check` (sandbox) both pass.
