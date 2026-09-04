--!strict
-- Shim: openspec lifecycle (groom/implement/verify) over the shared
-- library (see factory-components/README.md for the consumption
-- model). The only workflow code this repo owns.
--
-- ptah scripts take no runtime arguments, so the op and change name
-- are hand-edited per use; one call active at a time.

local openspec = require("../../factory-components/components/openspec/component")

local opsx = openspec.new({
	agent = "pi",
	judgeAgent = "pi",
	model = "zai/glm-5.3",
	judgeModel = "zai/glm-5.3-flash",
})

local changeName = "component-agent-handles"

opsx:groom(changeName)
opsx:implement(changeName)
opsx:verify(changeName)

local agent = ptah.agent("pi")
local session = agent:session({ id = "committer" })
session:prompt("Commit the uncommitted changes and push to remote")

