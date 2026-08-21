## 1. Definitions file

- [ ] 1.1 Write `types/ponos.d.luau` (`@meta`, `--!strict`): full `ponos` namespace (`agent`, `spawn`, `map`, `join`, `sleep`, `log`, `exit`, `version`), `Session` class (`prompt` → result/usage tables, `cancel`, `label` as method, `close`), `Task` class (`await`), option tables with typed `mcp_servers` from the ACP structs, generic `Outcome<T>` union on `map`/`spawn`/`join`; verify with `luau-lsp analyze --platform=standard --definitions=types/ponos.d.luau` on a scratch strict script that completes, hovers, and narrows
- [ ] 1.2 Add sandbox shadow declarations (`os` = time/clock, `coroutine` = yield, `loadstring`/`collectgarbage` = nil) and verify a scratch script calling `os.date`, `coroutine.create`, or `loadstring` produces type errors

## 2. `ponos types` subcommand

- [ ] 2.1 Embed the definitions via `include_str!` and add the `types` subcommand printing them to stdout with a `-- ponos <version> type definitions` header; verify `ponos types | tail -n +2` is byte-identical to `types/ponos.d.luau` and the command exits 0 with no registry configured
- [ ] 2.2 Add CLI tests for `ponos types` (success, version header present, no agent spawned); verify `cargo test --test cli` passes

## 3. Drift guards

- [ ] 3.1 Author a `--!strict` probe script exercising every member, method, and field the definitions promise (against the mock agent) and wire it as a cargo test; verify the test fails when a promised member is temporarily removed, then passes restored
- [ ] 3.2 Add `--!strict` headers to `examples/*.luau` (with any annotation tweaks needed to pass strict analysis) and verify `luau-lsp analyze --platform=standard --definitions=types/ponos.d.luau examples/*.luau` is clean
- [ ] 3.3 Add `checks.ponos-analyze` to `nix/checks.nix` running the analyze gate over `examples/*.luau`, the probe script, and script test fixtures, plus `luau-lsp` to the devshell; verify `nix flake check` passes including the new check
- [ ] 3.4 Verify the full guard suite together: `cargo test` (probe + cli + examples) and `nix flake check` both green

## 4. Documentation

- [ ] 4.1 Add a README "Editor setup" section: `ponos types > ponos.d.luau`, generic luau-lsp settings for VS Code and Neovim (standard platform), and the known residuals (map-callback annotations, `tostring(r)` sugar untyped, narrowing needs a local, require-tree guard runtime-only); verify the section names no repo-committed config files
- [ ] 4.2 Start the README's first script snippet with `--!strict` and verify the snippet matches an example that passes the analyze gate
