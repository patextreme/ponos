# Design: add-check-subcommand

## Context

See proposal.md for motivation. Today `ponos run` compiles and executes in one step
(`src/script/mod.rs::run`); the only static-analysis surface is editor-side luau-lsp
(`ponos types` + `.helix/languages.toml`) and the offline `ponos-analyze` flake check
(`nix/checks.nix`), which runs real luau-lsp over bundled examples. luau-lsp's analyzer
is not reachable from Rust: mlua exposes no parse/analysis API, so an in-process
typechecker is not an option (ruled out in the grilling session — Luau Analysis FFI is
weeks of unsafe work). mlua **can** compile-without-calling (`Chunk::into_function`),
and `full-moon` provides a pure-Rust Luau AST for the lint walk. Verified against
luau-lsp 1.67.0: analysis resolves file-relative requires natively, reports
`file(line,col): CodeName: message` on stderr, exits non-zero only on errors (warnings
like `LocalUnused` exit 0), and offers no CLI/luaurc strictness override — strict mode
comes only from per-file `--!strict` directives, which motivates the directive lint.

## Goals / Non-Goals

Goals: zero-execution verification; exit codes that separate findings (1) from
tool-unavailable (2); hermetic tests (no network, no real luau-lsp in the cargo suite);
`run` pre-flight sharing the lint code without changing run's permissive contract.

Non-Goals (beyond proposal scope): JSON output; multi-script/directory modes; dry-run;
any caching of analysis results; making `run` depend on luau-lsp.

## Decisions

### D1: Module layout — new `src/check.rs` with the lint walk in `src/check/lint.rs`

`src/check.rs` holds the pass pipeline (`check(cfg) -> CheckOutcome`), the luau-lsp
invocation, and findings; `src/check/lint.rs` holds the full-moon walk (require-graph
discovery, agent-name extraction, strict-directive detection). Alternative: fold into
`src/script/`. Rejected — checking shares *resolution rules* with the scripting runtime
but not its execution machinery; a separate module keeps the no-execution guarantee
structurally obvious (nothing in `src/check*` may call `lua.load(...).eval*`).

### D2: Compile pass via mlua `into_function`, not full-moon

The entry file is compiled through a fresh sandboxed `Lua` (`Lua::load(...).into_function()`)
so syntax diagnostics come from the *execution* compiler, matching what `run` would report.
full-moon parses every walked file anyway, so module-level syntax errors surface as
full-moon parse findings (`path:line:col: message`). One compiled-but-never-called chunk
is the strongest cheap guarantee the compile pass can give.

### D3: Reuse require resolution statically — extract the resolver, don't re-derive

`src/script/require.rs` already implements candidate resolution (`.luau`, `.lua`,
`init.luau`, `init.lua`) and the escape guard as free logic inside the `Require` impl.
Extract the pure path logic (`resolve_candidates(from_dir, module_path)`,
`escapes(root, path)`) into `require.rs` functions callable without a `Lua`; the runtime
impl and the lint both call them. Alternative: duplicate the rules in check. Rejected —
the rules drifting between check and run is the worst failure mode for this feature.

### D4: Lint walk — full-moon AST, literal-only, breadth/depth-first with visited-set

Walk each file's AST for two call shapes: `require(<string literal>)` and
`ponos.agent(<string literal>)` where the callee is *literally* the global member access
(`ponos.agent`). Store calls, aliased references (`local a = ponos.agent`), and
non-string arguments are **not** linted — no false positives (settled in grilling).
`require` edges recurse from the requiring file's directory; a visited-set keyed on the
canonicalized resolved path prevents cycles. `--!strict` detection: the first comment
token in the file (full-moon preserves comments) is `--!strict` — matching how luau-lsp
reads the hot-comment.

### D5: Registry discovery — exact reuse, failure maps to exit 2

`check` calls `Registry::discover(&invocation_dir)` with the same semantics as `run`.
Discovery failure (malformed TOML) is "could not run" → exit 2, not a finding. The
agent-name lint resolves each literal name against the merged registry; `${VAR}`
interpolation need not be evaluated — name *presence* in the merged map is the test
(the runtime interpolates values, not names).

### D6: luau-lsp invocation — PATH lookup, embedded definitions to temp file, raw passthrough

`luau-lsp analyze --platform=standard --definitions=<tmp> <entry>` with stderr piped to
our stderr verbatim (settled: no parsing, no filtering — robust to format drift; its own
exit code already encodes errors-only). The definitions come from the existing
`TYPE_DEFINITIONS` (`include_str!("../types/ponos.d.luau")`) written to a
`std::env::temp_dir()` file, so the check always typechecks against the installed
binary's API version. Missing binary: a `which`-style PATH scan (`env::split_paths`) in
a temp dir listing, not `Command::spawn` failure — gives the clean "install luau-lsp"
error → exit 2. Invoke luau-lsp only after in-process passes are clean? **No — run all
passes regardless** (findings are collected, never fail-fast), but still invoke it even
when in-process findings exist, so the author sees everything in one pass.

### D7: `run` pre-flight — same lint entry point, narrower set, exit 1

`cli.rs` runs compile + require + agent lints (the `check` code minus strictness and
luau-lsp) after script-file existence check and registry discovery, before `setup_lua`.
Findings print to stderr and the run exits 1 (script-error class, per the AGENTS.md
exit-code contract — `2` stays usage). No `--no-strict-check` escape hatch: the accepted
false-positive class (missing require on a never-executed path) is documented in the
README instead.

### D8: Tests — stub `luau-lsp` on PATH, mock-agent philosophy

Integration tests in `tests/check.rs` drive the real `ponos` binary (`env!(
"CARGO_BIN_EXE_ponos")`). The luau-lsp stub is an executable shell script in a temp dir
prepended to the child's `PATH`: variant A exits 0 silently (clean), variant B prints a
canned `file(1,1): TypeError: ...` line to stderr and exits 1 (findings), and a
PATH-less variant (empty temp dir) exercises the missing-binary error. In-process lint
findings get fixture scripts under `tests/fixtures/` (some shared with examples tests).
The flake's `ponos-analyze` check is untouched; no cargo test ever requires real
luau-lsp, keeping `cargo test` hermetic outside Nix.

## Risks / Trade-offs

- [luau-lsp diagnostic format drift changes what users see] → raw passthrough (D6) means
  we never parse it; only its exit code is load-bearing.
- [full-moon vs Luau grammar drift (new syntax)] → full-moon is the parser luau-lsp
  builds on and tracks Luau closely; a parse failure is itself reported as a finding.
- [Pre-flight false positive: missing require on a dead code path] → documented,
  accepted (settled); scripts are small and deliberate; fix is deleting the dead require.
- [Temp definitions file leaks or collides] → unique temp filename per invocation
  (`ponos-defs-<pid>-<nanos>.luau`), best-effort deletion; content is not secret.
- [Embedded defs vs luau-lsp `@roblox` naming quirk (`--definitions=NAME=PATH`)] → the
  flake check already uses plain `--definitions=<path>` successfully; keep that form.
- [Windows PATH/`.cmd` shims] → out of scope; PATH scan checks executable files and
  common extension set only where the platform requires it.

## Migration Plan

Additive: new subcommand, one new dependency, pre-flight newly fails only
certain-broken scripts. No config or script changes for users. Rollback = revert the
commit; `run` behaves as before.

## Open Questions

- None blocking. (Exact summary-line wording and finding colors are polish, decided in
  review.)
