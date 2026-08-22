--!strict
-- Per-session model fan-out: one agent, two sessions, two models.
-- `setConfig("model", …)` picks each session's model before its first
-- prompt — the process-level alternative (env vars in the registry) cannot
-- vary per session. Config option ids and value ids are agent-defined:
-- inspect them at runtime with `s:configOptions()` (the mock agent used
-- here offers a `model` select option and echoes the current value).
local agent = ponos.agent("demo")

local reviewer = agent:session({ id = "opus" })
local summarizer = agent:session({ id = "haiku" })
reviewer:setConfig("model", "opus")
summarizer:setConfig("model", "haiku")

local outcomes = ponos.map(
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
