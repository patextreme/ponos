--!strict
local schema = {
	type = "object",
	properties = {
		verdict = { type = "string", enum = { "approve", "block" } },
		score = { type = "integer", minimum = 0, maximum = 10 },
	},
	required = { "verdict", "score" },
}

local agent = ptah.agent("demo")
local s = agent:session({ id = "reviewer", resultSchema = schema })

-- Nil-check retry loop: a turn that ends without an accepted submission
-- yields `r.result == nil` (the agent ignored the tool, was sandboxed away
-- from it, or the turn was cancelled; a session that declared no
-- `resultSchema` contract yields nil every turn). Ask again — bounded.
local verdict
for attempt = 1, 3 do
	local r = s:prompt(("Review the diff for obvious bugs; be terse. (attempt %d)"):format(attempt))
	if r.result ~= nil then
		verdict = r.result
		break
	end
	ptah.log(("attempt %d ended without a typed result; retrying"):format(attempt))
end

assert(verdict ~= nil, "no typed result after 3 attempts")
ptah.log(("verdict=%s score=%d"):format(verdict.verdict, verdict.score))
s:close()
