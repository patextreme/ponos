## MODIFIED Requirements

### Requirement: Submit tool injection
When `result` is set, session creation SHALL offer the agent one additional MCP server over stdio, named `ponos`, exposing exactly one tool named `result_submit`. The tool's input schema SHALL be the declared schema wrapped under a single `value` property, so the declared schema reaches the model through the tool itself. The tool's description SHALL tell the agent to call it with its final result as `value` when its work is complete. Prompt text on a result session SHALL be passed through verbatim: ponos SHALL NOT append instructions to, or otherwise modify, the script's prompt. The schema SHALL NOT be inlined into prompt text. The injected server SHALL NOT change sessions that declare no `result`.

#### Scenario: Tool appears with wrapped schema
- **WHEN** a result session's agent lists tools from the injected server
- **THEN** exactly one tool `result_submit` is listed, whose input schema is `{ value: <declared schema> }`

#### Scenario: Tool description carries the submit guidance
- **WHEN** a result session's agent lists tools from the injected server
- **THEN** the `result_submit` tool's description instructs the agent to call it when its work is complete, with the final result as the `value` argument

#### Scenario: Prompt carries the instruction
- **WHEN** a prompt is sent on a result session
- **THEN** the text the agent receives is identical to the prompt text the script passed, carrying no ponos-appended instruction or suffix
