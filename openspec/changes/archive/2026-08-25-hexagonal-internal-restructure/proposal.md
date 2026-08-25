## Why

ponos grew in a few days into a flat 13-module crate (~5.8k lines) with four
coupling violations and two god modules. The seams needed for longevity —
between domain logic and the ACP transport, terminal rendering, config I/O,
and headless interaction policy — do not exist yet. The codebase is young
enough that establishing hexagonal structure now is cheap; every week of
growth on the flat layout makes the eventual split harder.

This is change **① of a three-change sequence** (① internal restructure,
② physical workspace split, ③ polish). It does all the architectural work
inside the single crate so that ② becomes a mechanical file move.

## What Changes

**Pure structural refactor — zero observable behavior change.** CLI surface,
exit codes, rendered output, ACP wire behavior, and the full offline test
suite stay byte-identical and green.

Module tree rebuilt to the target shape (single crate, new module layout):

- New `core` module (pure domain, no fs/process/socket I/O; data-level
  mlua and tokio sync allowed):
  turn/tool fold logic (`TurnFold`/`ToolFold`), task semantics
  (`TaskRegistry`/`TaskState`), `ResultContract` schema compilation, config
  model + merge + `${VAR}` interpolation, domain event types, error types.
- Four internal ports introduced as traits in `core`:
  - `AgentTransport` (spawn session, drive turns) — implemented by the ACP
    stdio adapter;
  - event/output sink (driver emits structured events upward instead of
    calling `Renderer` directly);
  - `ConfigSource` (registry discovery) — implemented by the TOML/fs loader;
  - interaction policy (agent→client request decisions: headless
    allow-all permission selection) — lifted out of the ACP driver.
- Domain events become **structured** (session id, kind, text delta, tool
  call state, usage, stderr) — formatting (truncation, line budgets, colors)
  moves to the render module. TUI-ready by design.
- Four coupling violations fixed in place:
  1. `cli ↔ check` cycle: `TYPE_DEFINITIONS` moves to `check`;
  2. `acp → bridge::SERVER_NAME`: bridge server name defined where consumed
     via injected value, dependency arrow reversed;
  3. `acp → render`: driver goes through the event-sink port;
  4. `check/lint → script::require`: static lint gets its own (zero-execution
     by construction) require-graph walker.
- God modules dissolved: `acp/mod.rs` (1928 lines) splits into transport /
  driver / folds (folds to `core`); `script/mod.rs` (762 lines) splits into
  sandbox setup, `ponos.*` bindings, runtime state, and the run entrypoint.

No changes to: `mock-agent`, `tests/` (integration tests untouched and stay
green), `examples/`, Cargo dependencies, nix/flake.

## Capabilities

### New Capabilities

None — no new externally observable behavior.

### Modified Capabilities

None — spec-level behavior is unchanged everywhere (pure refactor).

`skip_specs: true` — this change alters internal structure only.

## Impact

- **Code**: every module under `src/` moves or splits; `lib.rs` re-exports
  the new tree. Public crate API surface is internal-only (binary crate).
- **Tests**: integration tests (`tests/`) untouched and must stay green.
  Inline unit tests move with their code; new unit tests ride along with
  extracted pure logic (folds, config merge, permission selection) where
  they don't require the e2e harness.
- **Interaction with in-flight change**: `richer-render-logging` touches
  render output built on today's `DisplayEvent` shapes; sequencing matters —
  whoever lands second rebases. The structured-event rework in this change
  is the better base for it (peeks/kinds live in event payloads, not in
  format strings).
- **Out of scope**: physical workspace split (change ②), lint enforcement
  and docs (change ③), any new behavior (strict preservation).
