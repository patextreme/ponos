--!strict
-- Shim: openspec lifecycle (groom/implement/verify) over the shared
-- library (see factory-components/README.md for the consumption
-- model). The only workflow code this repo owns.
--
-- ptah scripts take no runtime arguments, so the op and change name
-- are hand-edited per use; one call active at a time.

local openspec = require("../../factory-components/components/openspec/component")
local prReview = require("../../factory-components/components/pr-review-loop/component")

local agent = ptah.agent("pi")

local opsx = openspec.new({
	agent = agent,
	judgeAgent = ptah.agent("pi"),
	model = "zai/glm-5.3",
	judgeModel = "zai/glm-5.3-flash",
})
local review = prReview.new({
	agent = agent,
	judgeAgent = agent,
	model = "zai/glm-5.3",
	judgeModel = "zai/glm-5.3-flash",
	reviewInstructionFile = ".ptah/instructions/review-instruction.md",
	dryRun = false,
	maxIterations = 15,
})

local changeName = "component-agent-handles"

opsx:groom(changeName)
opsx:implement(changeName)
opsx:verify(changeName)

local session = agent:session({
	id = "committer",
	resultSchema = { prUrl = { type = "string" } }
})
session:prompt("Commit the uncommitted changes and push to remote")
local result = session:prompt("What is the PR url? Pass the url back to the ptah via result submit tool")
review:review(result.result.prUrl :: string )
