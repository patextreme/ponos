# Tasks: pi-acp MCP wiring

Development happens in the `.work/pi-acp` clone (gitignored) against pinned rev
`d1cffc0`; the diff is exported to `patches/pi-acp-mcp-config.patch`. Design
reference: `design.md` (D1–D6).

## 1. pi-acp patch (in `.work/pi-acp`)

- [ ] 1.1 Implement MCP config materialization: filter stdio servers from
  `session/new`/`session/load` params, write `{ mcpServers: { <name>: {
  command, args, env, directTools: true } } }` to a `mkdtemp()` file (mode
  0600), and verify a unit test covering mapping + file mode + stdio-only
  filtering (http/sse warned and dropped) passes via `npm test`
- [ ] 1.2 Add the capability probe (cached `pi --help` substring check for
  `--mcp-config`) and the warn-and-drop path, and verify with a unit test
  using a fake `pi` command (`PI_ACP_PI_COMMAND`) whose `--help` omits the
  flag: session still spawns, warning surfaces, no `--mcp-config` arg passed
- [ ] 1.3 Thread the config through spawning: `PiRpcProcess.spawn` accepts the
  extra arg, `agent.ts` passes it only when servers were provided, temp file
  unlinked at session dispose; verify a component test spawns a fake pi,
  asserts the `--mcp-config <path>` argv token pair, reads the file contents,
  and asserts cleanup after session end
- [ ] 1.4 Run the full pi-acp suite (`npm test`) and lint (`npm run lint`);
  all green

## 2. Patch + flake

- [ ] 2.1 Export the diff to `patches/pi-acp-mcp-config.patch` (clean against
  rev `d1cffc0`; `git diff` in the clone) and verify it applies: reset the
  clone to the rev, `git apply` the patch, no conflicts
- [ ] 2.2 Add `packages.<system>.pi-acp` to `flake.nix`: `buildNpmPackage`,
  pinned rev `d1cffc047ab37a096ee70ca39cfc1de463db8d12`, nodejs 22,
  `patches = [ ./patches/pi-acp-mcp-config.patch ]`; verify with
  `nix build .#pi-acp` and `dist/index.js` present in the output
- [ ] 2.3 Add `pi-acp` to `devShell.packages`; verify inside `nix develop`
  that `command -v pi-acp` resolves to the flake output and
  `.ptah/config.toml`'s `pi` agent entry needs no edit
- [ ] 2.4 Verify `nix flake check` passes including the new package build

## 3. End-to-end + docs

- [ ] 3.1 Manual smoke (gitignored `.work/smoke/`): a `.luau` script with
  `resultSchema` run via `ptah run --agent pi` inside `nix develop`; verify
  the model sees `ptah_result_submit`, submits, and the script receives the
  typed result (not `nil`); run two scripts concurrently with different
  schemas in the same cwd and verify both results are correct
- [ ] 3.2 Degradation check: with `PI_ACP_PI_COMMAND` pointed at a fake pi
  lacking `--mcp-config` (or adapter temporarily disabled), verify the run
  still completes with `result = nil` and the warning is observable
- [ ] 3.3 Document in `README.md`: the `pi` agent entry, the
  `pi install npm:pi-mcp-adapter` prerequisite, `PI_ACP_PI_COMMAND` for
  non-dev-shell use, and the patch/pin maintenance note; verify the commands
  and paths mentioned match the shipped flake output
- [ ] 3.4 `openspec validate --change pi-acp-mcp-wiring` passes and the
  change is ready for review/archive
