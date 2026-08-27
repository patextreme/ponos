## MODIFIED Requirements

### Requirement: Sandboxed Luau environment
Scripts SHALL execute in a sandboxed Luau environment exposing only: `string`, `table`, `math`, `utf8`, `bit32`, `buffer`, `os.time`, `os.clock`, `print`, and a restricted `coroutine` table containing only `yield` (retained because the embedded async runtime requires it; all coroutine scheduling primitives MUST remain absent). The ambient environment MUST NOT expose file I/O, network, or debug facilities, and MUST NOT expose subprocess execution as a global (no `os.execute`, no `io`). Process execution reaches scripts only through the injected `ponos.exec` capability, specified by the `shell-exec` capability; scripts have no other host filesystem or network access beyond driving agents and `ponos.exec`.

#### Scenario: Sandboxed globals
- **WHEN** a script accesses `io`, `os.execute`, `debug`, or `coroutine.create`
- **THEN** the access resolves to nil (or raises an error on call) because the globals are absent

#### Scenario: Print passthrough
- **WHEN** a script calls `print("hello")`
- **THEN** the line is written to ponos's standard output unmodified, without session prefixes

## ADDED Requirements

### Requirement: JSON module
The `ponos` namespace SHALL provide `ponos.json.parse(string)` returning the decoded value as Luau data (arrays as tables with consecutive integer keys starting at 1, objects as string-keyed tables, `null` as `nil`), raising a Lua error on malformed input; and `ponos.json.stringify(value, { indent?: number })` returning the encoded JSON string. The module performs no I/O.

#### Scenario: Round trip
- **WHEN** a script calls `ponos.json.parse('{"a":[1,2]}')` and stringifies the result with `ponos.json.stringify(v, { indent = 2 })`
- **THEN** the output is valid JSON encoding `{"a": [1, 2]}` with two-space indentation

#### Scenario: Malformed input raises
- **WHEN** a script calls `ponos.json.parse("{oops")`
- **THEN** a Lua error is raised and can be caught with `pcall`

#### Scenario: Decoding command output
- **WHEN** a script calls `ponos.exec("printf '[{\\"n\\":1}]'")` and passes `r.stdout` to `ponos.json.parse`
- **THEN** the result is a table whose first element is `{ n = 1 }`
