## 1. Library core (one representation everywhere)

- [ ] 1.1 `factory-components/std/predicate.luau` — `PredicateOptions.agent: Agent`; drop the internal `ptah.agent(options.agent)` (use the handle directly); update the header's "data-only options" contract line to the new wording (data plus declared ptah runtime handles). Verify: `nix develop -c cargo test --test factory_components predicate` passes.
- [ ] 1.2 `factory-components/components/openspec/component.luau` — `Config.agent`/`Config.judgeAgent: Agent` (doc comments updated: handle, not registry name); drop `local work: Agent = ptah.agent(config.agent)`; dedupe `verify()`'s archiver to `work:session({ id = "openspec-archiver" })`. Verify: `nix develop -c cargo test --test factory_components openspec` passes.
- [ ] 1.3 `factory-components/components/pr-review-loop/component.luau` — Config drops `repo`; `agent`/`judgeAgent: Agent`; drop the internal `ptah.agent`; review prompt becomes `…to review PR {prUrl}.` (no `in {config.repo}` phrase). Verify: `nix develop -c cargo test --test factory_components pr_review` passes.

## 2. In-repo shims

- [ ] 2.1 `.ptah/workflows/openspec.luau` — construct handles (`agent = ptah.agent("pi")`, `judgeAgent = ptah.agent("pi")`); set `changeName = "component-agent-handles"` for the change's own lifecycle run. Verify: `nix develop -c cargo test --test factory_components dogfood_openspec` passes.
- [ ] 2.2 `.ptah/workflows/pr-review-loop.luau` — construct handles; delete the `repo = "patextreme/ptah"` line; leave the `loop:review(...)` call untouched. Verify: `nix develop -c cargo test --test factory_components dogfood_pr_review` passes.

## 3. Contract docs

- [ ] 3.1 `factory-components/README.md` — reword the component-contract bullet ("Config is data — strings, numbers, booleans — plus ptah runtime handles where the component's config type declares them (`agent: Agent`). Functions are not configuration."); update the consuming example to construct handles. Verify: example is internally consistent with the components' exported Config types.
- [ ] 3.2 `factory-components/components/pr-review-loop/README.md` — config block: handle fields, no `repo` line; keep the per-call note that the PR URL is the sole repository context. Verify: config block matches the component's Config type field-for-field.
- [ ] 3.3 `factory-components/components/README.md` — update the "config is data-only" phrase to the new contract wording. Verify: no stale "data-only" phrasing remains in the tree (`grep -rn "data-only" factory-components/`).
- [ ] 3.4 Root `README.md` §Factory Components — update the shim example (`ptah.agent("claude")`) and the "Data-only config" bullet. Verify: rendered example compiles conceptually against the delta spec.
- [ ] 3.5 `skills/ptah/SKILL.md` §Factory Components — same example and contract-sentence update as 3.4 (in-repo edit; the deployed copy is a symlink and follows). Verify: `grep -n "registry name" skills/ptah/SKILL.md` returns no agent-config hits.

## 4. Test suite (offline)

- [ ] 4.1 `crates/ptah-cli/tests/factory_components.rs` predicate tests — options become `{ agent = ptah.agent("judge"), … }`. Verify: predicate tests pass.
- [ ] 4.2 Same file, openspec component tests — `agent = ptah.agent("demo")`, `judgeAgent = ptah.agent("judge")`. Verify: openspec component tests pass.
- [ ] 4.3 Same file, pr-review-loop test — drop `repo = "example/example"`, construct handles. Verify: `pr_review_loop_converges_review_fix_push` passes (push-prompt and instruction-path assertions unchanged).
- [ ] 4.4 Same file, read-only-mount test — handle config. Verify: `component_runs_from_a_read_only_mount` passes.
- [ ] 4.5 Same file, compat-gate test — typo script constructs the handle so `judgeAgnt` is the isolated error; wrong-type script drops `repo`; clean script drops `repo` and uses handles. Verify: `mistyped_component_config_is_a_check_finding` passes inside `nix develop` (needs luau-lsp on PATH).

## 5. Change lifecycle and full validation

- [ ] 5.1 Full offline suite green: `nix develop -c cargo test` (includes examples, check pipeline, deps guard). Verify: zero failures.
- [ ] 5.2 `openspec validate component-agent-handles --strict` passes, then run the change's own lifecycle (groom/implement/verify via the updated openspec shim) and archive per the openspec workflow. Verify: change lands in `openspec/changes/archive/` and the synced spec carries the delta.
