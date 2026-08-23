--!strict
-- Per-session model fan-out: one agent, two sessions, two models.
-- The `config` session option pins each session's model at creation
-- (one setConfig per entry, applied before the constructor returns) —
-- the process-level alternative (env vars in the registry) cannot vary
-- per session. Config option ids and value ids are agent-defined:
-- inspect them at runtime with `s:configOptions()` (the mock agent used
-- here offers a `model` select option and echoes the current value).
local agent = ponos.agent("demo")

local reviewer = agent:session({ id = "opus", config = { model = "opus" } })
local summarizer = agent:session({ id = "haiku", config = { model = "haiku" } })

local outcomes = ponos.parallel(
	{ reviewer, summarizer },
	function(s: Session)
		local r = s:prompt("Which model are you running under? One word.")
		return ("%s → %s"):format(s:label(), tostring(r))
	end,
	{ concurrency = 2 }
)

for _, entry in ipairs(outcomes) do
	if entry.ok then
		ponos.log(entry.value)
	else
		ponos.log("FAILED: " .. entry.error)
	end
end

reviewer:close()
summarizer:close()
