## 1. Core extraction (pure moves, zero port work)

- [ ] 1.1 Create `src/core/` with `error.rs`; move task semantics (`TaskState`, `TaskRegistry`, `TaskResult`, spawn bookkeeping) from `src/task.rs` into `src/core/task.rs`, updating re-exports in `lib.rs`; verify `cargo test` green and `ponos.parallel` e2e tests unchanged
- [ ] 1.2 Move `TurnFold`/`ToolFold`/settle logic out of `src/acp/mod.rs` **verbatim** into `src/core/turn/`; verify the 27 acp inline tests still pass (moved or re-pathed) and acp e2e streaming tests green
- [ ] 1.3 Move `ResultContract` (schema compile, remote-`$ref` rejection) from `src/result_contract.rs` into `src/core/contract.rs`, leaving all socket/channel I/O behind in a new `src/result_wire.rs`; verify typed-results inline tests and `tests/typed_results.rs` green
- [ ] 1.4 Move config model (`AgentSpec`, `Registry` types, merge + `${VAR}` interpolation) from `src/config.rs` into `src/core/config/`; leave TOML parse + fs discovery in `src/config_fs.rs`; verify config inline tests (moved) and `tests/cli.rs` registry tests green
- [ ] 1.5 Move `select_allow_option` (AllowAlways-preferred headless permission selection) from `src/acp/mod.rs` into `src/core/` with unit tests covering: AllowAlways preferred, first-allow fallback, no-allow-options case; verify mock permission e2e tests green
- [ ] 1.6 Audit `src/core/` imports: no mlua, tokio I/O, fs, acp, render, or async-process reachable (grep + review); fix stragglers; verify `cargo build` with core's reduced dep set unaffected elsewhere

## 2. Event sink port (D2)

- [ ] 2.1 Define `SessionEvent` enum + payloads in `src/core/events.rs` (text delta w/ break flag, tool call id/kind/title/status, usage, stderr chunk, session lifecycle, result verdict); verify it compiles with `core`-only deps
- [ ] 2.2 Define `EventSink` port in `src/core/ports.rs`; implement it in `src/render/` (existing `DisplayEvent` mapping + all formatting stays); verify render inline tests pass against new event types with unchanged expectations
- [ ] 2.3 Rewire the ACP driver to emit `SessionEvent`s through the sink instead of calling `Renderer` directly; verify full e2e render output byte-identical (`tests/e2e.rs`, `tests/cli.rs` timestamp-stripped comparisons green)
- [ ] 2.4 Rewire `result_wire` lifecycle messages through the sink (drop its `Arc<Renderer>`); verify `tests/typed_results.rs` green

## 3. Interaction-policy + config-source ports (D3, D2 remainder)

- [ ] 3.1 Define `InteractionPolicy` in `src/core/ports.rs` with the headless impl from 1.5; make the ACP driver consult it for `session/request_permission` while other agent→client requests keep method-not-found in the adapter; verify mock permission/unknown-request e2e tests green
- [ ] 3.2 Define `ConfigSource` in `src/core/ports.rs`; implement in `src/config_fs.rs` (discover + load both TOML layers); wire from `cli.rs`; verify registry discovery tests + `ponos check` config-finding tests green

## 4. Check decoupling + bridge inversion (D5, D6)

- [ ] 4.1 Move `TYPE_DEFINITIONS` from `src/cli.rs` to `src/check/`; verify `cli ↔ check` cycle gone (`cargo dep` grep for `crate::cli` in check) and `tests/check.rs`, `tests/analyze.rs`, `tests/types.rs` green
- [ ] 4.2 Give `check/lint` its own zero-execution require-graph path resolver (no `script::require` import; same directory rules); verify lint inline tests + `tests/check.rs` green
- [ ] 4.3 Introduce `BridgeConfig` (server name + env wiring) flowing from `cli.rs` composition into session options; `bridge::SERVER_NAME` referenced only in `src/bridge.rs`; verify no `acp → bridge` import remains and `tests/typed_results.rs` green

## 5. Transport seam + god-module dissolution (D4)

- [ ] 5.1 Split `src/acp/mod.rs` into `acp/process.rs`, `acp/proto.rs`, `acp/driver.rs` with `start_session`'s signature unchanged; no lock/channel-topology changes across await points; verify full acp test set green
- [ ] 5.2 Define `AgentTransport` in `src/core/ports.rs` shaped by what `script` consumes (session spawn, prompt/cancel/close, config options); ACP driver implements it; `script` depends on the port, not the adapter; verify `tests/script.rs` + e2e green
- [ ] 5.3 Split `src/script/mod.rs` into sandbox setup, `ponos.*` bindings, runtime state, and run entrypoint modules; verify `tests/script.rs`, `tests/examples.rs`, `tests/e2e.rs` green

## 6. Final polish within this change

- [ ] 6.1 Dissolve remaining cross-adapter imports: run `grep -rn "use crate::" src/` and confirm arrows point `adapter → core` or `→ cli` only (exception list: none); fix violations; verify `cargo test` green
- [ ] 6.2 Full gate: `cargo test`, `cargo clippy -- -D warnings`, `nix flake check` all green; `git diff --stat tests/ examples/` empty
