## Context

Single crate, 13 modules, ~5.8k lines. Two god modules carry most of the
mass: `src/acp/mod.rs` (1928 lines: process spawn + JSON-RPC + turn folds +
permission policy + result wiring + rendering) and `src/script/mod.rs`
(762 lines: sandbox + bindings + runtime state + entrypoint). Known
violations: `cli ↔ check` cycle (`TYPE_DEFINITIONS`), `acp → bridge`,
`acp → render`, `result_contract → render`, `check/lint → script::require`.

Constraints (settled with the user — see proposal):

- Pragmatic hexagonal: ports only at the four funded seams (transport,
  output, config source, interaction policy). The Luau host is a fixed
  fixture, **not** a port.
- This change stays **one crate**; the physical `crates/*` split is change ②.
  Every module boundary created here maps 1:1 onto a future crate, so ② is a
  file move, not a redesign.
- Strict behavior preservation: integration tests (`tests/`) byte-identical,
  suite green at every commit. Inline unit tests move with their code.
- Runtime shape is a constraint: all Luau-side state is `!Send` on one
  `LocalSet`; only tokio sync primitives cross tasks. Port designs must not
  force `Send` bounds where none exist today.

## Goals / Non-Goals

**Goals:**

- A `core` module holding all pure domain logic, depending on nothing but
  std + serde + jsonschema.
- Dependency arrows that all point inward: `acp`/`render`/`config`/`check`/
  `script` → `core`, never across each other, never back into `cli`.
- Ports defined as `core` traits, implemented in the leaf modules, wired in
  `cli` (the composition root) — so change ② can move each implementor into
  its own crate without touching call sites again.
- Structured domain events flowing driver → sink, with all formatting
  isolated in `render`.

**Non-Goals:**

- The workspace split, lint enforcement, docs (change ③ territory).
- Async-trait object-safety gymnastics for hypothetical in-process
  transports; the `AgentTransport` port is extracted, not generalized.
- Splitting `mock-agent`, touching `tests/` or `examples/`.
- Any behavior change, including "improvements" the new structure makes
  natural.

## Decisions

**D1 — Module tree (the change-② crate map).** Target layout:

```
src/
  core/           # pure domain: no mlua, no tokio I/O, no fs, no acp
    turn/         #   TurnFold, ToolFold, settle logic
    task.rs       #   TaskState, TaskRegistry, TaskResult
    contract.rs   #   ResultContract (schema compile; no socket)
    config/       #   AgentSpec, Registry model, merge, ${VAR} interp
    events.rs     #   structured domain events (Event enum + payloads)
    ports.rs      #   AgentTransport, EventSink, ConfigSource, InteractionPolicy
    error.rs
  acp/            # adapter: stdio process + JSON-RPC + driver
  render/         # adapter: terminal line renderer (formats events)
  config_fs.rs    # adapter: TOML discovery + load (ConfigSource impl)
  script/         # fixed fixture: mlua sandbox, ponos.* bindings, require
  check/          # pipeline: compile pass, lints, luau-lsp shell-out
  bridge.rs       # MCP bridge binary plumbing (server name lives with it)
  result_wire.rs  # UDS channel + submission sink (I/O half of results)
  cli.rs          # composition root: parse, wire adapters into ports, run
```

*Why not `ports/`+`adapters/` folders:* domain-first folders map directly
to future crates and keep import paths meaningful; a ports/adapters tree
encourages the "everything is an adapter" over-generalization we chose
against in Q1(a). *Why `config_fs`/`result_wire` aren't in `core`:* they
exist to keep `core` I/O-free; their crate homes in change ② are `ponos-config`
and `ponos-cli` respectively.

**D2 — Event sink replaces `Renderer` in the driver.** `core::events` gains
a `SessionEvent` enum with structured payloads (text delta with
message-break flag, tool call id/kind/title/status, usage counts, stderr
chunk, session lifecycle, result verdict). The driver folds wire updates
(via `core::turn`) and emits `SessionEvent`s through an `EventSink` port;
`render::Renderer` implements the port and keeps all formatting
(truncation, budgets, colors, timestamps). `result_contract`'s lifecycle
messages likewise go through the sink instead of holding `Arc<Renderer>`.
*Alternative rejected:* passing `Arc<Renderer>` down as-is — keeps the
acp→render dependency and bakes display strings into the driver.

