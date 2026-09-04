--!strict
-- Daemon loop skeleton (stdlib): apply a per-repo operation to every
-- configured repository, each repo isolated behind an error boundary so
-- one raising repo cannot abort the others. Sequential by default;
-- bounded-concurrency parallel via ptah.parallel when `concurrency` is
-- set (>= 1).

export type RepoOutcome = {
	repo: string,
	ok: boolean,
	-- Normalized error message for a failed repo (nil on success).
	error: string?,
}

export type DaemonOptions = {
	-- >= 1: run the per-repo operation with this concurrency bound.
	-- Absent: sequential.
	concurrency: number?,
}

local function errorMessage(value: any): string
	if type(value) == "string" then
		return value
	end
	return tostring(value)
end

local function each(repos: { string }, op: (repo: string) -> (), options: DaemonOptions?): { RepoOutcome }
	local opts: DaemonOptions = if options ~= nil then options else {}
	-- A nil-returning adapter so both pcall (boolean, R...) and
	-- ptah.parallel's (T) -> U callback shape see a definite return
	-- value under strict mode.
	local function guarded(repo: string): nil
		op(repo)
		return nil
	end
	local outcomes: { RepoOutcome } = {}
	if opts.concurrency ~= nil and opts.concurrency >= 1 then
		local entries = ptah.parallel(repos, guarded, { concurrency = opts.concurrency })
		for i, entry in entries do
			local repo = repos[i]
			if entry.ok then
				table.insert(outcomes, { repo = repo, ok = true, error = nil })
			else
				table.insert(outcomes, { repo = repo, ok = false, error = errorMessage(entry.error) })
			end
		end
	else
		for _, repo in ipairs(repos) do
			local ok, err = pcall(guarded, repo)
			if ok then
				table.insert(outcomes, { repo = repo, ok = true, error = nil })
			else
				table.insert(outcomes, { repo = repo, ok = false, error = errorMessage(err) })
			end
		end
	end
	return outcomes
end

return {
	each = each,
}
