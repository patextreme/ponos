# Type Definitions Spec Delta

## MODIFIED Requirements

### Requirement: Editor setup documentation
The README SHALL document how to obtain definitions (`ponos types`) and the generic luau-lsp settings (VS Code and Neovim, standard platform) that load them, without the repository committing any editor or Luau configuration files. The documentation SHALL note the known residuals: strict analysis of generic `map` callbacks occasionally needs explicit parameter annotations; the prompt-result string-conversion sugar is not covered; outcome narrowing requires a local binding; the require-tree restriction is not enforced by editor analysis (luau-lsp resolves requires without ponos's escape-guard), though the runtime and `ponos check` both enforce it — the runtime at require time, `ponos check` statically before any run.

#### Scenario: Reader configures an editor
- **WHEN** a reader follows the README editor-setup section
- **THEN** they can produce a definitions file matching their installed ponos version and point luau-lsp at it using documented generic settings

#### Scenario: Reader understands the require-tree residual
- **WHEN** a reader encounters the residuals list in the editor-setup section
- **THEN** the require-tree entry states editor analysis does not enforce ponos's escape-guard and names `ponos check` as the static enforcement point
