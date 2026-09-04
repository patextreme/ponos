## MODIFIED Requirements

### Requirement: Component facade contract

Every workflow component SHALL be a module exposing a constructor that
accepts a data config table and returns an instance whose methods are the
component's typed operations; config SHALL NOT contain callable hooks.
Ptah runtime handles (e.g. agent handles) SHALL be permitted as config
values where the component's exported config type declares them.
Components and stdlib modules SHALL be strict-mode typed so that `ptah check`
in a consumer repo validates the consumer's config against the component's
config type.

#### Scenario: Consumer config mismatches the component type

- **WHEN** a consumer shim passes a config table that does not satisfy the component's exported config type and runs `ptah check`
- **THEN** check reports a type error naming the offending field

#### Scenario: Per-call data is a method argument

- **WHEN** a consumer calls a component operation that acts on a specific item (e.g. a change name or PR URL)
- **THEN** the item is supplied as a method argument, not baked into the component's config

#### Scenario: Agent handle accepted as config

- **WHEN** a consumer constructs an agent handle (from a registry name or an inline agent spec) and passes it in a config field the component's config type declares as an agent handle
- **THEN** the component drives that role's sessions through the supplied handle and performs no agent construction of its own

#### Scenario: Callable hook rejected by the type gate

- **WHEN** a consumer shim passes a config containing an arbitrary function in a config field and runs `ptah check`
- **THEN** check reports a type error naming the offending field

### Requirement: PR review loop component

The library SHALL provide a PR review loop component that runs a
review→fix→push convergence against a pull request, with repository-specific
settings (agent prompts, reviewer instruction text, dry-run gating)
expressed as config rather than code. The target repository SHALL NOT be
component config: it arrives per call inside the PR URL.

#### Scenario: Review finds fixable findings

- **WHEN** the reviewer reports findings judged resolvable without a human
- **THEN** the component drives a fix session and pushes, iterating until the review passes or escalation occurs

#### Scenario: Repository context is per-call

- **WHEN** the loop reviews a pull request
- **THEN** the repository context comes from the PR URL passed to the operation, and the component's config declares no repository field
