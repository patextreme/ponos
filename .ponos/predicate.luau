--!strict

export type PredicateOptions = {
  sessionId: string,
  model: string,
}

-- Compose the multiline predicate prompt. The prose lives in a single
-- template string (%s marks each injection point), so it reads top to
-- bottom and edits never touch concatenation plumbing.
local function buildPrompt(predicate: string, payload: string): string
  return string.format(
    [[
Review if the following predicate is true in the payload. Submit the predicate result to the ponos submit tool.

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
local function predicateAgent(predicate: string, payload: string, options: PredicateOptions): boolean
  local oc: Agent = ponos.agent("opencode")

  local verdict
  repeat
    local session: Session = oc:session({
      id = options.sessionId,
      result = { type = "boolean" },
    })
    session:setConfig("model", options.model)
    local promptResult = session:prompt(buildPrompt(predicate, payload))
    verdict = promptResult.result :: boolean
  until verdict ~= nil
  return verdict
end

return predicateAgent
