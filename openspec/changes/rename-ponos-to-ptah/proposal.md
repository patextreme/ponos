# Rename ponos → ptah

## Why

The name carries an unfortunate reading: Greek πόνος means "pain/toil", and in
some regions "ponos" reads as diarrhea — a liability for a name users type and
say daily. `ptah` (Egyptian god of craftsmen, who created the world by speaking
it — apt for a tool that drives agents purely through prompts) is available,
short, and typed just as easily. The rename is cheapest now, pre-release,
before scripts, skill deployments, or muscle memory accrete.

## What Changes

A mechanical, workspace-wide rename with **zero compatibility shims**. No
behavior, contract, or semantic changes of any kind — same values, same errors,
same wire protocol shapes; only the names change.

- **BREAKING** The Luau global `ponos` becomes `ptah` (`ptah.agent`,
  `ptah.spawn`, `ptah.parallel`, `ptah.join`, `ptah.sleep`, `ptah.log`,
  `ptah.exit`, `ptah.exec`, `ptah.json`, `ptah.version`). No deprecated
  `ponos` alias: sandboxed globals never die if aliased, and this repo owns
  every script that uses them.
- **BREAKING** Project directory `.ponos/` becomes `.ptah/`; the definitions
  file `ponos.d.luau` becomes `ptah.d.luau`; user config `~/.config/ponos/`
  becomes `~/.config/ptah/`. Old paths are not read as fallback.
- **BREAKING** The binary and CLI verbs become `ptah run`, `ptah check`,
  `ptah types`; rendered diagnostics switch the `[ponos]` prefix to `[ptah]`.
- **BREAKING** Result-channel wire names flip together (both halves ship in
  this repo, so no external consumer can break): MCP server name `ponos` →
  `ptah` (tool `mcp__ponos__result_submit` → `mcp__ptah__result_submit`),
  env vars `PONOS_BRIDGE_ADDR`/`PONOS_RESULT_SCHEMA` → `PTAH_*`, socket
  prefix `ponos-r-` → `ptah-r-`, test-only vars `PONOS_REQUIRE_REAL_LSP`,
  `PONOS_TEST_MODEL`, `PONOS_EXEC_TEST_TOKEN` → `PTAH_*`.
- Crate and package identity: `crates/ponos-{cli,core,acp,check,config,luau,render,result}`
  → `crates/ptah-*`; the facade lib `ponos` → `ptah`; the CLI package becomes
  `ptah-cli` with `[[bin]] name = "ptah"` (the bare `ptah` crate name is taken
  on crates.io by a dormant crate; `ptah-cli`/`ptah-core` are free, keeping a
  future publish story stable). Nix attrs (`ptah`, `ptah-tests`, `ptah-smoke`,
  `ptah-analyze`, source-filter `ptahSrc`), `deps_guard` crate-name pins, and
  `CARGO_BIN_EXE_ponos` test references follow.
- Docs and meta sweep in the same change: README (including the name-origin
  block, rewritten for Ptah), `skills/ponos/` → `skills/ptah/` (SKILL.md API
  references and the `github.com/patextreme/ptah` URLs), `examples/`,
  `.ponos/` scripts (utils/workflows/instructions), `.helix/languages.toml`
  definitions path, AGENTS.md, and all of `openspec/` — synced specs via
  these deltas, and archived changes' prose swept mechanically (git history
  preserves the original wording; the working tree speaks one name).
- Sequencing: lands on a quiet tree — `add-shell-exec` archives first, and
  these deltas are written against the post-`add-shell-exec` spec surface,
  carrying its additions (e.g. `ptah.exec`) forward renamed, never reverted.
- Out of scope (manual, same day): the GitHub repo rename and the local
  working-directory rename; re-pointing the deployed `~/.pi/agent/skills/ponos`
  symlink; moving the user's own `~/.config/ponos/config.toml`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

Every capability's requirements embed the old name in normative text (the
global, the CLI verbs, the config paths, the MCP server name, the `[ponos]`
diagnostic prefix). The deltas are mechanical renames of that text; no
requirement's semantics change.

- `agent-registry`: config paths `.ponos/config.toml` → `.ptah/config.toml`,
  `~/.config/ponos/` → `~/.config/ptah/`; `ponos.agent` → `ptah.agent` in
  scenarios and prose.
- `agent-sessions`: actor prose (`ponos SHALL …` → `ptah SHALL …`) throughout.
- `cli`: the binary surface `ponos run` / `ponos check` / `ponos types` →
  `ptah …`; `ponos.log` → `ptah.log`; `ponos.agent` scenario mentions.
- `render-logging`: `ponos`-prefixed output lines → `ptah`; actor prose.
- `script-checking`: `ponos check` invocations and `ponos.agent` lint mentions
  → `ptah …`; `.ponos/config.toml` discovery mention → `.ptah/…`.
- `scripting`: the `ponos` namespace and every `ponos.*` member mention →
  `ptah.*` (including the `exec` member `add-shell-exec` adds).
- `session-config-options`: actor prose.
- `type-definitions`: the declared global `ponos` → `ptah`, `ponos types` →
  `ptah types`, `ponos.d.luau` filename mentions → `ptah.d.luau` (member list
  carried forward with `exec`).
- `typed-results`: MCP server name `ponos` → `ptah`; the `[ponos]` lifecycle
  diagnostic line → `[ptah]`; actor prose.
- `shell-exec` (exists once `add-shell-exec` archives): every `ponos.exec` /
  `ponos.json` mention in its requirements and scenarios → `ptah.exec` /
  `ptah.json`, including the `ponos.exit kills running child` scenario heading.

## Impact

- Every crate, the workspace manifest, Cargo.lock, the Nix flake (pname,
  packages, checks, source filter), and the embedded-definitions
  `include_str!` path in `ponos-check`.
- ~160 files, ~1,500 occurrences, per the pre-proposal inventory. All
  user-visible surfaces break simultaneously in one commit — scripts, config
  paths, env vars, socket names — by design (no half-renamed state).
- `mock-agent` internals that literally construct client name/stdio name
  `"ponos"` and the `mcp__ponos__result_submit` tool name in tests.
- The Nix source filter's hardcoded `/.ponos` suffix rules and the root-level
  stale `result*` symlinks into `/nix/store/*-ponos-*` (cosmetic, gitignored).
