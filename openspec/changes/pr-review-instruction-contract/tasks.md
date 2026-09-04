## 1. Component: built-in default + optional instruction document

- [ ] 1.1 Create `factory-components/components/pr-review-loop/default-instruction.luau`
  (`--!strict` data module returning the instruction string): port
  `.ptah/instructions/review-instruction.md` with the skill-file frontmatter
  and the "Determining What to Review" (`$ARGUMENTS` dispatch) section
  dropped, everything else verbatim, and append the classification
  directive to the Output section (every finding classified BLOCKING =
  must be resolved before acceptance / NON-BLOCKING). Verify by diffing
  against the source file (only the two drops + the appended directive
  differ) and by `ptah check` analyzing a scratch shim that requires the
  module.
- [ ] 1.2 In `factory-components/components/pr-review-loop/component.luau`:
  relax `reviewInstructionFile` to `string?`, require the default module,
  branch the review prompt (configured file: "Use the review instruction
  at {path}…" / default: inline the default instruction text above the
  review ask), keep the classification ask in both branches, and update the
  `Config` doc comment (classification contract, default when omitted,
  file wins). Verify with `ptah check` on two scratch shims — one omitting
  the field, one configuring it (both strict-clean).
- [ ] 1.3 Document in `factory-components/components/pr-review-loop/README.md`:
  the instruction contract (classification requirement, fixed vocabulary,
  different-component boundary), the built-in default (used when
  `reviewInstructionFile` is omitted, file wins, default is the contract's
  reference instance, `default-instruction.luau` is the worked example to
  copy), and update the config sample. Add the data-only sibling module to
  the layout description in `factory-components/components/README.md`.
  Verify by reading both back against the four scenarios in
  `specs/factory-components/spec.md`.

## 2. Dogfood the default

- [ ] 2.1 Drop `reviewInstructionFile` from `.ptah/workflows/pr-review-loop.luau`
  (the shim consumes the built-in default, exactly like any consumer).
  Verify with `ptah check .ptah/workflows/pr-review-loop.luau`.
- [ ] 2.2 Delete `.ptah/instructions/review-instruction.md` (and the
  instructions dir if left empty). Verify: `grep -rn "review-instruction"
  --include="*.rs" --include="*.luau" --include="*.md"` outside
  `openspec/` finds no references to the deleted path.

## 3. Offline tests

- [ ] 3.1 Rewrite `pr_review_loop_converges_review_fix_push`
  (`crates/ptah-cli/tests/factory_components.rs`) to default mode: omit
  `reviewInstructionFile`; assert the echoed prompt carries the default's
  classification directive (e.g. "BLOCKING") alongside the existing
  convergence and push assertions. Verify with
  `cargo test --test factory_components pr_review_loop_converges`.
- [ ] 3.2 Drop `reviewInstructionFile` from
  `pr_review_loop_dry_run_never_pushes_but_still_comments` (default mode;
  existing assertions unchanged). Verify with
  `cargo test --test factory_components pr_review_loop_dry_run`.
- [ ] 3.3 Add a file-mode precedence scenario: a test-authored instruction
  document in the project temp dir, configured via `reviewInstructionFile`;
  assert its path reaches the agent and the default's classification
  directive does not (file wins). Verify with the scenario's test run.
- [ ] 3.4 Leave the type-gate tests' file-mode configs unchanged (a string
  field remains well-typed); run `cargo test --test factory_components` and
  `cargo test --test examples` in the dev shell — all green, no unpinned
  strings.
