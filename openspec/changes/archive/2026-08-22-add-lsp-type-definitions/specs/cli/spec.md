# CLI Spec

## Purpose

Defines the user-facing command surface of the ponos binary: how scripts are invoked, how output is controlled, and the process exits.

## MODIFIED Requirements

### Requirement: Run subcommand executes a script
The `ponos` CLI SHALL provide `ponos run <script.luau>`, where `<script.luau>` is a positional required path to the entry Luau script.

#### Scenario: Successful run
- **WHEN** `ponos run script.luau` is invoked and the script completes without uncaught errors
- **THEN** the process exits with code 0

#### Scenario: Missing script argument
- **WHEN** `ponos run` is invoked without a positional path
- **THEN** the CLI prints a usage error and exits non-zero without executing anything

#### Scenario: Nonexistent script file
- **WHEN** the positional path does not exist on disk
- **THEN** the CLI prints an error naming the path and exits non-zero
