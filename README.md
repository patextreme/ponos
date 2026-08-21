# ponos

Luau-scripted multi-agent orchestration over the
[Agent Client Protocol](https://agentclientprotocol.com/) (ACP).

`ponos run script.luau` executes a small, versionable Luau script that drives
any ACP-speaking agent (Claude Code, Gemini CLI, Codex, …) over stdio.
Scripts look synchronous — `reply = session:prompt("…")` blocks the script,
not the runtime — so fan-outs, pipelines and watchdogs read like plain code.

```lua
local claude = ponos.agent("claude")
local s = claude:session({ id = "reviewer" })
local r = s:prompt("Review src/main.rs for obvious bugs; be terse.")
ponos.log(tostring(r))       -- r.text; r.stop_reason; r.usage.input …
s:close()
```

Fan out over many targets with a concurrency cap:

```lua
local outcomes = ponos.map(targets, function(t)
    return s:prompt("Summarize " .. t)
end, { concurrency = 2 })
```

## Install / build

```sh
nix build              # produces bin/ponos (crane, pinned nightly toolchain)
nix develop            # dev shell with the pinned toolchain
cargo build            # plain cargo works too
cargo test             # full suite; integration tests use the mock agent only
```

## CLI

```
ponos run <script.luau> [--quiet] [--verbose] [-vv] [--no-color]
ponos --version
```

- `--quiet` — suppress streaming render and diagnostics (script `print` still passes)
- `--verbose` — runtime lifecycle diagnostics
- `-vv` — additionally pass agent subprocess stderr through
- `--no-color` — drop ANSI colors, keep `[agent/session]` text prefixes

Exit codes: `0` on success, `1` on an uncaught script error or a never-observed
task error (printed to stderr), `2` on CLI/usage errors, and `n` when the script
calls `ponos.exit(n)`.

## Agent registry

Agents are configured in TOML. Project entries (`.ponos/config.toml`, found
upward from the invocation directory) override user entries
(`~/.config/ponos/config.toml`) per agent name:

```toml
# ~/.config/ponos/config.toml
[agents.claude]
command = "npx"
args = ["-y", "@agentclientprotocol/claude-agent-acp@latest"]
env = { ANTHROPIC_API_KEY = "${ANTHROPIC_API_KEY}" }
```

`${VAR}` interpolates from ponos's environment at resolve time (unset →
empty); `env` values are merged over the inherited environment. Scripts can
also pass an inline spec and skip the registry entirely:

```lua
local codex = ponos.agent({
    command = "npx",
    args = { "-y", "@agentclientprotocol/codex-acp@latest" },
})
```

Any Anthropic-compatible provider works through the standard env. For
example, running Claude Code against Z.AI's GLM models:

```toml
[agents.glm]
command = "npx"
args = ["-y", "@agentclientprotocol/claude-agent-acp@latest"]

[agents.glm.env]
ANTHROPIC_BASE_URL = "https://api.z.ai/api/anthropic"
ANTHROPIC_API_KEY = "${ZAI_API_KEY}"
ANTHROPIC_MODEL = "glm-4.6"
ANTHROPIC_SMALL_FAST_MODEL = "glm-4.5-air"
```

(Verified end-to-end: a real turn streams chunks + usage and completes with
`stop_reason = "end_turn"`.)

`ponos` declares no client capabilities: agents never get permission prompts,
file access, or terminal control — every agent-to-client request is answered
with a JSON-RPC method-not-found error, so turns never hang.

## The `ponos` namespace

| API | Description |
| --- | --- |
| `ponos.agent(name_or_spec)` | Agent factory (registry name or inline `{command=, args=, env=}` spec) |
| `agent:session({id=, cwd=, mcp_servers=})` | New session (own subprocess); `id` defaults to `s1, s2, …` |
| `session:prompt(text, {timeout_ms=})` | One turn → `{ text, stop_reason, usage }` (`__tostring` → text) |
| `session:cancel()` | Cancels the in-flight turn (returns `stop_reason = "cancelled"`) |
| `session:close()` | Ends the session and reaps the agent process |
| `ponos.spawn(fn)` → `task:await()` | Concurrent task; errors re-raise at the await site |
| `ponos.join({task, …})` | Wait for tasks → per-task `{ok, value}` / `{ok=false, error}` entries |
| `ponos.map(items, fn, {concurrency=})` | Fan-out (default unlimited) → per-item outcome entries |
| `ponos.sleep(ms)` / `ponos.log(msg)` / `ponos.exit(code)` / `ponos.version` | Runtime helpers |

Scripts run in a sandboxed Luau environment: `string`, `table`, `math`,
`utf8`, `bit32`, `buffer`, `os.time`, `os.clock`, and `print` — no file I/O,
subprocesses, network, or debug facilities. `require` resolves `.luau`
modules relative to the requiring file and rejects paths escaping the script
tree. (One deviation: a restricted `coroutine` table containing only `yield`
remains visible because the embedded async runtime needs it; the scheduling
primitives are absent.)

## Examples

See [`examples/`](examples/) — sequential review, fan-out with a concurrency
cap, and a watchdog cancel — and run them against the bundled mock agent:

```sh
mkdir -p .ponos
cat > .ponos/config.toml <<'EOF'
[agents.demo]
command = "target/debug/mock-agent"
args = []
EOF
ponos run examples/sequential_review.luau
```

## Development

- `src/bin/mock-agent/` — a scriptable ACP agent used by the offline test
  suite (`MOCK_CHUNKS`, `MOCK_HANG`, `MOCK_PERMISSION`, `MOCK_TOOL`,
  `MOCK_PLAN`, `MOCK_USAGE`, `MOCK_STDERR`, `MOCK_DELAY_MS`, …).
- `nix flake check` runs the entire suite in the sandbox.
- Toolchain: pinned nightly in `rust-toolchain.toml`, consumed by the oxalica
  overlay for devshell and crane builds.
- **NixOS note:** vendor binaries shipped inside npm packages (e.g. the
  Claude Code executable) are dynamically linked against a generic loader.
Run them via a wrapper that invokes a nix glibc loader explicitly, and point
  the adapter at it with `CLAUDE_CODE_EXECUTABLE`:

  ```sh
  #!/bin/sh
  exec /nix/store/…-glibc/lib/ld-linux-x86-64.so.2 \
    ~/.npm/_npx/…/claude-agent-sdk-linux-x64/claude "$@"
  ```
