--!strict
-- Fan-out with a concurrency cap: at most 2 reviewers in flight.
local agent = ponos.agent("demo")
local s = agent:session({ id = "pool" })

local targets = { "alpha", "beta", "gamma", "delta", "epsilon" }
local outcomes = ponos.parallel(
    targets,
    function(target)
        local r = s:prompt(("Summarize the changes in %s."):format(target))
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

s:close()
