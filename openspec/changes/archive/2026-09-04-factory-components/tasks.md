## 1. stdlib extraction

- [x] 1.1 Create `factory-components/std/` and `factory-components/components/` and verify the tree exists with a placeholder `README.md` per directory
- [x] 1.2 Port the typed judge from `.ptah/utils/predicate.luau` into `std/predicate.luau` (`--!strict`, exported `PredicateOptions` type) and verify an offline test drives it against the mock agent, including the no-verdict retry bound ending in a script error
- [x] 1.3 Arbitrate the `gh` transport into `std/gh.luau` — typed JSON outcome record (identus shape), two-sided `trim` (midnight behavior), argument quoting — and verify with offline tests using a stub `gh` fixture on PATH (success-with-JSON and non-zero-exit scenarios, no raising)
- [x] 1.4 Implement `std/converge.luau` (prompt → judge → fix loop with first-class human escalation and iteration cap) and verify offline tests cover all four spec scenarios: first-pass pass, fixable failure converges, human escalation, iteration cap
- [x] 1.5 Implement `std/daemon.luau` (pcall-isolated per-repo loop, sequential and bounded-concurrency parallel via `ptah.parallel`) and verify an offline test shows one raising repo does not abort the others, in both modes

## 2. Components

- [x] 2.1 Create `factory-components/components/openspec/` exposing `new(config) -> instance` with `:groom(change)`, `:implement(change)`, `:verify(change)`; port groom/verify behavior from `.ptah/workflows/openspec-{groom,verify}.luau` onto `std.converge`/`std.predicate`; implement `:implement` on the same primitives; verify offline tests cover groom, verify-including-archive, and implement against the mock agent
- [x] 2.2 Write the openspec component's README declaring environment requirements (agent carrying the openspec skills, `openspec` on PATH) and verify the declared-requirements scenario in the spec is satisfiable by reading it
- [x] 2.3 Extract `factory-components/components/pr-review-loop/` from identus-ws's pr-review-loop: facade `new(config)` with repo specifics (instruction text, target repo, dry-run gate) as config; decide `run()` sugar at implementation time; verify offline tests drive review→fix→push convergence via the mock agent
- [x] 2.4 Export a strict `Config` type from each component and verify a deliberately mistyped config table in a scratch shim is reported by `ptah check` as a type error naming the field

## 3. Dogfood this repo

- [x] 3.1 Rewrite `.ptah/workflows/openspec-groom.luau` and `.ptah/workflows/openspec-verify.luau` as shims requiring `factory-components/components/openspec` and verify the existing workflow behavior is preserved (manual run against the mock agent via the test suite). *(As-built note: implemented as the two shims above, then consolidated into a single `.ptah/workflows/openspec.luau` exposing the full lifecycle in 7a63d32; behavior preservation stays covered by `tests/factory_components.rs`.)*
- [x] 3.2 Rewrite `.ptah/workflows/pr-review-loop.luau` as a shim onto the pr-review-loop component (same verification)
- [x] 3.3 Delete `.ptah/utils/` and verify `rg -l 'predicateAgent|gh.json' .ptah/` finds nothing — no second copy of judge or transport survives under `.ptah/`

## 4. Test wiring, packaging, docs

- [x] 4.1 Add the offline suite entries for every stdlib module and component entry point to `crates/ptah-cli/tests/` (mock agent only, no network) and verify `cargo test --test examples` (or the new test file) is green
- [x] 4.2 Add a read-only-mount test: copy the library tree to a read-only temp dir, run a component from a shim elsewhere, verify success (no writes inside the tree, no relative-cwd dependence)
- [x] 4.3 Keep `factory-components/` in the flake build source and verify `nix build` and `nix flake check` pass with the tree included
- [x] 4.4 Document consumption in `README.md` and `skills/ptah/SKILL.md`: mount-point freedom, shim pattern, data-only config, check-as-compat-gate; verify the documented example shim passes `ptah check`
- [x] 4.5 Run `openspec validate factory-components --strict`, then the full `cargo test` suite, and verify both pass
