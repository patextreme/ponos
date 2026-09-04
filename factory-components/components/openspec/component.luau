--!strict
-- openspec component: groom, implement, and verify a named openspec
-- change, composed from std.converge (the review→judge→fix loop with
-- human escalation) and std.predicate. See README.md in this directory
-- for the declared environment requirements.

local converge = require("../../std/converge")

export type Config = {
	-- Work agent (name in the registry) driving the openspec skills.
	agent: string,
	-- Judge agent for the typed predicates (can be the same name with a
	-- different model).
	judgeAgent: string,
	-- Optional per-session model config ids for work and judge sessions.
	model: string?,
	judgeModel: string?,
	-- Convergence cap shared by every operation (default 10).
	maxIterations: number?,
}

export type Instance = {
	-- Converge the change's proposal through review.
	groom: (self: Instance, change: string) -> string,
	-- Drive task execution to completion.
	implement: (self: Instance, change: string) -> string,
	-- Converge verification, then sync and archive the change in the
	-- same operation.
	verify: (self: Instance, change: string) -> string,
}

local DEFAULT_MAX_ITERATIONS = 10

local M = {}

function M.new(config: Config): Instance
	local maxIterations =
		if config.maxIterations ~= nil then config.maxIterations else DEFAULT_MAX_ITERATIONS

	local function drive(sessionId: string, prompt: string, judge: string): string
		return converge.converge({
			sessionId = sessionId,
			agent = config.agent,
			model = config.model,
			judgeAgent = config.judgeAgent,
			judgeModel = config.judgeModel,
			prompt = prompt,
			judge = judge,
			escalationPrompt = "For the findings, do you know how to resolve them, or do you need human input?",
			escalationJudge = "Human input is required to resolve the findings",
			fix = "Go ahead and resolve the findings",
			maxIterations = maxIterations,
		}).text
	end

	local instance: Instance = {
		groom = function(_self: Instance, change: string): string
			ptah.log(`openspec: reviewing change {change}`)
			local text = drive(
				"openspec-groom",
				`Review the openspec change named {change} using the openspec-review skill`,
				"The review does not contain any blocker findings"
			)
			ptah.log(`openspec: change {change} is ready to implement`)
			return text
		end,
		implement = function(_self: Instance, change: string): string
			ptah.log(`openspec: implementing change {change}`)
			local text = drive(
				"openspec-implement",
				`Implement the pending tasks of the openspec change named {change} using the openspec-apply-change skill`,
				"All tasks of the change are implemented"
			)
			ptah.log(`openspec: change {change} is implemented`)
			return text
		end,
		verify = function(_self: Instance, change: string): string
			ptah.log(`openspec: verifying change {change}`)
			local text = drive(
				"openspec-verify",
				`Verify the implementation of the openspec change named {change} using the openspec-verify-change skill`,
				"The verification does not contain critical findings or warnings"
			)
			-- The sync-and-archive step is part of the same operation: it
			-- runs once the verification loop has converged.
			local archiver: Session = ptah.agent(config.agent):session({ id = "openspec-archiver" })
			if config.model ~= nil then
				archiver:setConfig("model", config.model)
			end
			archiver:prompt(`Please sync and archive the change {change}`)
			archiver:close()
			ptah.log(`openspec: change {change} synced and archived`)
			return text
		end,
	}
	return instance
end

return M
