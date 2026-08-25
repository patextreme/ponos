## 1. Core extraction (pure moves, zero port work)

- [x] 1.1 Create `src/core/` with `error.rs`; move task semantics (`TaskState`, `TaskRegistry`, `TaskResult`, spawn bookkeeping) from `src/task.rs` into `src/core/task.rs`, updating re-exports in `lib.rs`; verify `cargo test` green and `ponos.parallel` e2e tests unchanged
- [x] 1.2 Move `TurnFold`/`ToolFold`/settle logic out of `src/acp/mod.rs` **verbatim** into `src/core/turn/`; verify the 27 acp inline tests still pass (moved or re-pathed) and acp e2e streaming tests green
- [x] 1.3 Move `ResultContract` (schema compile, remote-`$ref` rejection) from `src/result_contract.rs` into `src/core/contract.rs`, leaving all socket/channel I/O behind in a new `src/result_wire.rs`; verify typed-results inline tests and `tests/typed_results.rs` green
- [x] 1.4 Move config model (`AgentSpec`, `Registry` types, merge + `${VAR}` interpolation) from `src/config.rs` into `src/core/config/`; leave TOML parse + fs discovery in `src/config_fs.rs`; verify config inline tests (moved) and `tests/cli.rs` registry tests green
- [x] 1.5 Move `select_allow_option` (AllowAlways-preferred headless permission selection) from `src/acp/mod.rs` into `src/core/` with unit tests covering: AllowAlways preferred, first-allow fallback, no-allow-options case; verify mock permission e2e tests green
- [x] 1.6 Audit `src/core/` imports: no mlua, tokio I/O, fs, acp, render, or async-process reachable (grep + review); fix stragglers; verify `cargo build` with core's reduced dep set unaffected elsewhere

## 2. Event sink port (D2)

- [x] 2.1 Define `SessionEvent` enum + payloads in `src/core/events.rs` (text delta w/ break flag, tool call id/kind/title/status, usage, stderr chunk, session lifecycle, result verdict); verify it compiles with `core`-only deps
- [x] 2.2 Define `EventSink` port in `src/core/ports.rs`; implement it in `src/render/` (existing `DisplayEvent` mapping + all formatting stays); verify render inline tests pass against new event types with unchanged expectations
- [x] 2.3 Rewire the ACP driver to emit `SessionEvent`s through the sink instead of calling `Renderer` directly; verify full e2e render output byte-identical (`tests/e2e.rs`, `tests/cli.rs` timestamp-stripped comparisons green)
- [x] 2.4 Rewire `result_wire` lifecycle messages through the sink (drop its `Arc<Renderer>`); verify `tests/typed_results.rs` green

## 3. Interaction-policy + config-source ports (D3, D2 remainder)

- [x] 3.1 Define `InteractionPolicy` in `src/core/ports.rs` with the headless impl from 1.5; make the ACP driver consult it for `session/request_permission` while other agent→client requests keep method-not-found in the adapter; verify mock permission/unknown-request e2e tests green
- [x] 3.2 Define `ConfigSource` in `src/core/ports.rs`; implement in `src/config_fs.rs` (discover + load both TOML layers); wire from `cli.rs`; verify registry discovery tests + `ponos check` config-finding tests green

## 4. Check decoupling + bridge inversion (D5, D6)

- [x] 4.1 Move `TYPE_DEFINITIONS` from `src/cli.rs` to `src/check/`; verify `cli ↔ check` cycle gone (`cargo dep` grep for `crate::cli` in check) and `tests/check.rs`, `tests/analyze.rs`, `tests/types.rs` green
- [x] 4.2 Give `check/lint` its own zero-execution require-graph path resolver (no `script::require` import; same directory rules); verify lint inline tests + `tests/check.rs` green
- [x] 4.3 Introduce `BridgeConfig` (server name + env wiring) flowing from `cli.rs` composition into session options; `bridge::SERVER_NAME` referenced only in `src/bridge.rs`; verify no `acp → bridge` import remains and `tests/typed_results.rs` green

## 5. Transport seam + god-module dissolution (D4)

- [x] 5.1 Split `src/acp/mod.rs` into `acp/process.rs`, `acp/proto.rs`, `acp/driver.rs` with `start_session`'s signature unchanged; no lock/channel-topology changes across await points; verify full acp test set green
- [x] 5.2 Define `AgentTransport` in `src/core/ports.rs` shaped by what `script` consumes (session spawn, prompt/cancel/close, config options); ACP driver implements it; `script` depends on the port, not the adapter; verify `tests/script.rs` + e2e green
- [x] 5.3 Split `src/script/mod.rs` into sandbox setup, `ponos.*` bindings, runtime state, and run entrypoint modules; verify `tests/script.rs`, `tests/examples.rs`, `tests/e2e.rs` green

## 6. Final polish within this change

- [x] 6.1 Dissolve remaining cross-adapter imports: run `grep -rn "use crate::" src/` and confirm arrows point `adapter → core` or `→ cli` only (exception list: none); fix violations; verify `cargo test` green
- [x] 6.2 Full gate: `cargo test`, `cargo clippy -- -D warnings`, `nix flake check` all green; `git diff --stat tests/ examples/` empty

## Implementation notes (deviations surfaced during apply)

The pinned, untouched integration-test surface (`tests/acp.rs`,
`tests/script.rs`, `tests/e2e.rs` construct `SessionOptions` with 4-field
literals, `RunConfig` with 4-field literals, and call `start_session` /
`run` / `setup_lua` with fixed arity) made three of the design's ideal
arrows inexpressible without editing tests, which task 6.2 forbids:

1. **BridgeConfig does not flow from `cli` composition** (4.3): a new
   `SessionOptions`/`RunConfig` field is not constructible from the test
   literals. It lives as core-owned data (`BridgeConfig::ponos_bridge()`)
   with a unit test in `bridge.rs` pinning it to the bridge binary's
   constants — the `acp → bridge` import (the task's verification gate)
   is gone.
2. **`script → acp` survives as one documented composition line** (5.2):
   `default_transport()` in `script/state.rs`. Change ② moves it into
   `cli` when `RunConfig` can carry an injected transport (test surface
   updated there).
3. **`acp → result_wire` and `bridge → result_wire` remain** (6.1): the
   driver owns the fold, sink, label, and injected servers the per-session
   result-channel wiring needs (no dissolution without new pinned-surface
   parameters), and the bridge is the client half of the submit/verdict
   protocol. `result_wire` tests no longer import `render` (recording
   sink). The five adapter modules (`acp`, `render`, `check`, `script`,
   `config_fs`) import core only.

Also noted: `core` holds data-level mlua (`task`) and
`agent_client_protocol` schema types (`turn`, `session`, `ports`) — the
explicit verbatim moves of tasks 1.1/1.2 — and `std::env` reads (HOME,
`${VAR}`); it stays free of fs/process/socket I/O and of all adapter
modules, as audited in 1.6.
