--!strict

local oc: Agent = ptah.agent("opencode")

export type PredicateOptions = {
	sessionId: string,
	model: string,
}

local MAX_ATTEMPTS = 3

local function buildPrompt(predicate: string, payload: string): string
	return string.format(
		[[
Review if the following predicate is true in the payload, then submit the verdict as the JSON boolean true or false by calling this session's ptah result submit tool.

---

Predicate:

%s

---

Payload:
%s

]],
		predicate,
		payload
	)
end

-- Ask a small/fast agent whether `predicate` holds for `payload`.
-- Returns the verdict once the agent responds with a typed boolean.
-- A judge that never submits would hang the caller forever, so retries
-- are capped and exhaustion is a script error.
local function predicateAgent(predicate: string, payload: string, options: PredicateOptions): boolean
	for attempt = 1, MAX_ATTEMPTS do
		local session: Session = oc:session({
			id = options.sessionId,
			resultSchema = { type = "boolean" },
		})
		session:setConfig("model", options.model)
		local promptResult = session:prompt(buildPrompt(predicate, payload))
		local verdict = promptResult.result :: boolean?
		if verdict ~= nil then
			return verdict
		end
		ptah.log(`judge {options.sessionId} submitted no verdict (attempt {attempt}/{MAX_ATTEMPTS})`)
	end
	return error(`judge never submitted a typed boolean verdict after {MAX_ATTEMPTS} attempts`)
end

return predicateAgent
