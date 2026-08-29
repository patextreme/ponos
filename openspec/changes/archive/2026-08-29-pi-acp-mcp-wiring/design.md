# Design: pi-acp MCP wiring

## Context

See `proposal.md` for motivation. The facts that shape this design (verified
against the pinned sources):

- **ptah side needs nothing.** When a script declares `resultSchema`,
  `crates/ptah-acp/src/driver.rs` binds a per-session Unix socket and pushes one
  ACP `session/new { mcpServers }` **stdio** entry: spawn `<ptah-exe> __bridge`
  (an MCP stdio server exposing `result_submit`) with env `PTAH_BRIDGE_ADDR`
  (socket path — doubles as an unguessable capability token) and
  `PTAH_RESULT_SCHEMA`. The bridge is fully self-contained; guidance lives in
  the tool description; ptah never edits prompt text.
- **pi-acp v0.0.33** accepts `mcpServers` in `session/new`/`session/load` and
  stores them on the session object, unused (`src/acp/agent.ts:282`). It spawns
  `pi --mode rpc --no-themes` in the session cwd with `env: process.env`
  (`src/pi-rpc/process.ts`), and advertises `mcpCapabilities: { http: false,
  sse: false }`.
- **pi has no native MCP.** The `pi-mcp-adapter` package (v2.29.0 installed
  here) provides it: it reads layered MCP config files, supports stdio entries
  (`command/args/env`), connects lazily, and supports per-server
  `directTools: true|false|[names]` to register a server's tools as first-class
  pi tools instead of behind its `mcp` proxy tool.
- **Ephemeral channels exist.** Two were found:
  1. `pi --mcp-config <path>` — a CLI flag the adapter itself registers; inside
     a normal pi run the adapter resolves `options.configPath ??
     getConfigPathFromArgv()` (`index.ts:197`), so the flag flows through the
     full config-load path, `directTools` included.
  2. `pi -e <ext>` + the adapter's `pi-mcp-adapter:runtime-register:v1` event —
     in-process registration with no file, but runtime-registered servers are
     **hard-forced proxy-only** (`index.ts:424`: `directTools: false`).

## Goals / Non-Goals

**Goals:**

- `resultSchema` scripts running against the `pi` agent produce typed results
  end-to-end, with `result_submit` visible to the model as a direct tool.
- Per-session isolation: concurrent pi agents in the same or different cwds,
  with different result shapes, never interfere.
- Zero mutation of user-owned files (no `.pi/mcp.json`, no `.mcp.json`, no
  settings edits) and zero ptah crate changes.
- The patched pi-acp builds in this repo's flake; `nix develop` puts it on
  PATH; `nix flake check` builds it.

**Non-Goals:**

- Upstreaming to `svkozak/pi-acp` (out of scope by decision; patch carried
  in-repo).
- Auto-provisioning `pi-mcp-adapter` (warned, not installed).
- ACP http/sse MCP servers (pi-acp advertises neither; warn + drop).
- Any change to ptah's bridge, driver, or registry semantics.

## Decisions

### D1 — Delivery channel: per-session `--mcp-config` temp file

pi-acp writes an ephemeral MCP config to `mkdtemp()` under `os.tmpdir()` and
appends `--mcp-config <path>` to the pi spawn args, only when stdio
`mcpServers` were provided.

- Chosen over runtime-register (`pi -e` + event): that path forces
  proxy-only, so the model would have to *search* the `mcp` proxy tool to
  discover `result_submit` — ptah gives no prompt-level hint (guidance is in
  the tool description), so discovery is unreliable and results would
  silently degrade. The config path honors `directTools`.
- Chosen over a self-contained `-e` extension embedding a minimal MCP stdio
  client (`pi.registerTool` directly, no adapter dependency): it would remove
  the adapter prerequisite but roughly triples the patch (JSON-RPC framing,
  initialize, tools/list, tools/call, teardown). Revisit only if the adapter
  prerequisite proves unacceptable.
