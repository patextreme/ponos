# Tasks — drop the std convergence loop

## 1. Components

- [ ] 1.1 Inline the convergence loop into
  `factory-components/components/openspec/component.luau` from the
  validated prototype `.work/converge-drop/openspec-component.luau`
  (require path becomes `../../std/predicate`). Behavior parity: same
  prompts, session ids, header, and error strings as the current
  std-driven flow.
- [ ] 1.2 Inline the loop into
  `factory-components/components/pr-review-loop/component.luau` from
  `.work/converge-drop/pr-review-loop-component.luau`, keeping its
  dry-run gate, push-after-fix, and verdict-comment-on-converge steps.
- [ ] 1.3 Delete `factory-components/std/converge.luau`.

## 2. Docs

- [ ] 2.1 `factory-components/README.md`: drop the converge bullets; add
  the loop-conventions paragraph (session ids, prompt header, cap error
  wording); repoint the ADR reference to the archived factory-components
  change.
- [ ] 2.2 `factory-components/std/README.md`: drop the converge bullet.
- [ ] 2.3 `README.md`: repoint the `docs/adr/0001` link to the archived
  change; delete `docs/adr/0001-source-mounted-factory-components.md`
  (the directory goes with it).

## 3. Tests

- [ ] 3.1 In `crates/ptah-cli/tests/factory_components.rs`, delete the
  four std/converge scenarios (`converge_passes_on_the_first_pass`,
  `converge_fixable_failure_converges`,
  `converge_human_escalation_fails_without_a_fix`,
  `converge_iteration_cap_fails`); keep the `converges_on_second_pass`
  judge-rules helper (component tests use it).
- [ ] 3.2 Add `openspec_component_escalation_fails_without_a_fix`: judge
  rules reject the review and confirm human input → exit 1, stderr
  contains "human input is required", and no fix prompt reaches the agent.
- [ ] 3.3 Add `openspec_component_iteration_cap_fails`: judge rules reject
  everything with fixable findings → exit 1, stderr contains "did not
  converge within".

## 4. Verification

- [ ] 4.1 `cargo test --test factory_components` green.
- [ ] 4.2 Full suite green (`cargo test`), including the dogfood workflow
  tests (shims unchanged, proving the compat gate holds).
- [ ] 4.3 `openspec validate drop-converge-loop` clean.
