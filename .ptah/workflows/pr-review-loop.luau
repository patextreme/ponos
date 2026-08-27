--!strict

local cfg = require("./../utils/config")
local predicateAgent = require("./../utils/predicate")

-- compile-time constants first
const PR_URL = "https://github.com/patextreme/ptah/pull/4"
const MAX_ITERATIONS = 15

local pi: Agent = ptah.agent("pi")

local converged = false
local iteration = 0
while iteration < MAX_ITERATIONS do
	local reviewer: Session = pi:session({ id = `reviewer:{iteration}` })
	reviewer:setConfig("model", cfg.defaultModel)

	reviewer:prompt(`Use the review instruction at .ptah/instructions/review-instruction.md to review PR {PR_URL}`)
	local reviewVerdict = reviewer:prompt("Based on the review feedback, determine the blocking issues and non-blocking issues.")
	ptah.log("Review verdict: " .. reviewVerdict.text)

	if
		predicateAgent(
			"The review verdict does not contain blocking issues",
			reviewVerdict.text,
			{ sessionId = `judge-review:{iteration}`, model = cfg.judgeModel }
		)
	then
		reviewer:prompt("Please comment on the PR with review feedback along with verdict")
		converged = true
		break
	end

	reviewer:prompt("For each of the blocking issues, please use subagent to validate if issue is valid.")

	local needHumanResult = reviewer:prompt(
		"For the blocking issues remaining, do you know how to resolve them, or do you need human input?"
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
	ptah.log("Fix result: " .. fixResult.text)

	reviewer:prompt("Commit the changes and push them to the PR branch so the next review sees them.")
	reviewer:prompt("Also comment on the PR with review feedback")
	iteration += 1
end

if not converged then
	error(`Review loop did not converge after {MAX_ITERATIONS} iterations`)
end
