## ADDED Requirements

### Requirement: PR review instruction contract

The pr-review-loop component's documentation SHALL declare the contract its
reviewer instruction document must satisfy: a configured document defines
what counts as a blocking issue for the repository and instructs the
reviewer to classify findings as blocking or non-blocking, and the
component's judge predicates and fix prompts speak that classification
vocabulary. The component's config surface (the exported `Config` type's
doc comment for `reviewInstructionFile`) SHALL state the classification
requirement. The documentation SHALL also state the component's boundary:
verdicts that do not reduce to a blocking/non-blocking classification
(score gates, approve/request-changes, report-only reviews) are a different
component, not an instruction swap.

The component SHALL ship a built-in default instruction that satisfies this
contract and SHALL use it when no instruction document is configured; a
configured document SHALL take precedence over the built-in default.

#### Scenario: Instruction contract is declared

- **WHEN** a consumer consults the pr-review-loop component's documentation before supplying a reviewer instruction document
- **THEN** the required blocking/non-blocking verdict classification is stated, along with the boundary that verdicts not reducible to it belong to a different component

#### Scenario: Config surface states the classification requirement

- **WHEN** a consumer reads the exported `Config` type for the pr-review-loop component
- **THEN** the `reviewInstructionFile` field's documentation states that a configured instruction document must classify findings as blocking or non-blocking, and that omitting it selects the built-in default

#### Scenario: Built-in default instruction used when none is configured

- **WHEN** the component is configured without `reviewInstructionFile`
- **THEN** reviews run against the component's built-in default instruction, which directs the reviewer to classify each finding as blocking or non-blocking (the default is the contract's reference instance)

#### Scenario: Configured instruction takes precedence

- **WHEN** `reviewInstructionFile` is configured and points to a readable document
- **THEN** the built-in default is not used and the referenced document's instruction governs the review
