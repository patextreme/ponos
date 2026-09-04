## Why

Every ptah integration re-creates the same workflow machinery: the `gh`/exec
transport, the typed judge, the review→fix convergence loop, and the daemon
skeleton are duplicated across three trees today (identus-ws, midnight-ws,
and this repo's own `.ptah/utils`), and the copies have already drifted
(one-sided vs two-sided `trim`, divergent JSON outcome shapes, inverted
escalation predicates). Bug fixes must be hand-ported; standing up a new repo
means copying ~15 files and editing repo-specific bits.

## What Changes

- Add `factory-components/` — the shared workflow library consumed as
  source (mounted by each consumer repo; ADR-0001), containing:
  - `std/`: repo-agnostic helpers — `converge` (prompt → typed-predicate
    judge → fix loop with human escalation), `predicate` (typed boolean
    judge), `gh` (exec/JSON transport with the drift arbitrated into one
    shape), `daemon` (pcall-isolated repo loop, sequential or parallel)
  - `components/openspec/`: facade `new(config)` → `:groom(change)`,
    `:implement(change)`, `:verify(change)`; declared (not bundled)
    requirements: an agent with openspec skills and `openspec` on PATH
  - `components/pr-review-loop/`: extracted and generalized from identus-ws
    (repo-specific hardcodings become config); facade plus optional `run()`
    daemon sugar
- Establish the Component contract: a component is a facade of typed
  operations; config is data-only; components never require consumer files,
  never write inside the library tree, and never use a relative cwd
- Dogfood: this repo's `.ptah/workflows/*` become shims that require the
  library (`../../factory-components/…`); the local `.ptah/utils/` copies are
  removed in the same change
- Add test-only wiring: components exercised against the mock agent in
  `crates/ptah-cli/tests/` (no Rust behavior changes)
- Keep `factory-components/` in the flake's build source so a store symlink
  can target it
- Document consumption in `README.md` and `skills/ptah/SKILL.md`;
  vocabulary captured in `CONTEXT.md`

## Capabilities

### New Capabilities

- `factory-components`: the shared workflow library — stdlib surface,
  component facade contract, source-mount consumption model, and dogfooding
  requirements

### Modified Capabilities

(none — ptah's CLI behavior is unchanged; `require`, `check`, and `exec`
already support everything the library needs)

## Impact

- New tree `factory-components/` (Luau, `--!strict`), new tests in
  `crates/ptah-cli/tests/`, flake source filter update
- `.ptah/workflows/*` and `.ptah/utils/*` in this repo are rewritten as
  shims (behavior preserved: openspec groom/verify loops, pr-review-loop)
- No changes to any Rust crate's behavior or public API; `openspec/specs/`
  gains one capability
- Consumer repos (identus-ws, midnight-ws) can adopt by mounting the tree —
  out of scope for this change
