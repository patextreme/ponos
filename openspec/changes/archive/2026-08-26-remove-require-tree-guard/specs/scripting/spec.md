## MODIFIED Requirements

### Requirement: Relative module resolution
Scripts SHALL be able to `require` modules by relative path from the requiring file's directory (e.g. `require("./lib/pipeline")`, resolving `.luau` files). Relative paths resolve without a boundary: a require MAY traverse out of the entry script's directory (e.g. `require("../shared/helper")`) to any module reachable by relative path. Non-relative require strings (absolute paths, bare module names, aliases) MUST be rejected with a Lua error.

#### Scenario: Sibling module
- **WHEN** a script at `main.luau` requires `./lib/util` and `lib/util.luau` exists
- **THEN** the module is loaded and its return value provided; a second require of the same path returns the cached module

#### Scenario: Module outside the entry script's directory
- **WHEN** a script at `workflow-1/main.luau` requires `../shared/helper` and `shared/helper.luau` exists as a sibling of `workflow-1/`
- **THEN** the module is loaded and its return value provided exactly as an in-directory module would be

#### Scenario: Missing module
- **WHEN** a script requires a path that does not resolve to an existing `.luau` file
- **THEN** the require call raises a Lua error naming the unresolved path

#### Scenario: Non-relative require string rejected
- **WHEN** a script requires an absolute path (`require("/etc/x")`) or a bare module name (`require("shared/helper")`)
- **THEN** the require call raises a Lua error stating that only `./` and `../` paths are allowed
