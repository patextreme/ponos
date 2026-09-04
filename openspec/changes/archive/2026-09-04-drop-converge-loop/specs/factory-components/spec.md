## REMOVED Requirements

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

## MODIFIED Requirements

### Requirement: openspec component

The library SHALL provide an openspec component whose instance exposes
groom, implement, and verify operations on a named change: groom converges a
change's proposals through review, implement drives task execution, and
verify converges verification then syncs and archives the change. The
component SHALL declare its environment requirements (an agent carrying the
openspec skills, `openspec` on PATH) in its documentation rather than
bundling or installing them. Convergence is the component's own loop over
the library's typed judge: a judge-rejected pass probes for human input,
a needed human fails the operation without issuing a fix, and exhausting
the iteration cap fails the operation with an error reporting the cap.

#### Scenario: Verify converges and archives

- **WHEN** verify runs on a change whose implementation passes the verification judge
- **THEN** the verification loop exits and the sync-and-archive step runs as part of the same operation

#### Scenario: Missing environment requirement

- **WHEN** the component's documentation is consulted for its environment requirements
- **THEN** the agent-skill and CLI requirements are listed so a consumer can verify them before running

#### Scenario: Human escalation

- **WHEN** a groom pass is judge-rejected and the escalation judge confirms human input is required
- **THEN** the operation fails with an error stating human input is needed, and no fix prompt is issued

#### Scenario: Iteration cap

- **WHEN** every pass is judge-rejected and the findings stay fixable up to the configured iteration cap
- **THEN** the operation fails with an error reporting the cap was reached
