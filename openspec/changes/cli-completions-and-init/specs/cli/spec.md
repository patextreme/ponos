# CLI Spec Delta

## ADDED Requirements

### Requirement: Completions subcommand emits shell completion scripts
The CLI SHALL provide `ptah completions <shell>`, where `<shell>` is a required positional argument accepting exactly `bash`, `zsh`, `fish`, `elvish`, and `powershell`. The command SHALL print the completion script for the named shell to standard output and SHALL print nothing else. Emitted scripts SHALL be generated from the binary's own command tree, so completions always match the installed binary's surface; hidden subcommands SHALL NOT appear in the emitted scripts. A missing or unknown `<shell>` argument SHALL be a usage error printing to standard error and exiting 2. The command SHALL NOT require a script, registry, or agent configuration and SHALL NOT touch the filesystem. The README SHALL document per-shell installation lines covering at least bash, zsh, and fish.

#### Scenario: Emit a bash completion script
- **WHEN** `ptah completions bash` is invoked
- **THEN** standard output contains a bash completion script registering a completion handler for `ptah`, and the process exits 0

#### Scenario: Every supported shell emits a plausible script
- **WHEN** `ptah completions <shell>` is invoked for each of `zsh`, `fish`, `elvish`, and `powershell`
- **THEN** each invocation prints a non-empty, shell-appropriate script to standard output and exits 0

#### Scenario: Visible subcommands appear, hidden ones do not
- **WHEN** a completion script is generated for any shell
- **THEN** the visible subcommands (`run`, `check`, `types`, `completions`, `init`) appear in it and the hidden `__bridge` subcommand does not

#### Scenario: Unknown shell is a usage error
- **WHEN** `ptah completions tcsh` is invoked
- **THEN** a usage error is printed to standard error and the process exits 2

#### Scenario: No side effects
- **WHEN** `ptah completions fish` runs on a machine with no agent registry and no `.ptah` directory
- **THEN** it exits 0 without creating files or spawning agents

### Requirement: Init subcommand scaffolds a project .ptah directory
The CLI SHALL provide `ptah init`, which scaffolds `./.ptah/` relative to the current working directory with exactly two files:

- `.ptah/ptah.d.luau` — byte-identical to `ptah types` standard output (version header included);
- `.ptah/config.toml` — a fully commented skeleton documenting the two-layer registry discovery (project entries override user entries per agent name), `${VAR}` environment interpolation, and the per-agent fields (`command` required, `args` and `env` optional), which SHALL parse as a valid empty registry exactly as written.

`ptah init` SHALL NOT create any other files (no starter script, no editor or Luau configuration), SHALL NOT search parent directories for an existing `.ptah`, and SHALL NOT write to the user-level config directory. A file that already exists SHALL be skipped with a per-file skipped message while the remaining files are still created; running `ptah init` twice SHALL leave previously existing files byte-identical. On success the command SHALL print one line per file created or skipped followed by next-step hints (editing the registry, pointing luau-lsp at the definitions, installing shell completions, the ptah skill) to standard output, and exit 0 — the hints SHALL print on every run, including runs where files were skipped. A failure to write (for example an unwritable directory) SHALL print an error to standard error and exit 1.

#### Scenario: Fresh init creates both files
- **WHEN** `ptah init` runs in a directory with no `.ptah`
- **THEN** `.ptah/ptah.d.luau` and `.ptah/config.toml` exist, the process exits 0, and each created file is announced on standard output

#### Scenario: Written definitions match the installed binary
- **WHEN** `.ptah/ptah.d.luau` written by `ptah init` is compared with `ptah types` output
- **THEN** they are byte-identical

#### Scenario: Skeleton is a valid empty registry
- **WHEN** `.ptah/config.toml` written by `ptah init` is parsed as a registry
- **THEN** it parses without error and contains no agents

#### Scenario: Re-running init is idempotent
- **WHEN** `ptah init` runs a second time in the same directory
- **THEN** each file is reported as skipped (exists), previously written files are byte-identical to before, and the process exits 0

#### Scenario: Partial scaffold completes
- **WHEN** `ptah init` runs in a directory where `.ptah/config.toml` already exists but `.ptah/ptah.d.luau` does not
- **THEN** the definitions file is created, the existing config is neither modified nor clobbered, and the process exits 0

#### Scenario: Hints print on every run
- **WHEN** `ptah init` completes, whether files were created or skipped
- **THEN** the next-step hints appear on standard output

#### Scenario: Unwritable target fails cleanly
- **WHEN** `ptah init` cannot create `./.ptah` (for example a read-only parent directory)
- **THEN** an error is printed to standard error and the process exits 1
