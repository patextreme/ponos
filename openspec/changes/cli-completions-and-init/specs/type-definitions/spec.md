# Type Definitions Spec Delta

## MODIFIED Requirements

### Requirement: Editor setup documentation
The README SHALL document `ptah init` as the front door for obtaining definitions: it writes `.ptah/ptah.d.luau` (byte-identical to `ptah types` output) and a commented `.ptah/config.toml` registry skeleton into `./.ptah/` in the working directory. The README SHALL document `ptah types > .ptah/ptah.d.luau` as the primitive for refreshing definitions after upgrading the binary without re-running init, and the generic luau-lsp settings (VS Code and Neovim, standard platform) pointing at `.ptah/ptah.d.luau`, without the repository committing any editor or Luau configuration files. The documentation SHALL note the known residuals: strict analysis of generic `map` callbacks occasionally needs explicit parameter annotations; the prompt-result string-conversion sugar is not covered; outcome narrowing requires a local binding.

#### Scenario: Reader configures an editor
- **WHEN** a reader follows the README editor-setup section
- **THEN** they can produce a definitions file matching their installed ptah version (via `ptah init`, refreshable via `ptah types > .ptah/ptah.d.luau`) and point luau-lsp at `.ptah/ptah.d.luau` using documented generic settings

#### Scenario: Reader understands the require-tree residual
- **WHEN** a reader encounters the residuals list in the editor-setup section
- **THEN** it contains no require-tree entry; the documentation states that editor analysis and ptah resolve relative requires identically
