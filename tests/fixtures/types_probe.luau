--!strict
--
-- Runtime probe for `types/ponos.d.luau`: exercises every member, method,
-- and field the definitions promise, against the mock agent. Wired as a
-- cargo test (tests/types.rs substitutes @MOCK_BIN@ with the mock binary)
-- and analyzed by the `ponos-analyze` nix check. If a member documented in
-- the definitions is removed or renamed in the runtime, this script fails.

-- ponos.agent(name) → agent:session(SessionOptions: id, cwd, mcp_servers)
local agent = ponos.agent("demo")
local servers: { McpServer } = {
	{
		type = "stdio",
		name = "ctx",
		command = "/bin/true",
		args = { "--flag" },
		env = { { name = "K", value = "V" } },
	},
	{
		type = "http",
		name = "remote",
		url = "http://127.0.0.1:1/mcp",
		headers = { { name = "H", value = "V" } },
	},
}
local s = agent:session({ id = "rev", cwd = ".", mcp_servers = servers })

-- session:label() is a method (not a property) at runtime.
assert(s:label() == "demo/rev", "label: " .. s:label())

-- prompt result: text, stop_reason, usage table (mock MOCK_USAGE=5,10,2,3),
-- prompt options: timeout_ms.
local r = s:prompt("review this", { timeout_ms = 5000 })
assert(r.text == "review this", "text: " .. r.text)
assert(r.stop_reason == "end_turn", "stop_reason: " .. r.stop_reason)
assert(r.usage.input == 2, "usage.input: " .. tostring(r.usage.input))
assert(r.usage.output == 3, "usage.output: " .. tostring(r.usage.output))
assert(r.usage.cache_read == 0, "usage.cache_read: " .. tostring(r.usage.cache_read))
assert(r.usage.cache_write == 0, "usage.cache_write: " .. tostring(r.usage.cache_write))
assert(tostring(r) == r.text, "tostring sugar: " .. tostring(r))

-- ponos.agent(AgentSpec: command, args, env) — registry-free.
local inline = ponos.agent({
	command = "@MOCK_BIN@",
	args = {},
	env = { MOCK_CHUNKS = "inline reply" },
})
local si = inline:session()
local ri = si:prompt("ignored")
assert(ri.text == "inline reply", "inline text: " .. ri.text)
si:close()

-- Typed results (SessionOptions.result / PromptResult.result): a result
-- session whose mock agent submits through the injected MCP bridge; the
-- outcome's `result` field carries the accepted submission end to end.
local result_agent = ponos.agent({
	command = "@MOCK_BIN@",
	args = {},
	env = { MOCK_SUBMIT = '{"verdict":"approve"}' },
})
local sr = result_agent:session({
	result = {
		type = "object",
		properties = { verdict = { type = "string" } },
		required = { "verdict" },
	},
})
local rr = sr:prompt("review")
assert(rr.result ~= nil, "result submitted")
assert(rr.result.verdict == "approve", "result.verdict: " .. tostring(rr.result.verdict))
assert(rr.stop_reason == "end_turn", "result stop_reason: " .. rr.stop_reason)
sr:close()

-- session:cancel() on a hanging turn → stop_reason == "cancelled".
local hang = ponos.agent("demo-hang")
local sh = hang:session()
local hanging = ponos.spawn(function()
	return sh:prompt("take your time").stop_reason
end)
ponos.sleep(100)
sh:cancel()
local cancelled = hanging:await()
assert(cancelled == "cancelled", "cancelled: " .. tostring(cancelled))
sh:close()

-- ponos.spawn → Task:await().
local task = ponos.spawn(function()
	ponos.sleep(10)
	return 41 + 1
end)
local awaited = task:await()
assert(awaited == 42, "await: " .. tostring(awaited))

-- ponos.join: ok and error entries, in order.
local ok_task = ponos.spawn(function()
	return "joined"
end)
local bad_task = ponos.spawn(function()
	error("join boom", 0)
end)
local joined = ponos.join({ ok_task, bad_task })
local e1 = joined[1]
if e1.ok then
	assert(e1.value == "joined", "join value: " .. e1.value)
else
	error("join[1] should be ok: " .. e1.error)
end
local e2 = joined[2]
if e2.ok then
	error("join[2] should have failed: " .. tostring(e2.value))
else
	assert(e2.error:find("join boom", 1, true) ~= nil, "join error: " .. e2.error)
end

-- ponos.map with concurrency: ok and error entries, in item order.
local outcomes = ponos.map({ "alpha", "beta" }, function(item)
	if item == "beta" then
		error("bad item", 0)
	end
	return item:upper()
end, { concurrency = 2 })
assert(#outcomes == 2, "map count: " .. tostring(#outcomes))
local m1 = outcomes[1]
if m1.ok then
	assert(m1.value == "ALPHA", "map value: " .. m1.value)
else
	error("map[1] should be ok: " .. m1.error)
end
local m2 = outcomes[2]
if m2.ok then
	error("map[2] should have failed: " .. tostring(m2.value))
else
	assert(m2.error:find("bad item", 1, true) ~= nil, "map error: " .. m2.error)
end

-- ponos.log, ponos.version.
assert(type(ponos.version) == "string" and #ponos.version > 0, "version")
ponos.log("probe complete: " .. ponos.version)

s:close()
ponos.exit(0)
