const SMALL_MODEL = "qwencloud/deepseek-v4-flash-0731"
const DEFAULT_MODEL = "zai/glm-5.3"
const CHANGE_NAME = "revise-script-api"

const PASS_RULE = [[
You will review the following review result from the openspec change reviewer.
If the review result has no blocker findings, please approve it by submitting `true` via the ponos submit-result MCP.
Otherwise, submit `false` as the result.
]]

local pi: Agent = ponos.agent("pi")
local oc: Agent = ponos.agent("opencode")

local function judgeReviewResult(iter: number, judgeRule: string, reviewText: string): boolean
  local verdict
  repeat
    local judge: Session = oc:session({
      id = `judge:{iter}`,
      result = { type = "boolean" }
    })
    judge:setConfig("model", SMALL_MODEL)
    local promptResult = judge:prompt(judgeRule .. "\n\n---\n\n" .. reviewText)
    verdict = promptResult.result :: boolean
  until verdict ~= nil
  return verdict
end

local iteration = 0
while true do
  local reviewer: Session = pi:session({ id = `reviewer:{iteration}` })
  reviewer:setConfig("model", DEFAULT_MODEL)

  local reviewResult = reviewer:prompt(`Review the openspec change named {CHANGE_NAME} using the openspec-review skill`)
  if judgeReviewResult(iteration, PASS_RULE, reviewResult.text) then
    ponos.log("The change is ready to implement")
    break
  end

  local needHumanResult = reviewer:prompt(`For the blocker findings, do you know how to resolve them or do you need human input`)
  if not judgeReviewResult(iteration, "Submit the result `true` to ponos if the result is resolvable without human input", needHumanResult.text) then
    ponos.log("The change needs human input to resolve the issue")
    break
  end

  local fixResult = reviewer:prompt("Go ahead and resolve the blockers")
  ponos.log("Fix result: " .. fixResult.text)

  iteration += 1
end
