--!strict

const DEFAULT_MODEL = "zai/glm-5.3"
const JUDGE_MODEL = "zai-coding-plan/glm-5.3"
const CHANGE_NAME = "add-check-subcommand"

local predicateAgent = require("./predicate")

local pi: Agent = ponos.agent("pi")

local iteration = 0
while true do
	local reviewer: Session = pi:session({ id = `reviewer:{iteration}`, config = { model = DEFAULT_MODEL } })

	local reviewResult = reviewer:prompt(
		`Verify the implementation of openspec change named {CHANGE_NAME} using the openspec-verify-change skill`
	)
	if
		predicateAgent(
			"The verification does not contain critical findings or warnings.",
			reviewResult.text,
			{ sessionId = `judge-review:{iteration}`, model = JUDGE_MODEL }
		)
	then
		ponos.log("The change is ready to be synced and archived")
		break
	end

	local needHumanResult = reviewer:prompt(
		`For the critical and warning severity findings, do you know how to resolve them, or do you need human input?`
	)
	if
		not predicateAgent(
			"The verification result required human input to resolve",
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

local archiver: Session = pi:session({ id = "archiver", config = { model = DEFAULT_MODEL } })
archiver:prompt(`Please sync and archive the change {CHANGE_NAME}`)
