## Purpose

The shared workflow library (`factory-components/`): repo-agnostic helper
modules and composable workflow components that consumer repositories mount
as source and drive through thin shims, replacing per-repo copies of the
same agent-orchestration machinery.

## ADDED Requirements

### Requirement: Convergence loop operation

The library SHALL provide a convergence-loop operation that drives an agent
session toward a typed predicate: it prompts an agent, judges the result,
and on failure prompts a fix and repeats, until the predicate holds, the
loop escalates to a human, or an iteration cap is reached.

#### Scenario: Predicate holds on first pass

- **WHEN** the operation runs and the judge confirms the success predicate for the agent's output
- **THEN** the loop exits successfully without issuing a fix prompt

#### Scenario: Fixable failure converges

- **WHEN** the judge rejects the output but confirms the findings are resolvable without a human
- **THEN** the operation issues the configured fix prompt and iterates, and exits successfully once a later pass satisfies the predicate

#### Scenario: Human escalation

- **WHEN** the judge rejects the output and confirms human input is required
- **THEN** the operation fails with an error stating human input is needed, and does not issue a fix prompt

#### Scenario: Iteration cap

- **WHEN** the predicate has not held after the configured maximum iterations
- **THEN** the operation fails with an error reporting the cap was reached

### Requirement: Typed judge

The library SHALL provide a typed boolean judge that asks a designated agent
model whether a predicate holds for a payload and returns the verdict from
the session's typed boolean result. A judge that submits no verdict SHALL be
retried a bounded number of times, and exhaustion SHALL be a script error,
never a hang or a silent default.

#### Scenario: Verdict returned

- **WHEN** the judge session submits a boolean verdict for the predicate and payload
- **THEN** the operation returns that verdict

#### Scenario: Judge never answers

- **WHEN** the judge session submits no typed verdict on every attempt up to the bound
- **THEN** the operation raises a script error naming the judge and the attempt count

### Requirement: GitHub CLI transport

The library SHALL provide a GitHub transport that shells out to the `gh` CLI
via ptah's exec, returning a structured outcome (exit code, stdout, parsed
JSON when requested) instead of raising, and SHALL quote arguments so values
containing spaces or quotes are passed through verbatim.

#### Scenario: Command succeeds with JSON output

- **WHEN** a `gh` invocation exits zero and JSON output is requested
- **THEN** the transport returns a success outcome carrying the parsed JSON value

#### Scenario: Command fails

- **WHEN** a `gh` invocation exits non-zero
- **THEN** the transport returns a failure outcome carrying the exit code and stderr, and the calling script keeps running

### Requirement: Daemon loop skeleton

The library SHALL provide a repo-loop skeleton that applies a per-repo
operation to every configured repository, isolates each repository behind an
error boundary so one raising repository cannot abort the others, and
supports both sequential and bounded-concurrency parallel execution.

#### Scenario: One repository raises

- **WHEN** the per-repo operation raises for one of several configured repositories
- **THEN** the loop records that repository's failure and completes the remaining repositories

### Requirement: Component facade contract

Every workflow component SHALL be a module exposing a constructor that
accepts a data-only config table and returns an instance whose methods are
the component's typed operations; config SHALL NOT contain functions.
Components and stdlib modules SHALL be strict-mode typed so that `ptah check`
in a consumer repo validates the consumer's config against the component's
config type.

#### Scenario: Consumer config mismatches the component type

- **WHEN** a consumer shim passes a config table that does not satisfy the component's exported config type and runs `ptah check`
- **THEN** check reports a type error naming the offending field

#### Scenario: Per-call data is a method argument

- **WHEN** a consumer calls a component operation that acts on a specific item (e.g. a change name or PR URL)
- **THEN** the item is supplied as a method argument, not baked into the component's config

### Requirement: Library self-containment

Library modules SHALL only require other modules within the library tree, so
the tree works mounted at any path; they SHALL NOT write files inside the
library tree; and they SHALL NOT invoke exec with a relative working
directory — repository-relative paths MUST arrive through config.

#### Scenario: Mounted at an arbitrary path

- **WHEN** the library tree is mounted at any directory inside a consumer repo and a shim requires a component by relative path
- **THEN** the component and its internal requires resolve without reference to the mount location

#### Scenario: Library tree is read-only

- **WHEN** a component runs from a read-only mount (e.g. the nix store)
- **THEN** the workflow completes without attempting to write inside the library tree

### Requirement: openspec component

The library SHALL provide an openspec component whose instance exposes
groom, implement, and verify operations on a named change: groom converges a
change's proposals through review, implement drives task execution, and
verify converges verification then syncs and archives the change. The
component SHALL declare its environment requirements (an agent carrying the
openspec skills, `openspec` on PATH) in its documentation rather than
bundling or installing them.

#### Scenario: Verify converges and archives

- **WHEN** verify runs on a change whose implementation passes the verification judge
- **THEN** the verification loop exits and the sync-and-archive step runs as part of the same operation

#### Scenario: Missing environment requirement

- **WHEN** the component's documentation is consulted for its environment requirements
- **THEN** the agent-skill and CLI requirements are listed so a consumer can verify them before running

### Requirement: PR review loop component

The library SHALL provide a PR review loop component that runs a
review→fix→push convergence against a pull request, with repository-specific
settings (agent prompts, reviewer instruction text, target repository,
dry-run gating) expressed as config rather than code.

#### Scenario: Review finds fixable findings

- **WHEN** the reviewer reports findings judged resolvable without a human
- **THEN** the component drives a fix session and pushes, iterating until the review passes or escalation occurs

### Requirement: Dogfooded in this repository

This repository's own `.ptah/workflows/*` SHALL be shims that require the
library rather than carrying their own helper copies; after this change no
workflow helper module SHALL be duplicated under `.ptah/`.

#### Scenario: Repo workflow uses the library

- **WHEN** the openspec groom and verify workflows in this repository run
- **THEN** their convergence and judging behavior comes from the library, and grepping `.ptah/` finds no second copy of the judge or transport

### Requirement: Offline test coverage

Every stdlib module and component entry point SHALL be exercised by the
offline test suite against the mock agent, with no network access and no
real agent.

#### Scenario: Library regressions caught offline

- **WHEN** a library module's behavior breaks (e.g. the judge stops returning verdicts)
- **THEN** the offline suite fails without spawning any real agent