- Chosen over writing the project's `.pi/mcp.json`: mutating user-owned files
  is messy (merge/clobber/`/mcp enable` interplay) and shared-cwd parallel
  sessions would race one file.

Parallelism falls out for free: each pi process reads only its own temp file,
so per-session sockets and schemas are isolated and identical server names
cannot collide across sessions. The user's other config layers still merge
normally underneath.

### D2 — Entry mapping and direct tools

Each ACP stdio server `{name, command, args, env}` becomes
`{ "command", "args", "env", "directTools": true }` under that name. For ptah
this yields tool `ptah_result_submit` — same visibility as Claude Code's
`mcp__ptah__result_submit`. `directTools: true` (all tools of the server) is
deliberate: an ACP client offering a server is explicitly telling the agent it
matters; hiding it behind proxy search inverts that intent. Server names are
kept as the client provided them.

### D3 — Absence handling: cached probe, warn + drop

`--mcp-config` is an extension-registered flag; without pi-mcp-adapter, pi
would fail arg-parsing at spawn. Today pi-acp silently ignores `mcpServers`,
so a hard failure would regress adapter-less users. Therefore: probe once per
pi-acp process (`pi --help`, substring check for `--mcp-config`, cached in a
module-level variable); if unsupported → surface a warning (same channel as
other startup notices) stating N MCP servers were dropped and the adapter
prerequisite, then spawn without the flag. ACP http/sse servers are warned +
dropped identically.

### D4 — Temp file hygiene

Written before spawn (the adapter reads it during pi init), mode `0600` — the
file contains `PTAH_BRIDGE_ADDR`, a capability token. Removed at session
dispose; stale files (crash) are inert. The flag is passed as two separate
argv tokens; pi-acp builds the args array directly, so no shell-quoting
concerns.

### D5 — Nix shape

`buildNpmPackage` for pi-acp v0.0.33 pinned at rev
`d1cffc047ab37a096ee70ca39cfc1de463db8d12` (fetched via `fetchzip`/
`fetchFromGitHub` + npm deps from `package-lock.json`), nodejs 22 (engines
floor is ≥20), patch applied via `patches`. Output `packages.<system>.pi-acp`;
also added to `devShell.packages` so this repo's `.ptah/config.toml`
(`command = "pi-acp"`) resolves via PATH unchanged — no store paths in the
committed config. Non-dev-shell consumers point the registry env
`PI_ACP_PI_COMMAND` at their nix `pi`.

### D6 — Patch source workflow

The patch is developed against a clone in `.work/pi-acp` (gitignored): make
changes, run `npm test` there, then export `git diff` to
`patches/pi-acp-mcp-config.patch` pinned to the recorded rev. Rev bumps
require a manual rebase (accepted; upstreaming out of scope).

## Risks / Trade-offs

- [Patch bit-rots against upstream pi-acp] → Pin the rev; rebase is a
  deliberate step; the patch stays small (spawn-arg + temp-file + probe logic
  in `agent.ts`/`process.ts`).
- [pi-mcp-adapter missing on a user's pi] → D3 probe + warning; results
  degrade to `nil`, exactly ptah's documented degradation path.
- [`--mcp-config` flag semantics change in future adapter versions] →
  Prerequisite pinned in docs (adapter ≥ the version registering the flag;
  2.29.0 verified); probe failure degrades rather than crashes.
- [Temp file survives a pi-acp crash] → Inert (lazy connect; socket gone);
  tmpdir cleanup eventually reclaims it.
- [Name collision between an ACP server name and a user-configured server] →
  Documented edge; the adapter's merge keeps configured servers authoritative
  for runtime registrations, and ptah's `ptah` name is unlikely to be
  user-configured.

## Migration Plan

Additive: new flake output + patch + docs. Nothing existing changes; rollback
is removing the package from the dev shell (the user-level pi-acp, if any,
resumes resolving via PATH).

## Open Questions

None — the delivery mechanism, degradation behavior, hygiene, and packaging
were all settled during the design interview (Q1–Q12).
