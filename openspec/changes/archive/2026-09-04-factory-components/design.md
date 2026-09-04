## Context

See proposal.md for motivation. The constraints that shape this design:

- ptah's `require` is relative-only (no absolute paths, no package names,
  no search paths) and deliberately has no directory boundary — so a
  library tree mounted anywhere inside a consumer repo already works at
  runtime, in `ptah check`'s lint graph, and in luau-lsp's analyze graph.
- A mount via nix flake input lands in the read-only nix store: a library
  module can never `require` back into the consumer repo, and can never
  write next to itself. Configuration must flow downhill: shim → component.
- `ptah.exec`'s `cwd` resolves against the invocation directory, not the
  script's directory; there is no script-dir API and no runtime script
  arguments, and this change adds none.
- Three duplication sites feed the extraction: identus-ws (merge-bot-prs,
  pr-review-loop), midnight-ws (agent-review), and this repo's `.ptah/`
  (openspec-groom, openspec-verify, pr-review-loop, utils/predicate,
  utils/config). The drift between them (one-sided `trim`, divergent JSON
  outcome shapes, inverted escalation predicates) is the arbitration input for the stdlib.
- ADR-0001 records the distribution decision: source-mounted library, no
  registry, no lockfile, `ptah check` as the compat gate.

## Goals / Non-Goals

**Goals:**

- One authoritative copy of the judge, transport, convergence loop, and
  daemon skeleton — consumed by this repo's own workflows (dogfooding is
  the drift-proofing)
- A component model where the consumer's shim is the only workflow code
  they own, and all repo specifics arrive as typed data
- Compatibility gate for free: strict-typed config means a consumer's
  `ptah check` validates their config against the component's config type

**Non-Goals:**

- Any ptah CLI behavior change (registry, install command, script args,
  script-dir API, sandbox changes)
- Distribution tooling — how a consumer mounts the tree (flake input +
  symlink, submodule, vendored copy) is their choice; we document, not build
- Versioning machinery (semver, changelogs, lockfiles) — the mount
  mechanism pins; `ptah check` gates
- Migrating identus-ws or midnight-ws — they adopt whenever they want
- Function hooks in config (data-only for v1; revisit when a real consumer
  hits the wall)

## Decisions

**Directory layout: `factory-components/{std,components/<name>}/`.**
`std` holds the repo-agnostic layer; each directory under `components/` is
one component. Alternative: `lib/` + `workflows/` — rejected to keep the
"software factory" vocabulary in one place and leave `workflows/` free for
consumer-repo conventions.

**Module interface: facade instances, not globals.** Each component module
returns a table with `new(config) -> instance`; instance methods are the
operations (`opsx:groom(change)`). stdlib modules return plain function
tables. Alternatives: a single `run(config)` appliance per component
(rejected — the lego demand is operations the consumer sequences);
module-level singletons configured by side effect (rejected — config must
be explicit and typed at construction).

**Flagship primitive: `converge`.** The review→judge→fix loop appears four
times across the three repos with drifted phrasings; `converge` takes
`prompt`, `judge` (a predicate string), `fix`, `maxIterations`, and an
escalation outcome, and owns the human-escalation semantics once. `groom`,
`verify`, `implement`, and pr-review-loop's loop become thin compositions
of `converge` + `predicate`.

**Arbitration of the gh drift: identus's richer shape wins, midnight's
two-sided `trim` wins.** The JSON outcome is a typed record (exit code,
stdout, parsed value), not a boolean-tuple return; `trim` strips both ends
(identus's one-sided version loses leading whitespace from `gh` output).
Where the two repos disagree without a functional reason, this repo's own
`.ptah` copies are the tiebreaker since they are the freshest.

**No writes, no relative cwd — enforced by convention and tests, not
sandbox.** Components needing scratch space take a path via config (the
consumer's repo). The offline tests assert components run against a
read-only *copy* of the library tree and with the invocation dir elsewhere,
so violations fail the suite rather than audit.

**Dogfooding via direct relative requires.** This repo's
`.ptah/workflows/*.luau` become shims requiring
`../../factory-components/...` — no symlink needed at home; the shims are
byte-for-byte the pattern a consumer would write (modulo the require prefix
pointing into the repo instead of at a mount point).

**Test wiring: extend the existing examples-style pattern.** New test
functions in `crates/ptah-cli/tests/` run each stdlib module and component
entry against the mock agent, plus a check-only test that a deliberately
mistyped config produces a `ptah check` finding (the compat-gate guarantee).
The mock agent is extended only if a needed behavior is missing.

## Risks / Trade-offs

- [Luau type ergonomics: strict-mode config tables with optional fields can
  produce noisy consumer errors] → Mitigation: every component exports its
  `Config` type and the docs show a complete example shim; the check test
  pins the failure mode so it stays legible.
- [No semver means a behavior change (not type-breaking) can reach a
  consumer silently on bump] → Mitigation: consumer updates are PR-gated
  diffs of the mounted source (ADR-0001); a `CHANGELOG.md` per component is
  the cheap escape hatch if this starts biting.
- [Component methods driving skills (openspec-*) depend on agent
  environment we don't control] → Mitigation: requirements declared in each
  component's README; the offline tests pin the prompt text so drift from
  the skills is visible at home first.
- [`converge` over-generalization: three ops fit today, but a fourth shape
  (e.g. merge-bot's gate chain) may not reduce to prompt/judge/fix] →
  Mitigation: merge-bot-prs and agent-review stay repo-local until a second
  consumer wants them; if they never extract cleanly, that is evidence
  about the primitive, not a failure.

## Migration Plan

Extraction and dogfooding land in one change; no consumer repo depends on
this repo's `.ptah/` layout, so there is nothing to migrate externally.
Rollback is reverting the change — the old `.ptah/utils` copies return with
the commit.

## Open Questions

- Final module naming inside `std` (`gh` vs `exec-transport`, `daemon` vs
  `loop`) — deferrable; the spec pins behavior, not names beyond the
  public primitives.
- Whether `components/pr-review-loop` keeps a `run()` daemon convenience or
  ships facade-only — decide at implementation once the facade is
  exercised; `run()` is sugar and can be added later without breaking
  anything.
