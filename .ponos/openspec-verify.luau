--!strict

local cfg = require("./config")

local predicateAgent = require("./predicate")

local pi: Agent = ponos.agent("pi")

local iteration = 0
while true do
	local reviewer: Session = pi:session({ id = `reviewer:{iteration}` })
	reviewer:setConfig("model", cfg.defaultModel)

	local reviewResult = reviewer:prompt(
		`Verify the implementation of openspec change named {cfg.changeName} using the openspec-verify-change skill`
	)
	if
		predicateAgent(
			"The verification does not contain critical findings or warnings.",
			reviewResult.text,
			{ sessionId = `judge-review:{iteration}`, model = cfg.judgeModel }
		)
	then
		ponos.log("The change is ready to be synced and archived")
		break
	end

	local needHumanResult = reviewer:prompt(
		`For the critical and warning severity findings, do you know how to resolve them, or do you need human input?`
	)
	if
		predicateAgent(
			"The verification result required human input to resolve",
			needHumanResult.text,
			{ sessionId = `judge-human:{iteration}`, model = cfg.judgeModel }
		)
	then
		error("The change needs human input to resolve the issue")
	end

	local fixResult = reviewer:prompt("Go ahead and resolve the blockers")
	ponos.log("Fix result: " .. fixResult.text)

	iteration += 1
end

local archiver: Session = pi:session({ id = "archiver" })
archiver:setConfig("model", cfg.defaultModel)
archiver:prompt(`Please sync and archive the change {cfg.changeName}`)
