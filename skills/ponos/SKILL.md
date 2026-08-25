---
name: ponos
description: Author, validate, and run Luau automation scripts for ponos — the CLI that drives ACP-speaking AI agents (Claude Code, Gemini CLI, Codex, …) headlessly from sandboxed Luau. Use this skill whenever you are writing or editing a .luau script that uses the `ponos` global (ponos.agent, session:prompt, ponos.parallel, ponos.spawn, resultSchema, configOptions, setConfig), configuring the agent registry (.ponos/config.toml), running or debugging `ponos run` / `ponos check` / `ponos types`, or whenever the user wants to orchestrate, fan out, pipeline, or watchdog multiple AI agents as code. Also trigger when debugging ponos script errors, exit codes, timeouts, cancels, typed results, or concurrency behavior.
---

# ponos scripting

`ponos run script.luau` executes a Luau script that drives any
ACP-speaking agent over stdio. Scripts read as plain synchronous code —
`reply = session:prompt("…")` blocks the script, not the runtime — while
the runtime multiplexes subprocesses underneath. Each `agent:session()`
owns its own agent subprocess; closing a session reaps the process.

- Source repo (deep investigation): **https://github.com/patextreme/ponos**
  - `README.md` — the full API/behavior truth. Read it first when this
    skill's summary is not enough.
  - `.ponos/ponos.d.luau` — the script API type definitions, embedded
    verbatim in the binary; the single source of truth for shapes.
  - `examples/*.luau` — vetted scripts (sequential, fan-out, model
    fan-out, watchdog, typed results).
  - `src/script/` (Luau runtime + sandbox), `src/acp/` (ACP client),
    `src/check*` (`check` pipeline), `src/bin/mock-agent/` (scriptable
    test agent used by the offline test suite).
- `ponos types` prints the type definitions matching the *installed*
  binary — no registry or agents needed. `ponos --version` confirms the
  binary exists.

## Workflow

1. **Confirm the toolchain**: `ponos --version`. No binary → build from
   the repo (`nix build`, or `cargo build` in its dev shell).
