## Why

Workflows are not purely probabilistic: between agent turns there is deterministic, side-effecting work (list GitHub PRs with `gh`, run a build, format files) that today can only ride a probabilistic agent turn or be pushed outside the script into wrapper bash. A `.luau` script should be able to run a shell command itself, get a typed result back, and feed it into agent orchestration — e.g. filter `gh pr list --json` output deterministically, then fan an agent out per surviving PR. Exec turns ponos scripts from "agent orchestrators" into "workflow orchestrators with an agent step."

## What Changes

- Add `ponos.exec(cmd: string, opts?: { timeoutMs: number? }) -> { exitCode: number, stdout: string, stderr: string }`: run a command through `/bin/sh -c`, blocking the calling coroutine (in-flight spawned agents keep progressing); capture stdout/stderr; return a result table for any exit code; raise a Lua error only when the command could not run at all or `timeoutMs` expires (the process group is killed, mirroring the turn-timeout contract).
- Add `ponos.json.parse(string) -> value` and `ponos.json.stringify(value, { indent?: number }) -> string` — a pure, I/O-free JSON module so captured command output becomes script data.
- Fund a fifth core port, `ProcessRunner`, implemented with tokio and composed in ponos-cli; the capability is injected into the runtime via `RunConfig` (like `AgentTransport`), keeping ponos-luau a curated environment plus injected capabilities and ponos-core I/O-free. The port set was closed by design; this change is that deliberate re-opening.
- Exec emits lifecycle events through `EventSink` (start line with the command, end line with exit status and duration); captured output stays captured — no live streaming.
- Environment contract: the child inherits ponos's environment and working directory; stdin is closed (`/dev/null`) — exec is non-interactive; no `cwd`/`env` override options in v1.
- Teardown: in-flight execs are killed (process groups) when the script errors, calls `ponos.exit`, or the run is cancelled — no orphans.
- Embedded type definitions gain `ponos.exec` and `ponos.json` (check/types break without them); README documents both APIs; one bundled offline example (git/sh builtins, not `gh`) exercises exec + json end-to-end with its `examples.rs` test.
- The sandbox description changes honestly: ambient globals still expose no subprocess execution — world access arrives only through the injected `ponos.exec` capability. No gating flag; the security story remains "sandbox limits the blast radius of bugs, not malice" (every `ponos run` already yields arbitrary shell via the headless allow-all agent).

## Capabilities

### New Capabilities
- `shell-exec`: the `ponos.exec` binding — invocation via `/bin/sh -c`, the result table, error contract (couldn't-run and timeout raise; nonzero exit is data), `timeoutMs` with process-group kill, environment/stdin inheritance, lifecycle events, teardown behavior, and the injected `ProcessRunner` port that funds it.

### Modified Capabilities
- `scripting`: the "Sandboxed Luau environment" requirement's blanket prohibition on subprocess execution is narrowed — ambient globals remain free of it, but the environment exposes the injected `ponos.exec` capability (specified by `shell-exec`); additionally a new requirement adds the `ponos.json` module to the namespace.
- `render-logging`: a new requirement covers rendering exec lifecycle lines (start line with the command, end line with exit code and duration) in color and quiet modes.
- `type-definitions`: "Definitions cover the script API" grows to enumerate `ponos.exec` (command string, options table, result table) and the `ponos.json` module; the runtime probe test exercises both.

## Impact

- `crates/ponos-core`: new `ProcessRunner` port in `ports.rs` (pure trait: run a command with optional timeout, return exit/stdout/stderr; no I/O in core), new exec lifecycle `SessionEvent` variants or a non-session-scoped event shape, `deps_guard` allowlist review for any new imports (none expected — trait only).
- `crates/ponos-luau`: `RunConfig` gains the injected runner; `bindings.rs` gains `ponos.exec` (async callback: emit start event, await port, emit end event, build result table) and `ponos.json` (pure serde_json-backed); teardown path tracks in-flight execs and kills their process groups.
- `crates/ponos-cli`: compose the tokio `ProcessRunner` impl at the composition root; renderer handles the new lifecycle lines in color/quiet/verbose modes; embedded definitions updated; README API rows + sandbox paragraph updates.
- `crates/ponos-cli/tests`: new e2e fixtures using offline shell builtins (`printf`, `sh -c 'exit 3'`, `sleep` for timeout/kill coverage); new bundled example + its `examples.rs` entry; definitions probe script extended.
- No dependency changes (tokio and serde_json already in the tree). No breaking changes to existing API surface; the `ponos` namespace only grows.
