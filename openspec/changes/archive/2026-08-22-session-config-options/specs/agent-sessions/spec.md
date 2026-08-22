## RENAMED Requirements

- FROM: `### Requirement: No client capabilities are exposed`
- TO: `### Requirement: No interactive client capabilities are exposed`

## MODIFIED Requirements

### Requirement: No interactive client capabilities are exposed
ponos SHALL declare exactly one client capability during initialization: `session.configOptions` (a non-interactive capability; it commits ponos to nothing interactive). ponos runs headless, so `session/request_permission` requests SHALL be answered, on every session, for the whole session lifetime, by selecting an allow option the agent offered: the first `AllowAlways` option when one is offered, otherwise the first allow-kind option of any other kind (e.g. `AllowOnce`). When the offer contains no allow-kind option at all, ponos SHALL respond with a JSON-RPC error indicating the method is unsupported. All other agent-to-client requests ponos has declared no support for — `fs/read_text_file`, `fs/write_text_file`, `terminal/*`, `elicitation/create` — SHALL be answered with a JSON-RPC error indicating the method is unsupported, and MUST NOT block the turn indefinitely. Selecting `AllowAlways` MAY cause the agent to persist an allow rule in its own configuration beyond the ponos run. `config_option_update` is a notification (no reply exists to deny) and SHALL be consumed, not ignored.

#### Scenario: Permission request allowed
- **WHEN** an agent calls `session/request_permission` offering allow options
- **THEN** ponos responds with the first `AllowAlways` option's id and the turn continues

#### Scenario: Permission request without allow-always
- **WHEN** an agent calls `session/request_permission` offering only an `AllowOnce` option
- **THEN** ponos responds with that option's id and the turn continues

#### Scenario: Permission request denied
- **WHEN** an agent calls `session/request_permission` offering only reject options, or no options
- **THEN** ponos responds with an unsupported-method error and the turn continues

#### Scenario: Unsupported requests still rejected
- **WHEN** an agent issues `fs/read_text_file`, `fs/write_text_file`, `terminal/*`, or `elicitation/create`
- **THEN** ponos responds with an unsupported-method error

#### Scenario: No hanging turns
- **WHEN** an agent issues any agent-to-client request mid-turn
- **THEN** ponos replies promptly and the turn continues toward completion

#### Scenario: Config option updates are consumed
- **WHEN** an agent sends a `session/update` carrying a `config_option_update`
- **THEN** ponos folds the update into that session's option state (no reply exists for notifications)
