# Proposal: add-check-subcommand

## Why

Authoring a ponos script today means discovering syntax and API-usage mistakes by running
it: `ponos run` compiles and executes in one step, so a typo costs a full run invocation —
and luau-lsp's nonstrict default silently accepts typo'd `ponos.*` members (`agent:sesion`)
unless every file opts into `--!strict`. There is no command that answers "does this script
verify?" without executing it.

## What Changes

- New subcommand `ponos check <script.luau>` that verifies a script with **zero execution**
  (no top-level code runs, no agents spawn):
  - **Compile pass** — in-process (mlua) syntax check; the chunk is compiled but never called.
  - **Static lint pass** — in-process (full-moon AST walk over the entry and its literal
    require graph): unknown literal `ponos.agent("name")` names against the discovered
    registry; literal `require("./x")` targets that do not resolve under ponos's rules
    (`.luau`/`.lua`/`init.luau`, escape-guard) or escape the script tree; missing `--!strict`
    directive in the entry or any file in the literal require graph.
  - **Typecheck pass** — shells out to `luau-lsp analyze` (PATH) with the binary's embedded
    `types/ponos.d.luau` written to a temp file; stderr passes through raw; exit code maps
    onto the check contract. luau-lsp absent from PATH is a hard error, not a silent skip.
  - Findings: all collected (never fail-fast), `path:line:col: message` on stderr, a summary
    line, `--no-color` flag; no JSON mode.
  - Exit codes: `0` clean · `1` findings · `2` check could not run (missing/unreadable
    script, registry discovery failure, luau-lsp missing).
- `ponos run` gains an **in-process pre-flight** (compile + require + agent lints only —
  no luau-lsp, no strictness enforcement) that fails the run before the first agent spawns.
- New dependency: `full-moon` (Luau parser, pure Rust).
- Docs: README section for `check`; exit-code contract note (README + AGENTS.md — `2` for
  `check` also covers "could not run"); the type-definitions editor-setup residual about
  the require-tree restriction being "runtime only" is revised (now also statically checked
  by `ponos check`).

Out of scope (explicitly): dry-run execution mode, in-process typechecker (Luau Analysis
FFI), JSON output, multiple script arguments / directory mode.

## Capabilities

### New Capabilities
- `script-checking`: The `ponos check` subcommand — no-execution verification of a script
  via compile pass, static lints (agent names, require targets, `--!strict` directives),
  and a luau-lsp typecheck pass; findings reporting and exit-code contract.

### Modified Capabilities
- `cli`: `ponos run` gains a pre-flight that fails certain-broken scripts (uncompilable,
  unresolvable literal require, unknown literal agent name) before the first agent spawns;
  exit-code notes extended for `check`.
- `type-definitions`: the editor-setup documentation requirement's enumerated residuals
  change — the require-tree restriction is no longer "enforced at runtime only" (it is now
  also statically enforced by `ponos check`).

## Impact

- `src/cli.rs` — new `check` subcommand (parse, dispatch, exit codes); `run` pre-flight
  wiring.
- New `src/check.rs` (or `src/check/`) — compile pass, full-moon lint walk, luau-lsp
  invocation, findings collection/format.
- `src/script/require.rs` — resolution rules extracted/reused statically (lint must not
  execute modules).
- `src/config.rs` — registry discovery reused by `check` unchanged.
- `Cargo.toml` — add `full-moon`.
- Tests: hermetic integration tests with a stubbed `luau-lsp` on PATH (temp-dir script),
  covering clean/findings/missing-binary; unit tests for lint walk fixtures.
- Docs: README (check section, exit codes, editor-setup residuals), AGENTS.md (exit-code
  contract).
- `ponos-analyze` flake check is unaffected (still runs real luau-lsp over bundled
  examples); `check` must not be added to the offline cargo suite's dependency set.
