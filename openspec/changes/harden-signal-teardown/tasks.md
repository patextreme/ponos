## 1. Core: the process-group kill registry

- [ ] 1.1 Add the registry type to `ptah-core` (register / deregister / snapshot of group-leader pids; pure data, no I/O) with unit tests covering register→snapshot, deregister→gone, double-deregister, and snapshot-is-a-copy; verify `cargo test -p ptah-core` passes and `cargo test -p ptah-core --test deps_guard` still passes (core stays I/O-free)

## 2. Adapters register their children

- [ ] 2.2 Give `ptah_acp::Transport` a constructor taking the shared registry handle (keep the current unit-struct path working for tests that don't care, or migrate them), register the spawned agent's pid right after `spawn_process` succeeds, deregister at every driver child-disposal site (`kill_and_reap` callers); verify `cargo test -p ptah-acp` passes
- [ ] 2.3 Give `TokioProcessRunner` the registry handle: register the exec child's pid after spawn, deregister in `GroupKillGuard::Drop`; verify `cargo test --test exec` passes

## 3. Composition root: the second-signal sweep

- [ ] 3.4 Create one registry instance in `cli.rs::main`, inject it into the transport and process runner, hand it to `install_signal_monitor`; on the second signal the monitor snapshots and raw-`kill(-pid, SIGKILL)`s every entry, then exits with the code matching the second signal (130 SIGINT / 143 SIGTERM) instead of hardcoded 130; verify the sweep function unit test (spawn a real `sleep` child, register, sweep, assert dead) passes
- [ ] 3.5 Confirm the first-signal teardown path is untouched (`kill_inflight_execs`, session close/join, `kill_and_reap` all unchanged); verify existing signal tests still pass: `cargo test --test cli signal`

## 4. e2e: pin the agent leg and the force escape

- [ ] 4.6 Add an agent-on-signal test: mock agent hung mid-turn (`MOCK_HANG`), SIGINT to ptah → run exits 130 and the agent process is dead (SIGTERM leg → 143); verify `cargo test --test cli` passes
- [ ] 4.7 Add a second-signal test: hung agent plus a tagged `ptah.exec("sleep <tag>")` in flight, first signal then immediate second signal → no surviving agent or tagged sleep, exit code matches the second signal; verify `cargo test --test cli` passes

## 5. Docs and cleanup

- [ ] 5.8 Amend README's cancellation paragraph: a second signal kills all agent and exec children before the immediate exit (code matching that signal); add the unix-only note to the same paragraph; verify `rg -n "second signal" README.md` shows the updated text
- [ ] 5.9 Delete `openspec/explorations/signal-handling-and-agent-orphaning.md` (superseded by this change and the amended specs); verify `openspec validate --change harden-signal-teardown` passes and the file is gone
- [ ] 5.10 Full suite green: `cargo test` (all crates) and `nix flake check` in the sandbox
