## Why

Script authors get zero editor support: the `ponos` global is injected by the
Rust host at runtime, so no LSP can see it — no completion, no hover, no types,
and `UnknownGlobal` warnings on every line. Worse, the sandbox *removes*
globals (`os.date`, `coroutine.create`, `loadstring`, …) that a stock Luau LSP
still happily suggests, so editor-approved code can fail at runtime with a
poison-global error. Verified locally: a definition file declaring the `ponos`
API and shadowing the trimmed stdlib fixes both directions — completion/types
appear, and sandbox violations become type errors at edit time.

## What Changes

- Add a hand-written Luau definition file (`types/ponos.d.luau`) that models the
  entire `ponos` API surface: `ponos.agent/spawn/map/join/sleep/log/exit`,
  session/task objects, prompt result and usage tables, option tables
  (including a typed `mcp_servers` shape derived from the ACP structs), generic
  `Outcome<T>` discriminated unions, and `label` as a method (matching runtime).
- The definitions also model the sandbox: `os` trimmed to `time`/`clock`,
  `coroutine` trimmed to `yield`, `loadstring`/`collectgarbage` declared nil —
  so removed globals are flagged in the editor, not discovered at runtime.
- Add a `ponos types` subcommand that prints the definitions to stdout
  (byte-exact `include_str!` of the repo file, prefixed with a generated
  version header), so users get defs version-matched to their installed binary:
  `ponos types > ponos.d.luau`.
- Add drift guards: a runtime probe test (mock-agent script exercising every
  member the definitions promise) and a `nix flake check` analyze gate
  (`luau-lsp analyze --platform=standard --definitions=types/ponos.d.luau` over
  examples, the probe script, and test fixtures); `luau-lsp` joins the devshell.
- Add a README "Editor setup" section documenting generic luau-lsp settings
  (VS Code and Neovim wording, no committed editor config), `ponos types`
  usage, and known residuals.
- Strictness is carried per-file: `--!strict` headers on `examples/*.luau` and
  the probe script; no `.luaurc` and no `.vscode/` are committed anywhere.

## Capabilities

### New Capabilities
- `type-definitions`: Luau type definitions for the `ponos` API and sandboxed
  environment — their content contract, distribution via `ponos types`, and the
  sync guards keeping them honest against the runtime.

### Modified Capabilities

(none — `openspec/specs/` is empty; the CLI change rides within the new
capability rather than modifying un-archived delta specs)

## Impact

- **New**: `types/ponos.d.luau`, `ponos types` subcommand (src/cli.rs, main),
  probe test script + cargo test, `checks.ponos-analyze` in `nix/checks.nix`,
  devshell addition (`luau-lsp`), README section.
- **Modified**: `examples/*.luau` gain `--!strict` headers (may need minor
  annotations to pass strict analysis).
- **Unchanged**: the script runtime API itself — no script that runs today
  changes behavior; this change only adds editor/authoring support around it.
- **Known residuals** (documented, accepted): contributors without editor
  setup see `UnknownGlobal 'ponos'` warnings in-repo; generic callbacks in
  `ponos.map` occasionally need explicit parameter annotations; the
  `tostring(r)` prompt-result sugar is not typeable in definitions;
  outcome narrowing works on locals; the require tree guard is runtime-only.
