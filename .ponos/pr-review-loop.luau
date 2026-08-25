--!strict

local cfg = require("./config")

-- This loop benchmarks a different default model than the openspec scripts;
-- keep the override local so the shared DEFAULT_MODEL isn't disturbed.
const DEFAULT_MODEL = "qwencloud/qwen3.8-max"

const PR_URL = ""
const MAX_ITERATIONS = 15

local predicateAgent = require("./predicate")

local pi: Agent = ponos.agent("pi")

local converged = false
local iteration = 0
while iteration < MAX_ITERATIONS do
	local reviewer: Session = pi:session({ id = `reviewer:{iteration}` })
	reviewer:setConfig("model", DEFAULT_MODEL)

	reviewer:prompt(`Please ask Claude CLI to review the PR {PR_URL}`)
	local reviewVerdict = reviewer:prompt("Based on Claude's review feedback, determine the blocking issues and non-blocking issues.")
	ponos.log("Review verdict: " .. reviewVerdict.text)

	if
		predicateAgent(
			"The review verdict does not contain blocking issues",
			reviewVerdict.text,
			{ sessionId = `judge-review:{iteration}`, model = cfg.judgeModel }
		)
	then
		reviewer:prompt("Please comment on the PR with Claude's review")
		converged = true
		break
	end

	local needHumanResult = reviewer:prompt(
		"For the blocking issues, do you know how to resolve them, or do you need human input?"
	)
	if
		predicateAgent(
			"The blocking issues require human input to resolve",
			needHumanResult.text,
			{ sessionId = `judge-human:{iteration}`, model = cfg.judgeModel }
		)
	then
		error("The change needs human input to resolve the issue")
	end

	local fixResult = reviewer:prompt("Go ahead and resolve the blockers")
	ponos.log("Fix result: " .. fixResult.text)

	reviewer:prompt("Commit the changes and push them to the PR branch so the next review sees them.")
	reviewer:prompt("Also comment on the PR with Claude's review")
	iteration += 1
end

if not converged then
	error(`Review loop did not converge after {MAX_ITERATIONS} iterations`)
end
