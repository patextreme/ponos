--!strict
-- Convergence loop (stdlib): drive an agent session toward a typed
-- predicate — prompt, judge the result, and on failure prompt a fix and
-- repeat, until the predicate holds, the loop escalates to a human, or
-- the iteration cap is reached.
--
-- Escalation semantics (arbitrated from the drifted per-repo copies):
-- `escalationJudge` is a predicate that HOLDS when human input is
-- required; the loop then fails with an error and does not issue the fix
-- prompt. When it does not hold the findings are fixable and the loop
-- iterates. Every prompt of an iteration is prefixed with a
-- `[<sessionId> iteration N of M]` header so the agent (and the logs)
-- can see the loop state.

local predicate = require("./predicate")

export type ConvergeOptions = {
	-- Work agent (name in the registry) and optional model config id.
	agent: string,
	model: string?,
	-- Session id prefix; per-iteration sessions are `<prefix>:<n>` and
	-- judge sessions `<prefix>-judge:<n>` / `<prefix>-human:<n>`.
	sessionId: string,
	-- Judge agent and optional model; the judge is queried with
	-- std.predicate's typed boolean contract.
	judgeAgent: string,
	judgeModel: string?,
	-- The operation's prompt and the predicate that must hold for its
	-- output.
	prompt: string,
	judge: string,
	-- Asked when the judge rejects the output; judged by
	-- `escalationJudge`.
	escalationPrompt: string,
	escalationJudge: string,
	-- Prompt issued when the findings are fixable.
	fix: string,
	-- Optional prompt after the fix (e.g. commit and push), and optional
	-- prompt once the judge accepts (e.g. post the verdict).
	afterFix: string?,
	onConverged: string?,
	maxIterations: number,
}

export type ConvergeResult = {
	-- Number of review passes issued (1 when the first pass converged).
	iterations: number,
	-- Final accepted review output.
	text: string,
}

local function converge(options: ConvergeOptions): ConvergeResult
	local work: Agent = ptah.agent(options.agent)
	local max = options.maxIterations
	for iteration = 0, max - 1 do
		local header = `[{options.sessionId} iteration {iteration + 1} of {max}]\n`
		local session: Session = work:session({ id = `{options.sessionId}:{iteration}` })
		if options.model ~= nil then
			session:setConfig("model", options.model)
		end

		local outcome = session:prompt(header .. options.prompt)
		if predicate(options.judge, outcome.text, {
			agent = options.judgeAgent,
			sessionId = `{options.sessionId}-judge:{iteration}`,
			model = options.judgeModel,
		}) then
			if options.onConverged ~= nil then
				session:prompt(header .. options.onConverged)
			end
			session:close()
			return { iterations = iteration + 1, text = outcome.text }
		end

		local escalation = session:prompt(header .. options.escalationPrompt)
		local needsHuman = predicate(options.escalationJudge, escalation.text, {
			agent = options.judgeAgent,
			sessionId = `{options.sessionId}-human:{iteration}`,
			model = options.judgeModel,
		})
		if needsHuman then
			session:close()
			error(
				`{options.sessionId}: human input is required to resolve the findings (iteration {iteration + 1} of {max})`
			)
		end

		local fix = session:prompt(header .. options.fix)
		ptah.log(`{options.sessionId} iteration {iteration + 1}: findings judged fixable — fix result: {fix.text}`)
		if options.afterFix ~= nil then
			session:prompt(header .. options.afterFix)
		end
		session:close()
	end
	return error(`{options.sessionId}: did not converge within {max} iterations`)
end

return {
	converge = converge,
}
