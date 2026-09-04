--!strict
-- Typed boolean judge (stdlib): ask a designated judge agent whether
-- a predicate holds for a payload, and read the verdict from the
-- session's typed boolean result.
--
-- Library contract (see ../README.md): data plus declared ptah runtime
-- handles, no writes, no relative-cwd dependence, requires stay inside
-- the tree.

export type PredicateOptions = {
	agent: Agent,
	sessionId: string,
	model: string?,
	maxAttempts: number?,
}

local DEFAULT_MAX_ATTEMPTS = 3

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

-- Ask a small/fast agent whether `predicateText` holds for `payload`.
-- Returns the verdict once the agent responds with a typed boolean.
-- A judge that never submits would hang the caller forever, so retries
-- are capped and exhaustion is a script error.
local function predicate(predicateText: string, payload: string, options: PredicateOptions): boolean
	local maxAttempts = if options.maxAttempts ~= nil then options.maxAttempts else DEFAULT_MAX_ATTEMPTS
	for attempt = 1, maxAttempts do
		local session: Session = options.agent:session({
			id = options.sessionId,
			resultSchema = { type = "boolean" },
		})
		if options.model ~= nil then
			session:setConfig("model", options.model)
		end
		local promptResult = session:prompt(buildPrompt(predicateText, payload))
		local verdict = promptResult.result :: boolean?
		session:close()
		if verdict ~= nil then
			return verdict
		end
		ptah.log(`judge {options.sessionId} submitted no verdict (attempt {attempt}/{maxAttempts})`)
	end
	return error(`judge {options.sessionId} never submitted a typed boolean verdict after {maxAttempts} attempts`)
end

return predicate
