## Context

The pr-review-loop component consumes a repo-specific reviewer instruction
document (`reviewInstructionFile`) but its convergence gate — a typed
boolean judge over the verdict's prose — is phrased in a fixed
"blocking issues" vocabulary hardcoded across four prompts/predicates
(review ask, convergence predicate, escalation probe, fix prompt). Neither
the component docs nor the instruction document declares that taxonomy; the
loop's review prompt invents the classification at runtime. See
proposal.md — Why.

Prior settled decision (archived `drop-converge-loop` change): loop shape
is component policy, not std machinery — different convergence semantics
are different components, and abstraction is extracted only on evidence of
a real third consumer ("extract shared mechanics, never the policy").

New constraint: most consumers will never author an instruction document,
so the component needs a built-in default good enough to be their actual
reviewer — and this repository dogfoods that default (single source:
the repo's own workflows omit `reviewInstructionFile` and its instruction
markdown file is deleted).

## Goals / Non-Goals

**Goals:**

- Make the loop↔instruction vocabulary coupling a *declared* contract
  instead of an implicit one, with a single home per side: the component
  docs own the protocol vocabulary; a configured instruction document owns
  what "blocking" means for the repo.
- Make the component usable zero-config, with a built-in default that is a
  serious reviewer (not a stub) and satisfies the contract — the reference
  instance consumers can copy when graduating to a configured document.
- Single-source dogfooding: this repo consumes exactly what consumers get.

**Non-Goals:**

- No parameterization of the taxonomy (config knobs for ask/predicate/fix
  phrasings) — deferred until a real second instruction with a different
  same-shape vocabulary exists.
- No inline instruction-text config (a `reviewInstruction: string` field) —
  same evidence rule; repos that won't author a file won't author text.
- No typed structured verdicts; no new components (e.g. score-gate) — those
  follow the evidence-driven rule when a real consumer appears.

## Decisions

1. **Declare the contract; do not parameterize.** The component keeps its
   fixed blocking/non-blocking vocabulary and documents it as the required
   output format. *Alternative:* a `taxonomy`/`verdict` config table with
   the four phrasings — rejected for now (anticipates a hypothetical
   consumer, the converge-drop failure mode; single-label templating breaks
   on multi-category gates). Revisit on evidence.

2. **Ownership split: instruction owns semantics, loop owns protocol and
   enforcement.** The configured document must define what counts as
   blocking and instruct the classification; the loop keeps its
   review-prompt classification ask in both modes as enforcement. The ask
   is redundant with a compliant instruction by design.

3. **Keep the prose judge; do not switch convergence to a typed structured
   verdict** (e.g. the review session submitting `{ blocking: number }` via
   `resultSchema`). *Alternative:* schema-checked convergence — crisper
   seam, very ptah-idiomatic. Rejected: it makes the work agent self-grade
   its own convergence, while the independent judge over the verdict prose
   is the current design's load-bearing check.

4. **Boundary documented as "different component, not an instruction
   swap"** for verdicts that don't reduce to blocking/non-blocking —
   restating the loop-shape-is-policy rule at the seam where a consumer
   would otherwise misuse `reviewInstructionFile`.

5. **The built-in default is the real reviewer, embedded in the component.**
   Most consumers never graduate to a configured document, so a stub
   default would mean mediocre reviews running silently forever: the
   default is a full port of this repo's instruction, lightly adapted —
   skill-file frontmatter dropped (format debt in a prompt) and the
   `$ARGUMENTS` input-dispatch section dropped (the component fixes the
   review target to the PR URL; "no arguments → git diff" actively
   conflicts with "review PR {url}"), everything else verbatim, plus the
   classification directive so the default satisfies the contract.
   It lives in a sibling data module
   (`components/pr-review-loop/default-instruction.luau`) rather than an
   inline constant — content, not logic; keeps the facade readable and
   content diffs reviewable — and rather than a sibling markdown file,
   which the sandboxed module cannot locate (no self-path at runtime) and
   which a mount-path reference would de-zero-config anyway.
   `reviewInstructionFile` relaxes to `string?`; a configured file wins.
   *Alternatives:* stub default (rejected above); keep the repo instruction
   file and accept duplication (rejected: two drifting copies, and the repo
   would stop dogfooding what consumers get); inline-text config option
   (deferred, see Non-Goals).

6. **Default persona is normal versioned behavior.** The spec pins only
   that the default satisfies the contract (classification directive
   present), not its content; tuning the default changes zero-config
   consumers' reviews on upgrade, which is ordinary dependency behavior —
   pinning a persona is exactly what configuring a document is for, and
   the README says so.

## Risks / Trade-offs

- [Contract drift: a configured instruction stops classifying] → the
  loop's review-prompt ask still elicits the split, so the gate degrades to
  the default blocking definition rather than breaking; the README contract
  is the visible check for consumers.
- [Default content drifts out of contract] → the default-mode test scenario
  asserts the classification directive reaches the agent; the contract
  scenario keeps the README honest.
- [Zero-config consumers' reviewer persona changes on upgrade] → accepted
  as versioned behavior (Decision 6); the README documents file-per-doc as
  the pin.
- [Dogfood switch leaves file mode unexercised in this repo's own
  workflows] → the offline file-mode precedence scenario (test-authored
  instruction document) keeps the configured-document path covered.
- [A real second vocabulary arrives soon] → add the minimal phrasing
  overrides then, defaults preserving today's strings; the declared
  contract makes that extension additive, not a redesign.
