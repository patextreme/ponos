## MODIFIED Requirements

### Requirement: No client capabilities are exposed
ponos SHALL declare no client capabilities during initialization. ponos runs headless, so `session/request_permission` requests SHALL be answered by selecting the first `AllowAlways` option the agent offered, on every session, for the whole session lifetime. All other agent-to-client requests ponos has declared no support for — `fs/read_text_file`, `fs/write_text_file`, `terminal/*`, `elicitation/create` — SHALL be answered with a JSON-RPC error indicating the method is unsupported, and MUST NOT block the turn indefinitely. Selecting `AllowAlways` MAY cause the agent to persist an allow rule in its own configuration beyond the ponos run.

#### Scenario: Permission request allowed
- **WHEN** an agent calls `session/request_permission` offering allow options
- **THEN** ponos responds with the first `AllowAlways` option's id and the turn continues

#### Scenario: Permission request denied
- **WHEN** an agent calls `session/request_permission` offering only reject options, or no options
- **THEN** ponos responds with an unsupported-method error and the turn continues

#### Scenario: Unsupported requests still rejected
- **WHEN** an agent issues `fs/read_text_file`, `fs/write_text_file`, `terminal/*`, or `elicitation/create`
- **THEN** ponos responds with an unsupported-method error

#### Scenario: No hanging turns
- **WHEN** an agent issues any agent-to-client request mid-turn
- **THEN** ponos replies promptly and the turn continues toward completion
