# ponos

> **Name origin:** *Ponos* (Πόνος) is the Greek spirit of toil, labor, and
> drudgery. Nothing here escapes that fate: `ponos` turns the mechanical task
> of driving AI agents into plain, scripted code — the runtime does the heavy
> lifting, and your agents carry the load.

Luau-scripted multi-agent orchestration over the
[Agent Client Protocol](https://agentclientprotocol.com/) (ACP).

`ponos run script.luau` executes a small, versionable Luau script that drives
any ACP-speaking agent (Claude Code, Gemini CLI, Codex, …) over stdio.
Scripts look synchronous — `reply = session:prompt("…")` blocks the script,
not the runtime — so fan-outs, pipelines and watchdogs read like plain code.

```lua
--!strict
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
ponos types
ponos --version
```

- `--quiet` — suppress streaming render and diagnostics (script `print` still passes)
- `--verbose` — runtime lifecycle diagnostics
- `-vv` — additionally pass agent subprocess stderr through
- `--no-color` — drop ANSI colors, keep `[agent/session]` text prefixes
- `ponos types` — print the Luau type definitions for the script API
  (see [Editor setup](#editor-setup)); needs no registry, script, or agents

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

## Permissions (headless posture)

`ponos` runs headless — nobody is there to be asked — so it answers every
`session/request_permission` by selecting an allow option the agent offered:
the first `AllowAlways` when one is offered, otherwise the first other
allow option (e.g. `AllowOnce`). A denied tool silently degrades output, so
allowing is the sane default for scripted runs; note that choosing
`AllowAlways` may persist an allow rule in the agent's own configuration
beyond the run (usually desirable for CI). When an offer contains no allow
option at all, ponos responds with an unsupported-method error. Everything
else agents may ask of a client — file access, terminal control,
elicitation — is answered with a JSON-RPC method-not-found error, so turns
never hang.

## The `ponos` namespace

| API | Description |
| --- | --- |
| `ponos.agent(name_or_spec)` | Agent factory (registry name or inline `{command=, args=, env=}` spec) |
| `agent:session({id=, cwd=, mcp_servers=, result=})` | New session (own subprocess); `id` defaults to `s1, s2, …`; `result` declares a typed-result contract (see below) |
| `session:prompt(text, {timeout_ms=})` | One turn → `{ text, stop_reason, usage, result }` (`__tostring` → text) |
| `session:cancel()` | Cancels the in-flight turn (returns `stop_reason = "cancelled"`) |
| `session:close()` | Ends the session and reaps the agent process |
| `ponos.spawn(fn)` → `task:await()` | Concurrent task; errors re-raise at the await site |
| `ponos.join({task, …})` | Wait for tasks → per-task `{ok, value}` / `{ok=false, error}` entries |
| `ponos.map(items, fn, {concurrency=})` | Fan-out (default unlimited) → per-item outcome entries |
| `ponos.sleep(ms)` / `ponos.log(msg)` / `ponos.exit(code)` / `ponos.version` | Runtime helpers |

### Typed results

`agent:session({ result = <schema> })` declares a typed result contract:
a JSON Schema as a plain Luau table. ponos then

- injects one extra MCP server into the agent's session, named `ponos`,
  exposing a single tool `result_submit` (agents that derive tool names
  call it `mcp__ponos__result_submit`). The declared schema travels in the
  tool's `value` argument — it never enters prompt text;
- appends one fixed sentence to each prompt telling the agent to submit
  its final result by calling the tool;
- validates each submission against the schema and reports violations back
  as a tool error the agent can see and fix *inside the same turn* — the
  retry loop that makes typed results reliable.

`session:prompt()` outcomes gain a `result` field: the turn's last accepted
submission converted to a Luau value (`tables`, `strings`, `numbers`,
`booleans`; JSON `null` arrives as `nil`), or `nil` when the turn had no
accepted submission. Last submission wins; a fresh turn starts with an
empty slot; cancelled and timed-out turns discard what they had gathered.

```lua
--!strict
local agent = ponos.agent("claude")
local s = agent:session({
    id = "reviewer",
    result = {
        type = "object",
        properties = {
            verdict = { type = "string", enum = { "approve", "block" } },
            score = { type = "integer", minimum = 0, maximum = 10 },
        },
        required = { "verdict", "score" },
    },
})

for attempt = 1, 3 do
    local r = s:prompt("Review the diff; be terse.")
    if r.result ~= nil then
        ponos.log(("%s (%d/10)"):format(r.result.verdict, r.result.score))
        break
    end
    ponos.log("no typed result; retrying")
end
s:close()
```

Schemas compile eagerly at `session()`: an invalid schema (or a remote
`$ref`, which is rejected so runs stay offline) raises a Lua error at your
call site before any subprocess spawns. Any root schema shape works — a
non-object schema such as `{ type = "string", enum = { "ship", "block" } }`
submits plain strings.

**Luau ↔ JSON notes.** The schema table converts from Luau to JSON with the
usual Lua caveats: an empty table `{}` serializes as an *object*, not an
array (write `enum = { "a" }` style non-empty arrays, or use explicit
array shapes), and JSON `integer`/`number` both arrive as Luau numbers.
JSON `null` in a submitted value arrives as `nil` (an explicitly submitted
`null` is indistinguishable from no submission).

**Degradation is designed, not exceptional.** Agents are free to ignore
suggested MCP servers, and sandboxes may block spawning the ponos binary.
In every such case prompts complete normally with `result = nil`, plus one
lifecycle log line (`--verbose`) noting the session ran without typed
results. Never an error, never a hang. Scripts that must have a value
write a nil-check retry loop, as above.

Scripts run in a sandboxed Luau environment: `string`, `table`, `math`,
`utf8`, `bit32`, `buffer`, `os.time`, `os.clock`, and `print` — no file I/O,
subprocesses, network, or debug facilities. `require` resolves `.luau`
modules relative to the requiring file and rejects paths escaping the script
tree. (One deviation: a restricted `coroutine` table containing only `yield`
remains visible because the embedded async runtime needs it; the scheduling
primitives are absent.)

## Editor setup

Scripts get completion, hover, and type checking — plus sandbox violations
flagged before a run — by pointing [luau-lsp](https://github.com/luau-lsp/luau-lsp)
at ponos's type definitions:

```sh
ponos types > ponos.d.luau
```

`ponos types` emits definitions version-matched to the installed binary: a
`-- ponos <version> type definitions` header followed by the file
byte-for-byte. Start scripts with `--!strict` for full checking (the
[bundled examples](examples/) do). The definitions also model the sandbox —
`os` trimmed to `time`/`clock`, `coroutine` to `yield`, and
`loadstring`/`collectgarbage` unavailable — so editor-approved code cannot
reach a global the runtime poisons. Definitions apply workspace-wide, so
keep them out of mixed Luau projects you don't run under ponos.

Helix needs no per-user setup: the repo ships `.helix/languages.toml`,
which points luau-lsp at `types/ponos.d.luau` (standard platform) for
any file in this workspace. Other editors — configure your own (VS Code
luau-lsp extension settings; "standard" platform, not
Roblox):

```jsonc
{
  "luau-lsp.platform.type": "standard",
  "luau-lsp.types.definitionFiles": ["ponos.d.luau"]
}
```

Neovim (nvim-lspconfig equivalent):

```lua
require("lspconfig").luau_lsp.setup({
  settings = {
    ["luau-lsp"] = {
      platform = { type = "standard" },
      types = { definitionFiles = { "ponos.d.luau" } },
    },
  },
})
```

Known residuals of the definitions (none affect execution):

- generic `ponos.map` callbacks occasionally need an explicit parameter
  annotation (`function(item: string) …`) for the item type to propagate;
- the `tostring(r)` prompt-result sugar is not covered by the definitions —
  use `r.text` where the type checker wants a string;
- outcome narrowing (`if entry.ok then entry.value …`) works on locals —
  bind the outcome entry to a variable first;
- typo'd *option* keys in a session-options table literal are not flagged
  by current luau-lsp (a table-literal excess-key limitation) — invented
  *outcome* fields (`r.txt`) are flagged, double-check option names by hand;
- the require-tree restriction (no paths escaping the script directory) is
  enforced at runtime only.

## Examples

See [`examples/`](examples/) — sequential review, fan-out with a concurrency
cap, a watchdog cancel, and typed results with a retry loop — and run them
against the bundled mock agent:

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

- `src/bin/mock-agent/` — a scriptable ACP agent (with an MCP client for
  suggested servers) used by the offline test suite (`MOCK_CHUNKS`,
  `MOCK_HANG`, `MOCK_PERMISSION` (`once`/`always`/`reject`), `MOCK_TOOL`,
  `MOCK_PLAN`, `MOCK_USAGE`, `MOCK_STDERR`, `MOCK_DELAY_MS`, `MOCK_SUBMIT`,
  `MOCK_SUBMIT_BAD`, `MOCK_SUBMIT_ONCE`, `MOCK_NO_MCP`, `MOCK_ECHO_MCP`,
  `MOCK_MCP_LIST`, …).
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
