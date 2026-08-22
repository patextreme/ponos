--!strict
-- Typed results: a review turn whose verdict arrives as a validated,
-- structured value instead of prose to parse. Declaring
-- `result = <JSON Schema>` on the session injects a `result_submit` tool
-- into the agent; each `prompt` outcome then carries `r.result` — the last
-- accepted (schema-valid) submission, or nil if the turn ended without
-- one.
local schema = require("./typed_results_schema")

local agent = ponos.agent("demo")
local s = agent:session({ id = "reviewer", result = schema.review })

-- Nil-check retry loop: a turn that ends without an accepted submission
-- yields `r.result == nil` (the agent ignored the tool, was sandboxed away
-- from it, or the turn was cancelled). Ask again — bounded.
local verdict
for attempt = 1, 3 do
	local r = s:prompt(("Review the diff for obvious bugs; be terse. (attempt %d)"):format(attempt))
	if r.result ~= nil then
		verdict = r.result
		break
	end
	ponos.log(("attempt %d ended without a typed result; retrying"):format(attempt))
end

assert(verdict ~= nil, "no typed result after 3 attempts")
ponos.log(("verdict=%s score=%d"):format(verdict.verdict, verdict.score))
s:close()
