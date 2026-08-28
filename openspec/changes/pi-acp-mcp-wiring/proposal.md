# pi-acp MCP wiring (patched package in the flake)

## Why

`pi` (driven through the `pi-acp` ACP adapter) cannot honor ptah's typed-results
contract: ptah already offers its `result_submit` bridge as a standard ACP
`session/new { mcpServers }` stdio entry whenever a script declares
`resultSchema`, but pi-acp accepts and stores those servers without ever wiring
them into pi (upstream: *"MCP servers are accepted in ACP params and stored in
session state, but not wired through to pi"*). The consequence: every
`resultSchema` script run against the `pi` agent silently degrades to
`result = nil`. We want full ptah ↔ pi interoperability, with the patched
adapter built and carried inside this repo's flake so testing is trivial and no
upstream coordination is required.

## What Changes

- New flake output `packages.<system>.pi-acp`: a `buildNpmPackage` of pi-acp
  v0.0.33 pinned at git rev `d1cffc0`, carrying an in-repo patch
  (`patches/pi-acp-mcp-config.patch`) that wires ACP-provided stdio MCP servers
  through to pi.
- Patch behavior in pi-acp: when `session/new` / `session/load` carry stdio
  `mcpServers`, write a per-session ephemeral MCP config (temp dir, mode 0600)
  and spawn `pi --mode rpc` with `--mcp-config <file>`; entries map ACP
  `{name, command, args, env}` to pi-mcp-adapter entries with
  `directTools: true` so tools appear as first-class pi tools. The temp file is
  removed when the session ends.
- Graceful degradation: support is probed once per pi-acp process
  (`pi --help` → `--mcp-config` present); when the pi-mcp-adapter package is
  absent (or an ACP http/sse server is offered), pi-acp warns and drops the
  servers — never a spawn failure, no regression for adapter-less users.
- The dev shell gains the package, so this repo's `.ptah/config.toml` entry
  (`command = "pi-acp"`) keeps resolving via PATH, unchanged.
- Docs: pi-agent interop notes — the `pi install npm:pi-mcp-adapter`
  prerequisite and `PI_ACP_PI_COMMAND` for pointing the adapter at a specific
  nix-provided `pi`.
- **No ptah crate changes.** The cargo suite stays untouched and fully offline;
  `nix flake check` additionally builds the new package.

## Capabilities

### New Capabilities

None — this change adds packaging (a patched third-party adapter as a flake
output) and documentation. No ptah behavior changes, so the change declares
`skip_specs: true` in `.openspec.yaml`.

### Modified Capabilities

None — `typed-results` and `agent-registry` behavior is unchanged on ptah's
side; the fix lives entirely in the patched adapter.

## Impact

- `flake.nix` — new package, devShell addition, check exposure.
- `patches/pi-acp-mcp-config.patch` — new in-repo patch against the pinned
  pi-acp rev (maintenance: manual rebase on rev bumps; upstreaming out of
  scope by decision).
- `README.md` — short interop section for the `pi` agent.
- `openspec/changes/pi-acp-mcp-wiring/` — this change.
- Runtime prerequisite on the user side: `pi install npm:pi-mcp-adapter`
  (warned, not auto-provisioned).
- Parallel-safety: per-session temp config + per-session bridge socket mean
  concurrent pi agents with different result shapes never collide.
