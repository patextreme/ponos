--!strict

const DEFAULT_MODEL = "qwencloud/qwen3.8-max"
const JUDGE_MODEL = "qwencloud/deepseek-v4-flash-0731"

const PR_URL = ""

local predicateAgent = require("./predicate")

local pi: Agent = ponos.agent("pi")

local iteration = 0
while true do
	local reviewer: Session = pi:session({ id = `reviewer:{iteration}` })
	reviewer:setConfig("model", DEFAULT_MODEL)

	reviewer:prompt(`Please ask Claude CLI to review the PR {PR_URL}`)
	local reviewVerdict = reviewer:prompt("Based on Claude's review feedback, determine the blocking issues and non-blocking issues.")
	if
		predicateAgent(
			"The review verdict does not contain blocking issues",
			reviewVerdict.text,
			{ sessionId = `judge-review:{iteration}`, model = JUDGE_MODEL }
		)
	then
	  reviewer:prompt("Please comment on the PR with Claude's review in the comment")
		break
	end

	local needHumanResult = reviewer:prompt(
		`For the blocking issues, do you know how to resolve them, or do you need human input?`
	)
	if
		predicateAgent(
			"The blocking issues require human input to resolve",
			needHumanResult.text,
			{ sessionId = `judge-human:{iteration}`, model = JUDGE_MODEL }
		)
	then
		error("The change needs human input to resolve the issue")
	end

	local fixResult = reviewer:prompt("Go ahead and resolve the blockers")
	ponos.log("Fix result: " .. fixResult.text)

	iteration += 1
end

