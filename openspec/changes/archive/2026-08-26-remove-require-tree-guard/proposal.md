## Why

`require` is jailed to the entry script's directory tree, which blocks the
most natural way to organize larger setups: sibling workflows sharing a
helper (`workflow-1/main.luau`, `workflow-2/main.luau`,
`shared/helper.luau` fail today, since `require("../shared/helper")` is
rejected as a script-tree escape). The guard was never a security boundary —
ponos scripts drive agent subprocesses with the user's full authority — so
the jail only costs organization freedom without buying safety. Editor
analysis (luau-lsp) already resolves such requires freely, making the
runtime the odd one out.

## What Changes

- `require("./...")` / `require("../...")` resolve relative to the requiring
  file with **no boundary**: requires may walk out of the entry script's
  directory to anywhere on disk reachable by relative path. Pure widening —
  every script that works today keeps working.
- Non-relative require strings (absolute paths, bare module names, aliases)
  remain rejected with a Lua error, exactly as today.
- The "escapes the script directory" finding class is **deleted** from
  `ponos check` and from the `ponos run` pre-flight — no portability warning
  replaces it. The script-tree concept is retired outright, not relocated.
- Static analysis parity improves: the documented type-definitions residual
  ("require-tree restriction is not enforced by editor analysis") ceases to
  exist and its documentation clause is removed.
- README and the ponos skill doc state the trusted-code contract explicitly
  (agent-scoped wording): scripts are trusted, the sandbox limits the blast
  radius of bugs, not malice.
- New `examples/` entry demonstrating a cross-tree require (two workflows +
  shared helper), pinned by an offline mock-driven test in `tests/examples.rs`.

Not doing: absolute-path requires, alias maps in config.toml, a portability
lint, and symlink canonicalization (no boundary remains to escape through).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `scripting`: the module-resolution requirement drops the script-tree
  escape guard; relative resolution is unbounded, non-relative rejection
  stays normative. New scenario: cross-tree require succeeds.
- `script-checking`: the static-lint requirement drops the script-tree
  escape clause from its findings list; the escape scenario is removed.
- `cli`: the `run` pre-flight requirement drops the script-tree escape guard
  from literal-require resolution.
- `type-definitions`: the README-requirement's known-residual list drops the
  require-tree/editor-divergence clause and its scenario.

## Impact

- `src/script/require.rs` — remove `guard`/`escapes` from the runtime
  navigator (`reset`, `to_parent`, `to_child`); keep `jump_to_alias`
  rejection and its message.
- `src/check/lint.rs` — remove the escape check in `resolve_edge` and the
  duplicated `escapes` helper; the walk stays bounded by its visited set.
- Tests — escape tests (runtime navigator + lint) replaced by cross-tree
  require tests; new examples test.
- Docs — README require section (~"rejects paths escaping the script tree")
  and residuals list; skill doc require section.
- Spec deltas — four capabilities above, all deletions/widenings; no new
  requirements.
