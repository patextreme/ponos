# Drop the std convergence loop — loops are component-local policy

## Why

`std/converge.luau` is a parameterized workflow, not a mechanism: it encodes
one loop shape (prompt → judge → escalate → fix → afterFix/onConverged) as a
twelve-field options table, requires the escalation phase even for loops
that don't want it, and already failed its second consumer once (the
openspec component worked around `onConverged` with its own archiver
session). Upcoming loop shapes — multiple judges, score-threshold exit
conditions, no escalation — do not reduce to prompt/judge/fix, so any std
loop that anticipates them becomes a config DSL: the original legibility
complaint arriving by another door. The things that are identical across
every loop shape (typed verdicts, capped retry, transport, repo fan-out)
already live in `std/predicate`, `std/gh`, and `std/daemon`; the loop
skeleton itself is ~30 lines of concrete, component-local policy.

The archived `factory-components` change pre-registered this outcome as a
risk ("a fourth shape may not reduce to prompt/judge/fix … if they never
extract cleanly, that is evidence about the primitive, not a failure").
The evidence has arrived; this change acts on it.

## What Changes

- Delete `factory-components/std/converge.luau`; no replacement module, no
  deprecation shim (external consumers are components-only, verified).
- The `openspec` and `pr-review-loop` components each inline their loop in
  exactly the shape that workflow needs, built on `std/predicate`. The
  escalation prompts become per-component policy; the archived change's
  "escalation semantics owned once" goal is explicitly reversed.
- Loop *conventions* (per-iteration session ids, the
  `[id iteration N of M]` prompt header, iteration-cap error wording) become
  a documented paragraph in `factory-components/README.md` so the two
  component loops stay visibly aligned.
- The four std-level converge test scenarios are deleted; escalation and
  iteration-cap behavior is re-pinned at component level (two new scenarios)
  — today those failure paths are only pinned by the std scenarios.
- The ADR practice is retired: `docs/adr/0001` is deleted and its two live
  references repointed at the archived `factory-components` change.
  `openspec/` is the sole decision record.

Component `Config` types are unchanged: consumer shims and the `ptah check`
compatibility gate are unaffected. A `std/gh` gap found while
investigating (timeout/could-not-run raise, contradicting its
failures-are-data contract) is recorded as a deferred observation in
design.md — no change until a consumer mounts it.

## Capabilities

### Modified Capabilities

- `factory-components`: the generic "Convergence loop operation"
  requirement is removed — convergence behavior moves into the component
  requirements; the openspec component gains human-escalation and
  iteration-cap scenarios pinning the failure paths at the level where they
  now live.

## Impact

- `factory-components/std/converge.luau` (deleted);
  `factory-components/components/{openspec,pr-review-loop}/component.luau`
  (loops inlined); `factory-components/README.md`, `factory-components/std/README.md`
  (converge bullets removed, conventions paragraph added, ADR reference
  repointed); `README.md` (ADR link repointed); `docs/adr/` (deleted);
  `crates/ptah-cli/tests/factory_components.rs` (four std scenarios removed,
  two component failure-path scenarios added).
- No consumer-visible API change: shims call the same component operations
  with the same config.
- Validated prototypes in `.work/converge-drop/` (adopted) and
  `.work/converge-redesign/` (considered and rejected) are the primary
  sources for the decision; `.work/` is gitignored, so design.md carries
  the substance.
