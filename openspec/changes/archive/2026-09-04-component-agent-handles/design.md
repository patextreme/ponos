## Context

The library's current contract (see `openspec/specs/factory-components/spec.md`)
pins config as data-only — strings, numbers, booleans — with components
resolving agent registry names internally via `ptah.agent(config.agent)`.
The pr-review-loop component additionally carries a `repo` config field
used exactly once, in a prompt phrase, while the per-call PR URL already
names the repository. Both consumers of the library live in this repo
(the two `.ptah/workflows/` shims and the test suite's generated
projects); external consumers mount as pinned source and gate bumps with
`ptah check`. The runtime already offers `ptah.agent(name | spec) ->
Agent`, and `Agent`/`Session` are exported ambient types available to
every module under `ptah check` — the library modules already annotate
locals with them.

## Goals / Non-Goals

**Goals:**

- Agent identity in config is a runtime handle the consumer constructs;
  the library never constructs agents
- One agent representation everywhere: both components' `agent`/
  `judgeAgent` and `std/predicate`'s `PredicateOptions.agent`
- `pr-review-loop` loses the `repo` field; the PR URL is the sole
  repository context
- Contract wording that admits declared ptah runtime handles while
  keeping the callable-hooks prohibition and the `ptah check` gate

**Non-Goals:**

- Accepting registry-name strings in config (no `string | Agent` union)
- Accepting raw `AgentSpec` tables in config (a consumer wanting an
  inline spec calls `ptah.agent({ ... })` and passes the handle)
- Bare PR number support in `review()` (a future signature change if
  ever wanted, not config)
- Any ptah CLI/runtime change — the `Agent` type already exists as
  specified

## Decisions

**Agent-only, not a union.** Config fields become `Agent`. The
alternative `string | Agent` keeps existing shims byte-identical but
costs a `typeof` branch at every use site, doubles the representation
forever, and makes every config diagnostic noisier ("string | Agent
expected"). Agent-only is a clean break; breaking is free right now —
this repo owns every consumer, and a mounted external copy gets a
`ptah check` finding naming the field on bump, which is the gate working
as designed. Registry ergonomics survive at the shim (`ptah.agent("pi")`),
and inline specs fall out for free (`ptah.agent({ command = ... })`).

**The contract line: no callable hooks, declared runtime handles OK.**
"Config is data — strings, numbers, booleans — plus ptah runtime handles
where the component's config type declares them (`agent: Agent`).
Functions are not configuration." An `Agent` is a typed, exported shape
the analyzer knows; an arbitrary function is an opaque callable that
would reopen callback-style configuration (the thing data-only was
chosen to exclude). Config stays inspectable except for the declared
handle fields, and the check gate keeps validating every field.

**`repo` dropped without URL parsing.** The prompt becomes
`…review PR {prUrl}.`. Parsing `owner/name` out of the URL was rejected:
it would bake a `github.com/o/r/pull/n` pattern into a currently
forge-agnostic component and add a parse-error path, all to reproduce
information the agent (and `gh`) already extracts from the URL. A fork
PR's URL lives under the base repo, so the URL never names the wrong
repository.

**Predicate widens identically, in the same change.** `PredicateOptions.
agent: Agent`; `std/predicate` stops calling `ptah.agent`. Leaving the
judge a string while work agents are handles would make the judge the
single role a consumer cannot construct — incoherent, and the component
could not forward its judge handle into the predicate call it already
makes.

**Components keep owning sessions and model config.** The handle controls
agent construction only: components still create per-iteration sessions
(`agent:session(...)`) and apply `session:setConfig("model", ...)` from
the string config fields. `model`/`judgeModel` stay strings — they name
per-session config options, not agent identities.

**Archiver dedupe rides along.** `openspec`'s `verify` currently
constructs a second agent (`ptah.agent(config.agent):session(...)`) for
its archive step; it reuses the work handle's `work:session({ id =
"openspec-archiver" })` instead. Behavior-identical (sessions, not
agents, own subprocesses), and the file is already being edited.

## Risks / Trade-offs

- [Breaking config change for any mounted external consumer on bump] →
  Mitigation: `ptah check` reports a type error naming `agent`/
  `judgeAgent` (the gate exists for exactly this); the READMEs' examples
  show the new form; the archive entry records the migration.
- [Shims gain a construction line per agent role] → accepted cost of
  explicit handles; the shim remains the only workflow code the consumer
  owns and the construction site is where a consumer would inline a spec
  or wrap construction anyway.
- [Losing the "in {repo}" prompt phrase changes what the work agent
  sees] → Mitigation: the offline tests pin the prompt fragments that
  matter (instruction path, push instruction); `gh pr view <url>` infers
  the repository, and the fork case still lands on the base repo.

## Migration Plan

One atomic change: library modules, both in-repo shims, tests, and docs
move together — no in-repo consumer can observe the intermediate states.
Rollback is reverting the change. External consumers (none today) bump
the mount and run `ptah check`, which names every field to fix.

## Open Questions

(none — representation, contract wording, and scope were settled before
proposal)
