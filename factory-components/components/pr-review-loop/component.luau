--!strict
-- PR review loop component: run a review→fix→push convergence against a
-- pull request, generalized from identus-ws's pr-review-loop — all repo
-- specifics (reviewer instruction text, target repository, dry-run
-- gating) arrive as config. Composed from std.converge + std.predicate.

local converge = require("../../std/converge")

export type Config = {
	-- Work agent (name in the registry) reviewing and fixing the PR.
	agent: string,
	-- Judge agent for the typed predicates.
	judgeAgent: string,
	-- Optional per-session model config ids for work and judge sessions.
	model: string?,
	judgeModel: string?,
	-- Target repository the PR lives in (`owner/name`).
	repo: string,
	-- Repository-relative path of the reviewer instruction document the
	-- agent must follow.
	reviewInstructionFile: string,
	-- When true the commit/push step is skipped: the loop converges on
	-- reviews and fixes but never pushes to the PR branch.
	dryRun: boolean?,
	-- Convergence cap (default 15).
	maxIterations: number?,
}

export type Instance = {
	-- Run the review loop against one pull request (the PR URL is
	-- per-call data, not config).
	review: (self: Instance, prUrl: string) -> string,
}

local DEFAULT_MAX_ITERATIONS = 15

local M = {}

function M.new(config: Config): Instance
	local maxIterations =
		if config.maxIterations ~= nil then config.maxIterations else DEFAULT_MAX_ITERATIONS
	local dryRun = config.dryRun == true

	local instance: Instance = {
		review = function(_self: Instance, prUrl: string): string
			local afterFix: string? = if dryRun
				then nil
				else "Commit the changes and push them to the PR branch so the next review sees them. Also comment on the PR with the review feedback."
			if dryRun then
				ptah.log(`pr-review-loop: dry run — fixes are not pushed to {prUrl}`)
			end
			return converge.converge({
				sessionId = "pr-review",
				agent = config.agent,
				model = config.model,
				judgeAgent = config.judgeAgent,
				judgeModel = config.judgeModel,
				prompt = `Use the review instruction at {config.reviewInstructionFile} to review PR {prUrl} in {config.repo}. Based on the review feedback, determine the blocking issues and the non-blocking issues.`,
				judge = "The review verdict does not contain blocking issues",
				onConverged = "Please comment on the PR with the review feedback along with the verdict",
				escalationPrompt = "For each of the blocking issues, validate with a subagent whether the issue is real. For the blocking issues that remain, do you know how to resolve them, or do you need human input?",
				escalationJudge = "Human input is required to resolve the blocking issues",
				fix = "Go ahead and resolve the blocking issues",
				afterFix = afterFix,
				maxIterations = maxIterations,
			}).text
		end,
	}
	return instance
end

return M
