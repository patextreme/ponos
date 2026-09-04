## Why

The pr-review-loop component's convergence gate is a typed judge reading the
review verdict's prose, and its predicates are phrased in a fixed vocabulary
("blocking issues"). But the contract between the loop and the instruction
document is undeclared: the component hardcodes that vocabulary in four
prompts/predicates, while the instruction document never promises to classify
findings as blocking/non-blocking — the loop's review prompt smuggles the
taxonomy in at runtime. Swapping `reviewInstructionFile` for an instruction
with a different taxonomy (severity scales, score gates) silently ungrounds
the judge predicates.

Separately: most consumer repos will never author an instruction document.
Without a built-in default, the component is unusable zero-config and the
contract it imposes has no reference instance. Ship the contract's reference
instance *as* the built-in default, so the taxonomy is always grounded and
the component reviews out of the box.

## What Changes

- The pr-review-loop component's documentation declares an **instruction
  contract**: a configured instruction document must define what counts as a
  blocking issue for the repo and instruct the reviewer to classify findings
  as blocking/non-blocking; the component's judge predicates and fix prompts
  speak that fixed vocabulary. Verdicts that do not reduce to this
  classification (score gates, approve/request-changes, report-only reviews)
  are documented as a different component, not an instruction swap — per the
  settled "loop shape is policy" decision (archived `drop-converge-loop`
  change).
- The component ships a **built-in default instruction** in a new sibling
  data module (`default-instruction.luau`): a lightly-adapted port of this
  repository's `.ptah/instructions/review-instruction.md` — skill-file
  frontmatter and the `$ARGUMENTS` input-dispatch section dropped (the
  component fixes the review target to the PR URL), everything else verbatim
  — ending with the blocking/non-blocking classification directive, so the
  default satisfies the contract and serves as its reference instance.
- `reviewInstructionFile` becomes optional (`string?` — a compatible
  relaxation for existing shims). **A configured file wins** over the
  built-in default; when omitted, reviews run against the default. The
  loop's review prompt keeps its classification ask in both modes
  (enforcement for instructions that under-specify the output format).
- This repository **dogfoods the default**: both workflow shims
  (`.ptah/workflows/pr-review-loop.luau` and `.ptah/workflows/openspec.luau`)
  omit `reviewInstructionFile` — the same component and the same built-in
  instruction every consumer gets — and
  `.ptah/instructions/review-instruction.md` is deleted (single source).
- Tests: the existing runtime scenarios move to default mode (mirroring the
  dogfood), and a new file-mode scenario pins precedence with a
  test-authored instruction file.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `factory-components`: the capability's spec gains a new `PR review
  instruction contract` requirement carrying the instruction-contract and
  built-in-default scenarios — the declared
  classification vocabulary, the default's use when no document is
  configured, and precedence of a configured document (docs-as-behavior,
  mirroring the existing environment-requirements scenario).

## Impact

- `factory-components/components/pr-review-loop/`: `component.luau`
  (optional field, prompt branch, doc comments), new
  `default-instruction.luau`, `README.md` (contract + default docs);
  `factory-components/components/README.md` (layout note for the data-only
  sibling module).
- `.ptah/workflows/pr-review-loop.luau` and `.ptah/workflows/openspec.luau`
  (drop the field from both shims);
  delete `.ptah/instructions/review-instruction.md`.
- `crates/ptah-cli/tests/factory_components.rs`: rewrite the two runtime
  scenarios to default mode, add the file-mode precedence scenario; type-gate
  configs stay valid.
- No std, CLI, or crate changes. Behavior change is confined to the
  component's config surface: omitting `reviewInstructionFile` changes from
  a check-time type error to running against the built-in default.
