--!strict

local oc: Agent = ponos.agent("opencode")

export type PredicateOptions = {
	sessionId: string,
	model: string,
}

-- Agents surface the injected submit tool under different names
-- (`mcp__ponos__result_submit`, `ponos_result_submit`, ...), so the
-- prompt describes it by the stable `result_submit` substring.
local MAX_ATTEMPTS = 3

-- Compose the multiline predicate prompt. The prose lives in a single
-- template string (%s marks each injection point), so it reads top to
-- bottom and edits never touch concatenation plumbing.
local function buildPrompt(predicate: string, payload: string): string
	return string.format(
		[[
Review if the following predicate is true in the payload, then submit the verdict as the JSON boolean true or false by calling this session's result submit tool (its name contains "result_submit"; the value must be a boolean).

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
			config = { model = options.model },
			resultSchema = { type = "boolean" },
		})
		local promptResult = session:prompt(buildPrompt(predicate, payload))
		local verdict = promptResult.result :: boolean
		if verdict ~= nil then
			return verdict
		end
		ponos.log(`judge {options.sessionId} submitted no verdict (attempt {attempt}/{MAX_ATTEMPTS})`)
	end
	return error(`judge never submitted a typed boolean verdict after {MAX_ATTEMPTS} attempts`)
end

return predicateAgent
