--!strict
-- Cross-tree require: this entry shares a helper with workflow-2 through
-- require("../shared/helper") — relative resolution has no directory
-- boundary, so sibling trees can share modules.
local helper = require("../shared/helper")

local agent = ponos.agent("demo")
local lines = helper.review_files(agent, { "src/main.rs", "src/config.rs" })
for _, line in ipairs(lines) do
	ponos.log(line)
end
