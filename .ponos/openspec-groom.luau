const SMALL_MODEL = "qwencloud/deepseek-v4-flash-0731"
const DEFAULT_MODEL = "zai/glm-5.3"
const CHANGE_NAME = "add-check-subcommand"

const PASS_RULE = [[
You will review the following review result from the openspec change reviewer.
If the review result has no critical or blocker findings, please approve it by submitting `true` via the ponos submit-result MCP.
Otherwise, submit `false` as the result.
]]

const NEED_HUMAN_RULE = [[
You will review the following review result from the openspec change reviewer.
For each finding with critical or blocker severity, determine whether you can resolve it yourself or whether you need my input.
If you need my input, submit `true` via the ponos submit-result MCP; otherwise, submit `false`.
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
  local reviewResult = reviewer:prompt(`Review the openspec change name {CHANGE_NAME} using the openspec-review skill`)

  if judgeReviewResult(iteration, PASS_RULE, reviewResult.text) then
    ponos.log("The change is ready to implement")
    break
  end

  if judgeReviewResult(iteration, NEED_HUMAN_RULE, reviewResult.text) then
    ponos.log("The change needs human input to resolve the issue")
    break
  end

  local fixResult = reviewer:prompt("Please resolve the findings with critical or blocker severity.")
  ponos.log("Fix result: " .. fixResult.text)

  iteration += 1
end
