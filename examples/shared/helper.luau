--!strict
-- Shared review helper required by both workflow entries as
-- "../shared/helper". Lives outside each workflow's directory: relative
-- resolution walks out of the entry tree to reach it.

local helper = {}

-- Run one reviewer session over a file list; returns one log line per
-- file ("path -> verdict").
function helper.review_files(agent: Agent, files: { string }): { string }
	local s = agent:session({ id = "reviewer" })
	local lines = {}
	for _, file in ipairs(files) do
		local r = s:prompt(("Review %s for obvious bugs; be terse."):format(file))
		table.insert(lines, ("%s -> %s"):format(file, tostring(r)))
	end
	s:close()
	return lines
end

return helper
