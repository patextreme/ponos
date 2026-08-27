--!strict
-- The sibling consumer of examples/shared/helper: same module, different
-- workflow — a cross-tree require keeps shared code out of each entry's
-- directory.
local helper = require("../shared/helper")

local agent = ptah.agent("demo")
local lines = helper.review_files(agent, { "docs/architecture.md", "docs/runbook.md" })
ptah.log(("workflow-2 reviewed %d files"):format(#lines))
