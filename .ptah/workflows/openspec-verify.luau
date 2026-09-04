--!strict
-- Shim: openspec verify loop (converge verification, then sync and
-- archive) over the shared library (see factory-components/README.md
-- for the consumption model). The only workflow code this repo owns.

local openspec = require("../../factory-components/components/openspec/component")

local ops = openspec.new({
	agent = "pi",
	judgeAgent = "pi",
	model = "zai/glm-5.3",
	judgeModel = "zai/glm-5.3-flash",
})

ops:verify("cli-completions-and-init")
