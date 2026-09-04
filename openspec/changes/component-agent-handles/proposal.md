## Why

Dogfooding surfaced two frictions in the component config contract.
First, `pr-review-loop`'s `repo` field restates a property of the
per-call PR URL as instance config: the URL already names the repository,
`gh pr view <url>` infers it, and an instance configured for one repo can
be called with another repo's PR — the library's own "per-call data is a
method argument, not config" rule applied to the PR URL is contradicted
by its sibling field. Second, the `agent`/`judgeAgent` fields are
registry-name strings resolved with an internal `ptah.agent()` call, so a
consumer cannot control agent construction — the runtime already accepts
inline specs (`ptah.agent({ command = … })`) and the component contract
forbids the consumer from using that freedom.

## What Changes

- **BREAKING** — `pr-review-loop` Config drops `repo`: the review prompt
  references only the PR URL; repository context arrives per-call inside
  the URL.
- **BREAKING** — agent fields become `Agent` handles instead of registry-name
  strings: `agent`/`judgeAgent` in both components' `Config` and
  `agent` in `std/predicate.PredicateOptions`. Consumers construct
  handles with `ptah.agent(name_or_spec)`; components and stdlib stop
  calling `ptah.agent()` internally. This also unlocks registry-free
  consumers (inline `AgentSpec`) for free.
- The config contract is reworded: config is data — strings, numbers,
  booleans — plus ptah runtime handles where the component's config type
  declares them. Callable hooks remain forbidden.
- Cleanup inside the change's blast radius: `openspec`'s `verify`
  archiver session reuses the work handle instead of constructing a
  second agent.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `factory-components`: the component facade contract admits declared
  ptah runtime handles (`Agent`) in config while keeping the
  callable-hooks prohibition; the PR review loop requirement no longer
  lists the target repository as config (the PR URL is the sole
  repository context, per-call).

## Impact

- `factory-components/components/pr-review-loop/component.luau` — Config
  type (drop `repo`, `Agent` fields), prompt text.
- `factory-components/components/openspec/component.luau` — Config type
  (`Agent` fields), archiver session dedupe.
- `factory-components/std/predicate.luau` — `PredicateOptions.agent:
  Agent`.
- Docs: `factory-components/README.md` (contract wording, consuming
  example), `components/README.md`, `components/pr-review-loop/README.md`
  (config block, drop `repo`).
- Shims: `.ptah/workflows/pr-review-loop.luau`, `.ptah/workflows/openspec.luau`
  — construct handles, drop `repo`.
- Tests: `crates/ptah-cli/tests/factory_components.rs` — config
  construction in test scripts, prompt-pinning assertions that currently
  expect the `in {repo}` phrase.
- No Rust crate behavior changes; no CLI surface changes.
