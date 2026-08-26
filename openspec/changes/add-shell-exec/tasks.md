## 1. Core: port and event types

- [ ] 1.1 Add `ProcessRunner` port (pure trait + `ExecOutcome`/`ExecError` data types) to `crates/ponos-core/src/ports.rs`; verify `cargo test -p ponos-core` passes and `deps_guard` stays green (no new core imports)
- [ ] 1.2 Add `ExecStart { command }` / `ExecEnd { command, exit_code: Option<i32>, timed_out, duration_ms }` variants to `SessionEvent` in `crates/ponos-core/src/events.rs`; verify `cargo build -p ponos-core` and any exhaustive-match compile errors guide the follow-up render work

## 2. Runtime: injection, binding, teardown

- [ ] 2.1 Add `process_runner: Option<Arc<dyn ProcessRunner>>` to `RunConfig` (`crates/ponos-luau/src/state.rs`) and thread it into `RuntimeState`; verify build with the field defaulting to `None` (no injection sites changed yet)
- [ ] 2.2 Implement the `ponos.exec` async callback in `crates/ponos-luau/src/bindings.rs`: parse `(string, opts?)`, reject a non-table opts, read the runner (raise a clear runtime error when absent), emit `ExecStart`, await the port, emit `ExecEnd`, return the camelCase `{ exitCode, stdout, stderr }` table; raise on spawn-failure and timeout with a message naming command and budget. Verify with a unit/e2e test driving a stub runner injected directly (success, nonzero exit, timeout, opts type error)
- [ ] 2.3 Implement in-flight exec teardown: cancel-safety in the runner wrapper (kill on drop) plus a registry in `RuntimeState` so script error / `ponos.exit` / outer cancel kills every live process group; verify with e2e tests where an exec is in flight during a script error and during `ponos.exit` (child observed dead after run)
- [ ] 2.4 Implement `ponos.json.parse`/`stringify` (pure serde_json, `null`→`nil`, indent option, clear errors for malformed input and non-string keys) in `bindings.rs`; verify unit tests: round trip, malformed raises, `exec` stdout → `parse` integration

## 3. Composition root: tokio runner + renderer

- [ ] 3.1 Implement the tokio `ProcessRunner` in ponos-cli (spawn `/bin/sh -c` with `process_group(0)`, stdin null, concurrent stdout/stderr reads + wait, `tokio::time::timeout`, kill group on expiry then reap) and inject it into `RunConfig` at the composition root; verify `cargo build` and a manual `ponos run` smoke script using `printf | wc -l`
- [ ] 3.2 Render exec lifecycle lines in `crates/ponos-render`: start line with the command, end line with exit code + duration (or timeout marker) under the reserved `"exec"` pseudo-label, full timestamp like other lines; suppressed entirely by `--quiet`; verify renderer unit tests for color/quiet and an e2e asserting the lines appear in order around a slow command
- [ ] 3.3 Reserve the `"exec"` label: reject it as a user-set session `id` at session-options validation with a clear error; verify unit test for the rejection and that README notes the reservation

## 4. Definitions, docs, example

- [ ] 4.1 Add `ponos.exec` (`ExecOptions`/`ExecResult`) and `ponos.json` to the embedded definitions; extend the runtime probe test to exercise both at runtime; verify `cargo test -p ponos-cli` probe passes and `ponos check` on a scratch exec-using script reports no unknown-global/type errors
- [ ] 4.2 Update README: API-table rows for `ponos.exec` and `ponos.json`; rewrite the sandbox paragraph for the injected-capability story (ambient globals unchanged, exec ungated and why); note non-interactive stdin, env/cwd inheritance, POSIX-sh contract, quoting guidance for dynamic args, and the `"exec"` label reservation. Verify docs review against the shipped behavior
- [ ] 4.3 Add the offline bundled example (git/`printf` pipelines + `ponos.json.parse` + one mock-agent turn interleaved with an exec) and register its test in `crates/ponos-cli/tests/examples.rs`; verify `cargo test --test examples <name>` passes

## 5. E2E coverage and full suite

- [ ] 5.1 Add e2e tests in `crates/ponos-cli/tests/`: successful command + pipeline, nonzero exit returns data, timeout kills group (child with a subshell child, both dead), `pcall` catches timeout, spawned agent progresses during exec, stdin EOF (`cat` exits immediately), env/cwd inheritance, quiet mode suppresses exec lines; verify `cargo test --test e2e` passes offline
- [ ] 5.2 Run the full suite (`cargo test`, then `nix flake check` in the sandbox) and fix fallout; verify everything green