**D3 — Interaction policy port for agent→client requests.**
`select_allow_option` (prefer `AllowAlways`, else first allow option) moves
to `core` as the headless `InteractionPolicy` impl. The ACP driver consults
the policy for `session/request_permission`; every other request still gets
method-not-found (that fallback stays in the adapter — it is wire protocol,
not policy). *Why a port for one impl:* it is the exact seam a TUI needs to
make permissions interactive, costs one enum + one fn, and removes a
behavioral decision from transport code.

**D4 — Transport extraction boundary.** `start_session` keeps its public
signature but becomes a thin `AgentTransport` impl over a split interior:
`acp/process.rs` (spawn, stderr pump, kill/reap), `acp/proto.rs`
(initialize/new-session handshake, capability negotiation), `acp/driver.rs`
(command loop, `run_turn`, fold orchestration via `core::turn`, event
emission, config-option folding). `SessionHandle` stays the type the script
layer sees — it is already a good async façade. *Alternative rejected:*
defining the port against the full ACP surface — the port is defined by
what `script` calls (`session`, `prompt`, `cancel`, `close`, config
options), which is what mocks and future transports must satisfy.

**D5 — Check decoupling.** `TYPE_DEFINITIONS` moves to `check` (kills the
cycle). `check/lint` stops importing `script::require`: the lint walker
gets its own minimal `.luau` module-resolution routine (same up-front
directory rules, zero-execution by construction, no `Require` trait impl).
*Why duplicated rather than shared:* runtime require is an mlua `Require`
impl with I/O; lint needs a pure path resolver — sharing would re-couple
check to the script host.

**D6 — bridge server-name inversion.** The MCP server spec injected into
agent sessions (`bridge::SERVER_NAME` + env wiring) becomes data flowing
**into** the driver from `cli` composition (a `BridgeConfig` value in
session options), with the constant defined in `bridge.rs` and referenced
only there.

**D7 — Port mechanics: generics now, `dyn` only where needed.** Ports are
plain traits; call sites take `impl Trait` generics where the owner is
concrete (`EventSink` is held by the driver as a generic — or `Arc<dyn>`
if the object-safety is free). No `async_trait` crate, no speculation.
Exact choice per port lands during implementation; both keep change ② a
move-only step.

**D8 — Behavior verification is the existing suite.** No new e2e tests.
Green `cargo test` + `nix flake check` at every commit is the acceptance
gate; byte-identical `tests/` is the definition of "no behavior change".

## Risks / Trade-offs

- [Fold extraction subtly changes turn text semantics (message-break
  handling is finicky)] → move fold code **verbatim**, add unit tests over
  the extracted `TurnFold` before touching call sites; e2e streaming tests
  (acp.rs, e2e.rs) cover the observable side.
- [Event structs drift from `DisplayEvent` shapes and change rendered
  output] → `render` remains the only owner of formatting; port the
  existing render unit tests to consume the new event types unchanged in
  expectations.
- [Conflict with in-flight `richer-render-logging`] → that change builds on
  today's `DisplayEvent`; sequence it **after** this change and rebase —
  its peeks belong in event payloads anyway. Flagged in proposal Impact.
- [Split interior of `acp/mod.rs` introduces deadlocks via new lock
  boundaries] → keep the existing `turn_lock` discipline and channel
  topology exactly; module splits must not cross await points with new
  locks. The 27 acp inline tests move with their code.
- [Pragmatic ports regress into either zero or full ceremony] → the four
  funded ports are a closed set in review; new ports require a change of
  their own.

## Migration Plan

Single-crate, behavior-preserving, landed as a sequence of green commits in
one change (see tasks.md ordering): core extraction first (pure moves),
then port introductions one seam at a time (render sink → interaction
policy → config source → transport), then check decoupling and bridge
inversion, then final module-tree polish. Rollback = revert; no data, no
wire format, no CLI surface involved.
