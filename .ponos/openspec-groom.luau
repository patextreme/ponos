const DEFAULT_MODEL = "zai/glm-5.3"
const SMALL_MODEL = "qwencloud/deepseek-v4-flash-0731"
const CHANGE_NAME = "revise-script-api"

local predicateAgent = require("./predicate")

local pi: Agent = ponos.agent("pi")

local iteration = 0
while true do
  local reviewer: Session = pi:session({ id = `reviewer:{iteration}` })
  reviewer:setConfig("model", DEFAULT_MODEL)

  local reviewResult = reviewer:prompt(`Review the openspec change named {CHANGE_NAME} using the openspec-review skill`)
  local reviewPredicatePrompot = "The review does not contain any blocker findings."
  if predicateAgent(reviewPredicatePrompot, reviewResult.text, { sessionId = `judge-review:{iteration}`, model = SMALL_MODEL }) then
    ponos.log("The change is ready to implement")
    break
  end

  local needHumanResult = reviewer:prompt("For the blocker findings, do you know how to resolve them or do you need human input")
  local needHumanPredicatePrompt = "The blocker findings are resolvable without human input"
  if not predicateAgent(needHumanPredicatePrompt, needHumanResult.text, { sessionId = `judge-human:{iteration}`, model = SMALL_MODEL }) then
    ponos.log("The change needs human input to resolve the issue")
    ponos.exit(1)
  end

  local fixResult = reviewer:prompt("Go ahead and resolve the blockers")
  ponos.log("Fix result: " .. fixResult.text)

  iteration += 1
end
