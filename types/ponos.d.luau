--!strict
-- @meta
--
-- Type definitions for ponos scripts: the injected `ponos` global plus the
-- sandboxed environment the runtime provides. Load them into your editor's
-- luau-lsp (`ponos types > ponos.d.luau` — see the README "Editor setup"
-- section). This file is the single source of truth; the binary embeds it
-- verbatim. Written in plain `declare name: type` syntax (no `declare
-- class`, which newer Luau parsers reject); method-style fields take an
-- explicit `self` so colon calls (`s:prompt(…)`) type-check.

-- ---------------------------------------------------------------------------
-- Data tables
-- ---------------------------------------------------------------------------

-- Token counts reported for a completed turn (zero when unreported).
export type Usage = {
	input: number,
	cacheRead: number,
	cacheWrite: number,
	output: number,
}

-- The result of `session:prompt(text)`. `tostring(r)` returns `r.text` at
-- runtime via a metatable; definitions cannot express that sugar, so use
-- `r.text` explicitly when the type checker wants a string. `result` is
-- the turn's typed submission (see `SessionOptions.result`): a Luau value
-- converted from the accepted JSON, `nil` when the session declared no
-- contract or the turn had no accepted submission.
export type PromptResult = {
	text: string,
	stopReason: string,
	usage: Usage,
	result: any?,
}

export type EnvVariable = {
	name: string,
	value: string,
}

-- stdio MCP server (`mcpServers` entry, `{ type = "stdio", … }`).
export type McpServerStdio = {
	type: "stdio",
	name: string,
	command: string,
	args: { string }?,
	env: { EnvVariable }?,
}

-- http MCP server (`mcpServers` entry, `{ type = "http", … }`).
export type McpServerHttp = {
	type: "http",
	name: string,
	url: string,
	headers: { EnvVariable }?,
}

export type McpServer = McpServerStdio | McpServerHttp

-- Options for `agent:session(…)`. All fields optional; `id` defaults to
-- `s1, s2, …` per agent, `cwd` to the invocation directory. `result` is a
-- JSON Schema expressed as a plain Luau table: when set, the session gets
-- a typed-result contract — prompts carry a submit-tool instruction and
-- the outcome's `result` field carries the accepted submission.
export type SessionOptions = {
	id: string?,
	cwd: string?,
	mcpServers: { McpServer }?,
	result: { [string]: any }?,
}

-- Options for `session:prompt(text, …)`.
export type PromptOptions = {
	timeoutMs: number?,
}

-- Inline agent spec for `ponos.agent({ … })` (skips the registry).
export type AgentSpec = {
	command: string,
	args: { string }?,
	env: { [string]: string }?,
}

-- Options for `ponos.map(items, fn, …)`.
export type MapOptions = {
	concurrency: number?,
}

-- Per-item outcome of `ponos.map` / `ponos.join`: narrow on `entry.ok`
-- (bind the entry to a local first) to reach `value` or `error`.
export type Outcome<T> = {
	ok: true,
	value: T,
} | {
	ok: false,
	error: string,
}

-- Handle returned by `ponos.spawn(fn)`; `task:await()` yields the task's
-- first return value or re-raises its error at the await site.
export type Task<T> = {
	await: (self: Task<T>) -> T,
}

-- Agent factory from `ponos.agent(name_or_spec)`.
export type Agent = {
	session: (self: Agent, opts: SessionOptions?) -> Session,
}

-- One live agent session (own subprocess). `label` is a method, not a
-- property: call it as `session:label()`.
export type Session = {
	prompt: (self: Session, text: string, opts: PromptOptions?) -> PromptResult,
	cancel: (self: Session) -> (),
	label: (self: Session) -> string,
	close: (self: Session) -> (),
}

-- ---------------------------------------------------------------------------
-- The `ponos` global
-- ---------------------------------------------------------------------------

declare ponos: {
	agent: (spec: string | AgentSpec) -> Agent,
	spawn: <T>(f: () -> T) -> Task<T>,
	join: <T>(tasks: { Task<T> }) -> { Outcome<T> },
	map: <T, U>(items: { T }, fn: (item: T) -> U, opts: MapOptions?) -> { Outcome<U> },
	sleep: (ms: number) -> (),
	log: (msg: string) -> (),
	exit: (code: number?) -> (),
	version: string,
}

-- ---------------------------------------------------------------------------
-- Sandbox shadows: the runtime curates the stdlib; these declarations mirror
-- exactly what survives, so editor-approved code cannot reach a global that
-- would fail at runtime.
-- ---------------------------------------------------------------------------

declare os: {
	time: (t: {
		year: number,
		month: number,
		day: number,
		hour: number,
		min: number,
		sec: number,
		isdst: boolean?,
	}?) -> number,
	clock: () -> number,
}

declare coroutine: {
	yield: (...any) -> ...any,
}

declare loadstring: nil
declare collectgarbage: nil
