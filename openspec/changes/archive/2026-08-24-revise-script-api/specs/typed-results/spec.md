## MODIFIED Requirements

### Requirement: Result contract declaration
`agent:session(options)` SHALL accept a `resultSchema` option whose value is a JSON Schema expressed as a Luau table. The schema SHALL be compiled at session-creation time; a schema that fails to compile SHALL raise a Lua error at the `session()` call site naming the compile failure. Schemas containing a remote `$ref` (a reference that is not a local JSON pointer within the same document) SHALL be rejected at the same point, so runs stay offline. Sessions created without `resultSchema` SHALL behave exactly as before, with no injected tool and no prompt augmentation. A `result` option SHALL NOT be read: a script passing the former name declares no contract, and the session behaves as a plain session.

#### Scenario: Valid schema accepted
- **WHEN** `session({ resultSchema = { type = "object", properties = { verdict = { type = "string" } }, required = { "verdict" } } })` is called
- **THEN** the session is created and the schema governs all prompts on it

#### Scenario: Invalid schema fails at the author's line
- **WHEN** `session({ resultSchema = { type = "objekt" } })` is called
- **THEN** a Lua error is raised from the `session()` call naming the schema problem

#### Scenario: Remote reference rejected
- **WHEN** the schema contains `$ref: "https://example.com/schema.json"`
- **THEN** a Lua error is raised from the `session()` call, before any agent subprocess is spawned

#### Scenario: Any root schema shape
- **WHEN** `resultSchema` is a non-object schema such as `{ type = "string", enum = { "ship", "block" } }`
- **THEN** the session accepts it and submissions are strings, not wrapped objects

#### Scenario: Legacy option name is not read
- **WHEN** `session({ result = { type = "object" } })` is called
- **THEN** the session is created as a plain session: no contract, no injected tool, and prompt outcomes carry a `nil` `result` field
