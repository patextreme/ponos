--!strict

local cfg = require("./../utils/config")
local predicateAgent = require("./../utils/predicate")

local pi: Agent = ponos.agent("pi")

local iteration = 0
while true do
	local reviewer: Session = pi:session({ id = `reviewer:{iteration}` })
	reviewer:setConfig("model", cfg.defaultModel)

	local reviewResult =
		reviewer:prompt(`Review the openspec change named {cfg.changeName} using the openspec-review skill`)
	if
		predicateAgent(
			"The review does not contain any blocker findings.",
			reviewResult.text,
			{ sessionId = `judge-review:{iteration}`, model = cfg.judgeModel }
		)
	then
		ponos.log("The change is ready to implement")
		break
	end

	local needHumanResult =
		reviewer:prompt("For the blocker findings, do you know how to resolve them or do you need human input")
	if
		not predicateAgent(
			"The blocker findings are resolvable without human input",
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
