# Agent Registry Specification

## Purpose

Defines how agent launch specifications are configured and resolved: TOML registry files, project/user precedence, environment variable interpolation, and inline overrides from scripts.

## Requirements

### Requirement: TOML agent registry
Agent definitions SHALL be configured in TOML: a project-level `.ponos/config.toml` and a user-level `~/.config/ponos/config.toml`. Each agent entry SHALL define `command` (program path), and MAY define `args` (argument list) and `env` (string-to-string map merged over the inherited environment). Project entries SHALL override user entries for the same agent name; the two files otherwise merge.

#### Scenario: Project overrides user
- **WHEN** agent `claude` is defined in both user and project config
- **THEN** the project definition wins and the user definition's fields are not inherited

#### Scenario: Registry merge
- **WHEN** user config defines `claude` and project config defines `gemini`
- **THEN** both agents are resolvable by name

#### Scenario: No registry found
- **WHEN** neither config file exists and a script calls `ponos.agent("claude")`
- **THEN** a Lua error is raised naming the unresolved agent

### Requirement: Environment variable interpolation
String values in agent registry entries SHALL support `${VAR}` interpolation from ponos's environment at resolve time. Unset variables SHALL expand to the empty string.

#### Scenario: Store-path command
- **WHEN** `command = "${HOME}/.local/bin/claude-acp"` and `HOME=/home/pat`
- **THEN** the resolved command is `/home/pat/.local/bin/claude-acp`

#### Scenario: Unset variable
- **WHEN** `args = ["--key=${MISSING_KEY}"]` and `MISSING_KEY` is unset
- **THEN** the argument resolves to `--key=`

### Requirement: Inline spec override
`ponos.agent(...)` SHALL accept either a registry name (string) or an inline spec table (`{ command = ..., args = ..., env = ... }`) that bypasses the registry entirely.

#### Scenario: Inline spec
- **WHEN** a script calls `ponos.agent({ command = "npx", args = {"-y", "@agentclientprotocol/codex-acp"} })`
- **THEN** the resulting sessions spawn that command directly, with no registry lookup

### Requirement: Agent environment inheritance
Agent subprocesses SHALL inherit ponos's environment with the entry's `env` values merged on top; `env` values SHALL also undergo `${VAR}` interpolation.

#### Scenario: Env merge
- **WHEN** an entry sets `env = { ANTHROPIC_MODEL = "${MODEL}" }` with `MODEL=opus`
- **THEN** the spawned agent sees `ANTHROPIC_MODEL=opus` in addition to the inherited environment
