--!strict
-- Fan-out with a concurrency cap: at most 2 reviewers in flight.
-- Each lane owns its session (own subprocess): turns on a single session
-- serialize, so a session per lane is what makes turns overlap — the
-- cap bounds concurrent agent processes, not turn requests.
local agent = ponos.agent("demo")

local targets = { "alpha", "beta", "gamma", "delta", "epsilon" }
local outcomes = ponos.parallel(targets, function(target)
	local s = agent:session({ id = "rev-" .. target })
	local r = s:prompt(("Summarize the changes in %s."):format(target))
	s:close()
	return ("%s: %s"):format(target, tostring(r))
end, { concurrency = 2 })

for i, entry in ipairs(outcomes) do
	if entry.ok then
		ponos.log(entry.value)
	else
		ponos.log(("FAILED %d: %s"):format(i, entry.error))
	end
end