2. **Discover available agents**: read `.ponos/config.toml` walking up
   from the directory `ponos` will be invoked in, then
   `~/.config/ponos/config.toml`. Project entries override user entries
   per agent *name*. If no registry fits, either write one or pass an
   inline spec (see [Registry](#registry)).
3. **Write the script**: first line `--!strict`. Sandbox rules below.
4. **Validate without executing**: `ponos check script.luau`. Fix every
   finding (details below). Exit `2` means the check itself could not run
   (missing script, registry discovery failure, or no `luau-lsp` on PATH) —
   fix that environment problem first.
5. **Run**: `ponos run script.luau`. Diagnose with `--verbose`
   (lifecycle), `-vv` (also forwards agent subprocess stderr),
   `--no-color` (plain text), `--quiet` (suppress rendering; script
   `print` still passes through).
6. Optional, when luau-lsp is available for editor/type support:
   `ponos types > ponos.d.luau` and configure luau-lsp with platform
   `standard`. Definitions are workspace-wide — keep the file out of
   mixed Luau projects not run under ponos.

## Script rules (the sandbox)

Scripts run in a curated Luau environment. Available: `string`, `table`,
`math`, `utf8`, `bit32`, `buffer`, `os.time`, `os.clock`, `print`, the
standard Luau base library (`pcall`, `error`, `assert`, `tostring`,
`tonumber`, `pairs`, `ipairs`, `select`, `type`/`typeof`,
`setmetatable`/`getmetatable`, `rawget`/`rawset`, …), and a restricted
`coroutine` table with only `yield` (the async runtime needs it). There is
**no** file I/O, no `io`, no subprocesses, no network, no
`debug`, no `loadstring`/`collectgarbage`. Do not fight this —
orchestration logic belongs in the script; anything touching the machine
belongs in the *agent's* prompt or the agent registry.

`require` resolves `.luau` modules relative to the requiring file
(`foo.luau`, `foo.lua`, `foo/init.luau`, `foo/init.lua`) and rejects
paths escaping the script tree. `--!strict` is enforced by `ponos check`
on the entry and every reachable file.

## API reference

### `ponos` namespace

| API | Description |
| --- | --- |
| `ponos.agent(name_or_spec)` | Agent factory. `name` resolves against the registry (raises `unknown agent \`name\`…` at this call when missing); inline spec `{ command =, args = {…}, env = {…} }` skips the registry. `${VAR}` in inline `env` values interpolates from ponos's environment. |
| `agent:session(opts?)` | New session (own subprocess). Returns an `Agent`-scoped handle; `opts` below. |
| `session:prompt(text, { timeoutMs = n }?)` | One turn → `PromptResult` (below). **Timeout raises a catchable Lua error** after sending a cancel — `pcall` it if you need to survive. |
| `session:cancel()` | Cancel the in-flight turn; the blocked `prompt` returns normally with `stopReason = "cancelled"`. |
| `session:close()` | End session, reap the process. |
| `session:label()` | `"agentName/sessionId"` string (a method — call with `:`). Handy for logs. |
| `session:configOptions()` | Live per-session config options (see [Config](#per-session-config-models-etc)). |
| `session:setConfig(id, value)` | Set a config option between turns; raises a catchable error carrying the id + agent message on rejection. |
| `ponos.spawn(fn)` → `task:await()` | Concurrent task; errors re-raise at the await site. |
| `ponos.join({task, …})` | Wait for tasks → per-task outcome entries. |
| `ponos.parallel(items, fn, { concurrency = n }?)` | Fan-out (default unlimited) → per-item outcome entries in item order. |
| `ponos.sleep(ms)` / `ponos.log(msg)` / `ponos.exit(code)` / `ponos.version` | Helpers. `log` renders with a `[ponos]` prefix; `exit` terminates the process with code `n`. |

Session options (`agent:session({...})`; all optional):

- `id` — session label; defaults to `s1, s2, …` per agent.
- `cwd` — working dir for the agent subprocess; defaults to the
  invocation directory.
- `mcpServers` — suggested MCP servers: `{ type = "stdio", name =,
  command =, args =, env = }` or `{ type = "http", name =, url =,
  headers = }`.
- `resultSchema` — typed-result contract (see below).

(No `config` option exists — it was removed. Passing `config = { … }`
raises a catchable error naming `setConfig` as the replacement, before
any agent subprocess spawns.)

`PromptResult`:

- `text` — the turn's **last agent message** (final contiguous text run;
  tool activity ends a run; falls back to the previous non-empty run if
  the turn ends on tool activity; `""` for cancelled turns). Narration
  streams to the terminal but does not pollute `r.text`. `tostring(r)`
  is `r.text` at runtime and type-checks fine (`tostring` takes `any`);
  the sugar itself is not covered by the type definitions, so write
  `r.text` explicitly wherever the checker expects a `string` — implicit
  coercion (`r .. ""`, a `string`-typed binding) is a type error.
- `stopReason` — typed plain `string` (the checker won't enforce
  exhaustiveness), but a script only ever observes these five values:
  `"end_turn"`, `"max_tokens"`, `"max_turn_requests"`, `"refusal"`,
  `"cancelled"`. ponos normalizes any unrecognized reason to
  `"end_turn"` at the ACP boundary.
- `usage` — `{ input, cacheRead, cacheWrite, output }` (zeros when
  unreported).
- `result` — the turn's typed submission, or `nil` (see below).

## Semantics that bite

- **One turn at a time per session.** Concurrent `prompt` calls on the
  same session serialize behind a turn lock — they queue, they do not
  overlap. For real parallel turns, create one session per concurrent
  lane (see the pool pattern below).
- **Headless permissions.** Nobody is there to answer prompts, so ponos
  auto-allows every permission request (first `AllowAlways`, else the
  first allow option). Everything else an agent may ask of a client gets
  method-not-found — turns never hang. Note `AllowAlways` can persist an
  allow rule in the agent's own config beyond the run.
- **Timeout vs cancel.** `timeoutMs` expiry sends a cancel, then *raises*
  a Lua error. `session:cancel()` makes the prompt *return* with
  `stopReason = "cancelled"` (`text` is `""`, `result` discarded).
- **Exit codes are a contract.** `0` success; `1` uncaught script error,
  a never-observed task error (a failed task nobody awaited), or `check`
  findings; `2` CLI usage errors (and "check could not run"); `n` when
  the script calls `ponos.exit(n)`.
- **Un-awaited task errors still fail the run** (exit 1, reported to
  stderr). Always `await()`/`join()` your tasks, or handle their outcome
  entries.

## Patterns

Sequential turns (state accumulates across prompts in one session):

```lua
--!strict
local agent = ponos.agent("claude")
local s = agent:session({ id = "reviewer" })
for _, file in ipairs({ "src/a.rs", "src/b.rs" }) do
    local r = s:prompt(("Review %s for obvious bugs; be terse."):format(file))
    ponos.log(("%s -> %s"):format(file, r.text))
end
s:close()
```

Parallel fan-out with a session pool (concurrency cap = pool size; one
session per lane so turns genuinely overlap):

```lua
--!strict
local agent = ponos.agent("claude")
local targets = { "alpha", "beta", "gamma", "delta", "epsilon" }

local outcomes = ponos.parallel(
    targets,
    function(target: string)
        -- one session per item: parallel turns, isolated context
        local s = agent:session({ id = "rev-" .. target })
        local r = s:prompt(("Summarize the changes in %s."):format(target))
        s:close()
        return ("%s: %s"):format(target, tostring(r))
    end,
    { concurrency = 2 }
)

for i, entry in ipairs(outcomes) do
    if entry.ok then
        ponos.log(entry.value)
    else
        ponos.log(("FAILED %d: %s"):format(i, entry.error))
    end
end
```

Typed results — declare a JSON Schema, then nil-check retry (degradation
is designed: agents may ignore the submit tool, sandboxes may block it;
the turn still completes normally with `result = nil`):

```lua
--!strict
local s = ponos.agent("claude"):session({
    id = "reviewer",
    resultSchema = {
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

Watchdog cancel (a slow turn returns `stopReason = "cancelled"`):

```lua
--!strict
local agent = ponos.agent("claude")
local s = agent:session({ id = "slow" })

local work = ponos.spawn(function()
    return s:prompt("Take your time on this one").stopReason
end)
ponos.sleep(30_000)
s:cancel()
assert(work:await() == "cancelled")
s:close()
```

## Typed results, details

`resultSchema` injects one extra MCP server named `ponos` exposing a
single `result_submit` tool (agents that derive tool names call it
`mcp__ponos__result_submit`). The schema travels in the tool's
description/value — ponos never modifies your prompt text. Submissions
are validated against the schema; violations go back as a tool error the
agent can fix *inside the same turn*. Last accepted submission wins;
cancelled/timed-out turns discard it.

- Schemas compile eagerly at `session()`: an invalid schema or a remote
  `$ref` raises at your call site **before** any subprocess spawns.
- Any root shape works — `{ type = "string", enum = { "ship", "block" } }`
  submits plain strings. JSON `null` arrives as `nil`.
- Luau→JSON caveat: an empty table `{}` serializes as an *object*, not an
  array — write non-empty arrays (`enum = { "a" }`) or explicit array shapes.
- Agents that don't submit on their own may need the prompt to say so.

## Per-session config (models, etc.)

Agents expose per-session config (above all the *model*) via ACP config
options. Apply with `setConfig` calls immediately after `session()`
returns — before the first prompt — enumerate with `configOptions()`:

```lua
--!strict
local agent = ponos.agent("claude")
local reviewer = agent:session({ id = "rev" })
reviewer:setConfig("model", "claude-opus-4-5") -- before the first prompt
reviewer:setConfig("model", "claude-haiku-4-5") -- or between turns
for _, o in ipairs(reviewer:configOptions()) do
    ponos.log(("%s = %s"):format(o.id, tostring(o.currentValue)))
end
reviewer:close()
```

**Sequencing matters and is yours to control.** Some agents re-derive
dependent options when a driving option is set (opencode resets
`effort` whenever `model` is set), so set driving options first and
dependent options last — each awaited `setConfig` sees the state the
previous one returned. There is no constructor `config` table exactly
because a Luau table cannot express that order (`pairs()` order is
unspecified); passing one raises before any subprocess spawns.

**Option ids and value ids are agent-defined** — `"model"` belongs to the
agent you drive; never assume a hardcoded id exists. Enumerate
`configOptions()` first. Entries carry `id`, `name`, `type`
(`"select"` | `"boolean"`), `currentValue`, optional `category` (UX hint
only), and for selects an `options` array of `{ id, name, description? }`.
`setConfig` accepts strings (choice ids) or booleans, and is serialized
with turns — a call issued mid-turn waits, so changes apply strictly
between turns. On rejection it raises a catchable error naming the id
and carrying the agent's message; the session stays open (configure
right after creation, and close or let the run-end sweep reap on
error).

## Registry

TOML; project `.ponos/config.toml` (found upward from the invocation
directory) overrides `~/.config/ponos/config.toml` **per agent name**:

```toml
[agents.claude]
command = "npx"
args = ["-y", "@agentclientprotocol/claude-agent-acp@latest"]
env = { ANTHROPIC_API_KEY = "${ANTHROPIC_API_KEY}" }
```

`${VAR}` interpolates from ponos's environment at resolve time (unset →
empty); `env` merges over the inherited environment. Process-level env
cannot vary per session — per-session model fan-out is exactly what
`setConfig` is for.

## `ponos check` and pre-flight

`ponos check script.luau` verifies a script **without executing it** —
nothing runs, nothing spawns. Three passes, findings collected together:

1. **Compile** — same compiler `run` uses (compiled, never called).
2. **Static lints** — full-moon walk over the entry and every file
   reachable through *literal* `require("...")` strings: unknown literal
   `ponos.agent("name")` names against the discovered registry; literal
   require targets that don't resolve; missing `--!strict`. Computed
   names/paths are not linted.
3. **Typecheck** — `luau-lsp analyze` against the embedded definitions;
   must be on PATH or the check hard-fails (exit 2).

Exit `0` clean · `1` findings (warnings like `LocalUnused` don't fail) ·
`2` could not run. `ponos run` also pre-flights (compile + literal
require + literal agent-name) and fails with exit 1 **before the first
agent spawns** — a literal require on a dead code path still fails it,
so delete dead requires.

## Pitfall checklist

- Missing `--!strict` on the entry (or a required module) → check finding.
- Implicit `PromptResult`→`string` coercion under `--!strict` → type
  error — write `r.text` (details under `PromptResult` above).
- Sharing one session across `parallel` workers → turns silently serialize
  (turn lock); pool sessions instead.
- `ponos.parallel` callback losing the item type → annotate the parameter
  (`function(item: string)`).
- Outcome narrowing works on locals — bind `local entry = outcomes[i]`
  (or a `for` variable) before `if entry.ok then entry.value …`.
- Typo'd *option* keys in a session-options literal are **not** flagged
  by luau-lsp — double-check `id`/`cwd`/`mcpServers`/`resultSchema`
  spelling by hand (invented result fields like `r.txt` *are* flagged).
- Setting a dependent option before its driving option (e.g. `effort`
  before `model` on opencode) — the agent's re-derivation silently
  reverts it; order your `setConfig` calls: driving options first.
- Empty Luau table `{}` in a schema means JSON object, not array.
- Unknown literal agent name → check finding and run pre-flight failure;
  registry miss at runtime raises at the `ponos.agent(...)` call.
- Expecting a value from `result` without a nil-check retry loop —
  degradation to `nil` is designed behavior, not an error.
