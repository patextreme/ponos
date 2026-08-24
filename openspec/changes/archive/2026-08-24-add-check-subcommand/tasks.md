# Tasks: add-check-subcommand

## 1. Foundation

- [x] 1.1 Add `full-moon` to `Cargo.toml` (workspace pin consistent with mlua's Luau
  version); verify `cargo build` succeeds offline inside the dev shell.
- [x] 1.2 Extract pure path-resolution helpers in `src/script/require.rs`
  (candidate resolution `.luau`/`.lua`/`init.luau`/`init.lua`, script-tree escape guard)
  callable without a `Lua` instance; verify existing `cargo test --test require` (or the
  require tests' current home) still passes unchanged.

## 2. Lint walk (`src/check/lint.rs`)

- [x] 2.1 Implement the full-moon walk: parse a file, collect literal
  `require("...")` call sites and literal `ponos.agent("...")` call sites, detect a
  leading `--!strict` hot-comment; verify with unit tests over fixture files (literal
  forms, computed/aliased forms ignored, commented-out calls ignored).
- [x] 2.2 Implement require-graph traversal: BFS/DFS from the entry over literal require
  edges with a visited-set on canonicalized paths, resolving via the 1.2 helpers;
  report broken/escaping requires and missing `--!strict` per file; verify unit tests
  cover cycle-free traversal, escape guard, and missing targets.
- [x] 2.3 Implement the agent-name lint against a discovered `Registry`; verify unit
  tests: unknown literal name flagged, known name clean, computed argument not flagged.

## 3. Check pipeline (`src/check.rs`)

- [x] 3.1 Implement the findings type (`path:line:col: message`, severity, summary
  count) and the compile pass (fresh sandboxed `Lua`, `Chunk::into_function`, never
  called); verify a syntax-error fixture produces a positioned finding.
- [x] 3.2 Implement luau-lsp invocation: PATH scan for the binary (exit 2 with
  "install luau-lsp" message when absent), write embedded `TYPE_DEFINITIONS` to a unique
  temp file, spawn `luau-lsp analyze --platform=standard --definitions=<tmp> <entry>`
  with raw stderr passthrough, map exit status; verify by hand against real luau-lsp
  (dev shell) — automated coverage comes in 5.x.
- [x] 3.3 Wire the pass pipeline into `check(cfg) -> CheckOutcome` (exit 0/1/2 per the
  script-checking spec) with all findings collected across passes; verify a fixture with
  findings in multiple passes reports all of them.

## 4. CLI surface

- [x] 4.1 Add the `check` subcommand to `src/cli.rs` (one positional script, `--no-color`),
  dispatch with registry discovery (failure → exit 2); verify `ponos check` unit tests
  in `cli.rs` (parse shape, missing-arg usage error) pass.
- [x] 4.2 Add the `run` pre-flight in `src/cli.rs`: compile + require + agent lints only
  (no strictness, no luau-lsp), findings to stderr, exit 1 before any agent spawns;
  verify with an integration test using the mock agent that an unknown literal agent
  name fails the run with no subprocess spawn (mock never receives a handshake).
- [x] 4.3 Confirm no behavior regressions for valid scripts: `cargo test` (full suite)
  passes, including `tests/examples.rs`.

## 5. Integration tests (`tests/check.rs`)

- [x] 5.1 Clean-path test: fixture script (strict, known agent, good requires) + stub
  `luau-lsp` (exit 0) on the child PATH; verify exit 0 and no stdout findings.
- [x] 5.2 Findings tests: syntax-error fixture; unknown-agent fixture; escaping/missing
  require fixture; missing `--!strict` fixture — each with a happy stub; verify exit 1
  and `path:line:col:` diagnostics plus summary line.
- [x] 5.3 luau-lsp stub findings test: stub printing a canned `file(1,1): TypeError:`
  line to stderr and exiting 1; verify the line passes through raw and the check exits 1.
- [x] 5.4 Missing-binary test: empty PATH temp dir; verify exit 2 and the "luau-lsp not
  found" error message.
- [x] 5.5 Pre-flight integration: `run` with unknown literal agent name exits 1 before
  spawn (no mock handshake), and `run` of a non-strict valid script still succeeds
  (covered in 4.2/4.3 — keep the assertions here if not already there).

## 6. Docs

- [x] 6.1 README: add the check section (passes, luau-lsp PATH dependency,
  `--!strict` requirement, exit codes) and extend the exit-code contract wording;
  update the editor-setup residuals entry for the require-tree restriction (editor
  analysis does not enforce it; `ponos check` does, statically) — verify against the
  type-definitions delta spec scenarios.
- [x] 6.2 AGENTS.md: extend the exit-code contract note (`2` for `check` also means
  "check could not run") and mention `src/check*` in the architecture map; verify
  wording matches the script-checking spec's exit-code requirement.
- [x] 6.3 Run `openspec validate add-check-subcommand`, `cargo test`, and
  `nix flake check` (includes `ponos-analyze`); verify all green.
