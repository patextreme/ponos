# Design — drop the std convergence loop

## Context

`std/converge.luau` was extracted as the "flagship primitive" of the
factory-components change, generalizing the review→judge→fix loop that
appeared four times across three repos. Two sessions of stress-testing
(concluded in a grilling session whose decisions are ledgered below)
concluded the extraction was premature: the loop is policy, not machinery.

Two throwaway prototypes were built and validated against the real
toolchain (`ptah check` strict-clean, mock-agent behavioral runs matching
the suite's pinned expectations):

- `.work/converge-drop/` — the loop inlined into both components (adopted:
  this design's implementation reference)
- `.work/converge-redesign/` — a restructured single loop with optional
  escalation and a status result (considered and rejected: it fixes the
  legibility symptom but still pins one shape)

## Why the loop is policy, not machinery

- The parts that vary across loops — judges (count, combination), exit
  conditions (predicate, score threshold), escalation, follow-ups — are
  the workflow's own decisions. A std loop anticipating them becomes a
  config DSL (`judges = {…}, exit = {kind = "threshold"}`), which is the
  original "flow is hard to understand" complaint arriving by another door.
- The mechanisms every loop shape shares already exist as std modules:
  `std/predicate` (typed boolean verdict, bounded retry),
  `std/gh` (transport), `std/daemon` (per-repo isolation). Score-threshold
  exits need nothing new: `resultSchema` accepts full JSON Schema
  (`number`, bounded objects), so a typed score judge is a plain session.
- Evidence against the primitive: the openspec component already worked
  around it (its own archiver session instead of `onConverged`);
  `afterFix`/`onConverged` each had exactly one consumer; and the named
  upcoming shapes (multi-judge, thresholds) don't fit prompt/judge/fix.
- The archived change pre-registered this: "if they never extract cleanly,
  that is evidence about the primitive, not a failure."

## The new rule

**std contains only mechanisms a third consumer would use verbatim**
(transport, typed verdicts, capped retry, isolation). **Loop shape is
component policy**, written per component in the shape that workflow
needs. When a real third loop lands, extract whatever *mechanics* prove
shared (e.g. a capped-retry number judge → `std/score`), never the loop
policy — the same evidence-driven rule that deletes converge here.

## What each component keeps

The openspec component's loop: per-iteration session, judge via
`std/predicate`, escalation probe judged by the same judge agent, fix
prompt, iteration cap. No follow-up prompts (verify's archive step stays
in its own session — it doesn't need the converged session's context).

The pr-review-loop component's loop: same skeleton plus its own shape —
escalation validates blocking findings with a subagent before asking, each
fix is followed by a commit-and-push prompt (convergence works because
fixes land in repo state between per-iteration stateless sessions), and
the converged session posts the verdict comment.

Shared conventions (documented in `factory-components/README.md` so drift
stays visible): per-iteration work sessions `<prefix>:<n>`, judge sessions
`<prefix>-judge:<n>`, escalation-judge sessions `<prefix>-human:<n>`, and
every prompt prefixed `[<prefix> iteration N of M]` so agents and logs see
loop state. Error wording: `{id}: human input is required to resolve the
findings (iteration N)` / `{id}: did not converge within M iterations`.

## Explicitly given up

- "Escalation semantics owned once" (a stated goal of the archived
  factory-components change): the two components' escalation prompts now
  differ *by intent* — they were never actually the same ("findings" vs
  "blocking issues" plus subagent validation); the std "default" unified
  them cosmetically.
- ~10–15 lines of loop mechanics duplicated per component. Cheaper than
  one wrong abstraction at n=2; the conventions paragraph is the guard.
- The structured status result considered in the rejected prototype:
  components fail loud (script error → exit 1), which is the desired
  end-to-end semantics under `std/daemon`'s per-repo error isolation.

## Decisions ledger (grilling session)

1. External consumers (identus-ws, midnight) are components-only — delete
   outright, no deprecation shim.
2. std afterwards: `predicate` + `gh` + `daemon` + conventions paragraph.
   No new modules.
3. Escalation prompts inline per component; reversal recorded here.
4. Port failure-path tests to component level (escalation + cap) — the
   std scenarios were their only pin.
5. Pure cut — the hypothetical score-gate component stays in `.work/`
   until a real consumer exists.
6. PR listing/comment helpers: not now — midnight deliberately replaced
   `gh pr list` discovery with a purpose-built claim-aware CLI; comments
   remain a one-liner over `std/gh` transport when a consumer names its
   shape.
7. ADRs retired: `openspec/` is the sole decision record; `docs/adr/0001`
   deleted, references repointed at the archived change.

## Deferred observation (no change until a consumer exists)

`std/gh.run()` calls `ptah.exec` unwrapped, so could-not-run and
timeout raise — contradicting the module's "failures are data" contract
and unwinding any daemon that uses it. Midnight's repo-local copy has
the fix (pcall-wrap; raise → `exitCode = -1` data) plus a typed
`JsonOutcome<T>` decode, and uses raw command strings our args-array
`run()` cannot express. Grilled and deliberately deferred: nothing
changes until a consumer actually mounts `std/gh` — then that consumer's
shape (and midnight's drifted copy) arbitrate the fix, per the same
evidence-driven rule that deletes converge here. `ptah.exec`'s own
raise-on-timeout surface stays as-is regardless: std is the adapter.
