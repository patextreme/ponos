--!strict
-- Per-session model fan-out: one agent, two sessions, two models.
-- Configure each session with a setConfig call right after creation —
-- before the first prompt. There is no constructor `config` option: a
-- config table cannot express application order, which matters for
-- agents with dependent options (e.g. opencode re-derives `effort`
-- whenever `model` is set), so config is applied with ordered
-- setConfig calls instead — driving options (like `model`) first. The
-- process-level alternative (env vars in the registry) cannot vary per
-- session. Config option ids and value ids are agent-defined: inspect
-- them at runtime with `s:configOptions()` (the mock agent used here
-- offers a `model` select option and echoes the current value).
local agent = ponos.agent("demo")

local reviewer = agent:session({ id = "opus" })
reviewer:setConfig("model", "opus")
local summarizer = agent:session({ id = "haiku" })
summarizer:setConfig("model", "haiku")

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
