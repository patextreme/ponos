# Design: LSP type definitions for ponos scripts

## Context

See `proposal.md` — Why. The runtime injects the `ponos` global from Rust
(`src/script/mod.rs`, `bind_ponos`) and curates the stdlib (`os` trimmed to
`time`/`clock`, `coroutine` to `yield`, `loadstring`/`collectgarbage`
poisoned). No file an LSP can read describes any of this. All load-bearing
mechanics below were verified against luau-lsp 1.67.0 (`--platform=standard
--definitions=...`): `declare` globals resolve, stdlib shadowing works
(`Key 'date' not found` for `os.date`), `declare loadstring: nil` yields
"Cannot call a value of type nil", and `.luaurc` has **no** `definitions` key
(only `languageMode`/`aliases`/`globals`), so definitions must be supplied via
editor settings or the analyze CLI flag.

## Goals / Non-Goals

Goals: full editor typing for the script API; sandbox violations surfaced at
edit time; definitions version-locked to the binary; drift made loud.

Non-Goals: changing the script API itself (no require-alias restructure);
`ponos init` scaffolding; committing `.luaurc`/`.vscode/`; GitHub Actions;
codegen of definitions from Rust registration code; runtime error-message/DX
changes.

## Decisions

**D1 — `declare`-global definitions file, not an API change.** `types/ponos.d.luau`
(`@meta`, `--!strict`) declares `ponos` as a global, loaded via
`--definitions` / editor setting. Alternative: `local ponos = require("@ponos")`
resolved through `.luaurc` aliases + a special case in `ScriptRequirer` — more
conventional module feel but a breaking API change across runtime, examples,
and docs for zero functional gain. A `.luaurc` `globals: ["ponos"]` fallback
was rejected: it types everything `any`, which is ten minutes of work for no
value.

**D2 — The definitions model the sandbox subtractively.** Include
`declare os: { time, clock }`, `declare coroutine: { yield }`,
`declare loadstring: nil`, `declare collectgarbage: nil`. Verified: the editor
then flags exactly what the runtime poisons. Residual risk: definitions are
workspace-global in the LSP, so a mixed Luau workspace gets the trimmed stdlib
in non-ponos files too. Accepted (ponos scripts are typically isolated trees);
fallback if it bites is splitting `ponos-sandbox.d.luau` from `ponos.d.luau`.

**D3 — Distribution is `ponos types` → stdout.** The repo file is the single
source of truth; the binary embeds it (`include_str!`) and `ponos types` prints
it with a generated `-- ponos <version> type definitions` header, so users'
emitted defs always match their installed binary (Lune's model). stdout over
file-writing: no clobber/path semantics to design, Unix-composable. No `ponos
init` until someone asks.

**D4 — Drift guards: probe test + analyze check, not codegen.**
(a) A cargo test runs a mock-agent script touching *every* member/method/field
the defs promise — this is the guard that catches semantic drift (analyze
checks scripts against defs; both can be wrong together). (b) A nix check
`checks.ponos-analyze` runs `luau-lsp analyze --platform=standard
--definitions=types/ponos.d.luau` over `examples/*.luau`, the probe script,
and test fixtures, on the pinned nixpkgs luau-lsp (1.67.0). Matches the
existing `nix/checks.nix` pattern; GitHub Actions deliberately out of scope.
Codegen rejected at this API size (~8 functions, 3 classes). Note the trap this
change already dodged once: `label` is a *method* at runtime
(`s:label()`), easy to mis-declare as a property — the probe test makes that
class of mistake a build failure.

**D5 — No committed editor config; generic docs only.** No `.vscode/`, no
`.luaurc` anywhere in the repo — editor configuration belongs to each
machine. README documents the generic luau-lsp settings (VS Code
`luau-lsp.types.definitionFiles` + platform `standard`; Neovim
nvim-lspconfig equivalent) plus `ponos types`.

**D6 — Strictness rides per-file `--!strict` headers.** Examples get copied
by users out of the repo, away from any `.luaurc`; a header survives the copy.
Consequence: contributors without editor setup see `UnknownGlobal 'ponos'`
warnings in-repo — accepted price of D5, fixed by reading the README.

**D7 — Full typing depth.** `Outcome<T>` discriminated unions, generics on
`map`/`spawn`, typed option tables including `mcp_servers` derived from the
ACP structs (`acp::SessionOptions`), `Task.await` untyped-multivalue honestly
left loose where Luau cannot express it. Documented residuals: generic
`map` callbacks occasionally need `function(x: number)` annotations;
`tostring(r)` on prompt results (via `__tostring`) is not typeable in defs;
narrowing works on local bindings.

## Risks / Trade-offs

- [Defs drift from runtime semantics] → probe test (D4a) fails the build;
  version header (D3) makes stale user-side defs identifiable.
- [Editor luau-lsp ≠ pinned analyze version] → defs stick to stable
  declaration syntax; the gate pins one known-good analyzer.
- [Mixed-Luau workspaces hit trimmed stdlib] → documented; split-file fallback
  available (D2).
- [Strict-mode churn in examples] → small, in-scope annotation tweaks; the
  analyze check (D4b) enforces it permanently.

## Migration Plan

Additive; no runtime behavior changes. Rollback = delete `types/`,
`ponos types`, the check, and the README section.
