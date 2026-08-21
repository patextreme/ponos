# Tasks: Agent Orchestration Runtime (v1)

## 1. Project scaffolding

- [ ] 1.1 Initialize cargo package (`ponos`, binary crate): `Cargo.toml` with deps `clap`, `mlua` (`luau`, `async`), `agent-client-protocol`, `tokio` (full), `serde`, `toml`, `tracing` (+ `tracing-subscriber`); verify `cargo build` succeeds and a hello-world `src/main.rs` runs
- [ ] 1.2 Create `src/` module skeleton (`config.rs`, `script/`, `acp/`, `render/`, `task.rs`) as empty modules wired into `main.rs`; verify `cargo build` still green
- [ ] 1.3 Add `rust-toolchain.toml` pinning an exact nightly; verify `rustc --version` inside and outside nix devshell reports the pin

## 2. Nix flake (flake-parts)

- [ ] 2.1 Write `flake.nix` as a flake-parts shell importing `./nix/*.nix`; inputs: nixpkgs, rust-overlay (oxalica), crane, flake-parts; verify `nix flake check` parses
- [ ] 2.2 `nix/toolchain.nix`: derive toolchain from `rust-toolchain.toml` via oxalica overlay; verify devshell cargo matches the pin
- [ ] 2.3 `nix/package.nix`: crane build of the crate for x86_64-linux; verify `nix build` produces a runnable `ponos --version`
- [ ] 2.4 `nix/devshell.nix` + `nix/apps.nix`; add aarch64-linux and aarch64-darwin to supported systems; verify `nix develop -c ponos --version` and `nix run` work (cross-arch verified by build matrix if available, else by `nix flake check` evaluation)

## 3. Config registry

- [ ] 3.1 Implement TOML loading of user (`~/.config/ponos/config.toml`) and project (`.ponos/config.toml`) registries with per-entry project-wins merge; unit tests cover merge precedence and absent-both-files
- [ ] 3.2 Implement `${VAR}` interpolation (unset → empty) for `command`/`args`/`env`; unit tests cover set, unset, and embedded cases
- [ ] 3.3 Implement resolve-by-name returning an error naming the agent when unresolvable; unit test asserts the error message

## 4. ACP client core

- [ ] 4.1 Implement process spawn + `initialize` handshake + connection setup per session, with inherited env merged over entry `env`; verify against the mock agent (5.1) with a ping-through integration test
- [ ] 4.2 Implement deny-all `Client` trait impl: every agent→client request answered `-32601` promptly; unit/integration test asserts a permission-requesting agent gets an error and the turn still completes
- [ ] 4.3 Implement session driver: `session/new` (cwd, id, mcp_servers passthrough), update notification folding (message chunks → text, usage), and `session/prompt` request/response with `stopReason`; integration test: chunked echo assembles final text and stop_reason
- [ ] 4.4 Implement `session/cancel` path: `session:cancel()` and `timeout_ms` expiry both send cancel; the awaiting prompt returns `stop_reason == "cancelled"` (cancel) or raises timeout error (expiry); integration tests for both
- [ ] 4.5 Implement `session:close()` and run-end teardown (terminate + reap all children) on normal end, `ponos.exit`, and uncaught error; integration test asserts no zombie processes remain (check via `waitpid` bookkeeping / process listing)

## 5. Mock ACP agent fixture

- [ ] 5.1 Create `fixtures/mock-agent` binary: initialize handshake, session lifecycle, echo prompt with configurable chunk stream (`MOCK_CHUNKS`, `MOCK_DELAY_MS` env or scenario file); verify with a hand-run JSON-RPC session transcript
- [ ] 5.2 Extend mock agent: tool_call + plan updates, `usage_update`, cancel compliance (respond `stopReason=cancelled`), permission-request mode, stderr chatter mode; integration tests exercise each behavior

## 6. Luau runtime

- [ ] 6.1 Set up sandboxed Lua: `Lua::sandbox(true)`, curated stdlib (string, table, math, utf8, bit32, buffer, os.time/clock), `print` passthrough; unit test asserts `io`/`debug`/`os.execute` are absent
- [ ] 6.2 Implement relative require (custom `Require` over script dir, `.luau` resolution, caching, rejection of escaping paths); unit tests for sibling/missing/cached cases
- [ ] 6.3 Implement task runtime: `ponos.spawn` → Task with `:await()` (error re-raise), `ponos.join`, `ponos.map` with `concurrency` (default unlimited), `ponos.sleep`; unit tests with fake async ops cover ordering, cap, contained errors, await re-raise
- [ ] 6.4 Bind `ponos.agent`/`session`/`prompt`/`cancel`/`close` to the ACP layer (factory objects, default `s1,s2` labels, result table with `__tostring`, `usage`/`stop_reason`); integration test drives a full prompt turn against mock agent end-to-end from Luau
- [ ] 6.5 Implement `ponos.log`, `ponos.exit`, `ponos.version`; script-end waits for outstanding tasks; integration tests cover pending-spawn wait, explicit exit code, uncaught-error teardown (exit 1), and never-retrieved task error failing the run (exit 1)

## 7. CLI surface

- [ ] 7.1 Implement clap CLI: `ponos run <script>`, `--quiet`, `--verbose`, `-vv`, `--no-color`, `--version`; unit tests (clap harness) for missing-arg and flags; manual check each flag's observable effect
- [ ] 7.2 Implement renderer: line-buffered `[agent/session]`-prefixed output, per-session palette colors, `--no-color` degradation, `--quiet` suppression, `-vv` stderr passthrough; integration test captures stdout of a concurrent two-session run and asserts attribution and color behavior

## 8. End-to-end validation

- [ ] 8.1 Write example scripts (`examples/`): sequential review, fan-out map with concurrency cap, watchdog cancel; verify each runs green against the mock agent in CI
- [ ] 8.2 Add `checks` to nix running the full test suite offline in the sandbox (mock agent only); verify `nix flake check` passes
- [ ] 8.3 Manual smoke: one real adapter (e.g. `@agentclientprotocol/claude-agent-acp`) via user config; document the config snippet in `README.md`; verify a real turn streams and completes
- [ ] 8.4 Validate the change: `openspec validate add-agent-orchestration-runtime --strict` passes and all specs' scenarios are covered by a test or documented manual check
