--!strict
-- GitHub CLI transport (stdlib): shell out to `gh` through ptah.exec and
-- return a structured outcome instead of raising — a failed command is
-- data for the calling script. This is the arbitrated shape of the three
-- drifted per-repo copies: a typed JSON outcome record with a two-sided
-- trim of captured output, and POSIX-safe single-quoting so argument
-- values containing spaces or quotes pass through verbatim.

export type GhOptions = {
	-- Request parsing of stdout as JSON (sets `json` on the outcome).
	json: boolean?,
	-- No budget unless set (mirrors ptah.exec).
	timeoutMs: number?,
}

export type GhOutcome = {
	-- exitCode == 0.
	ok: boolean,
	exitCode: number,
	-- Captured output, trimmed at both ends.
	stdout: string,
	stderr: string,
	-- Parsed JSON value when `json` was requested and the command
	-- succeeded; nil otherwise. Malformed JSON from a zero exit is a
	-- contract violation and raises (a loud error, never a silent nil).
	json: any,
}

-- Two-sided trim: `gh` decorates its output with surrounding whitespace
-- (and one-sided trims lose the leading half of it).
local function trim(s: string): string
	local trimmed = (s:gsub("^%s+", ""))
	return (trimmed:gsub("%s+$", ""))
end

-- POSIX sh single-quoting: wrap in single quotes and escape embedded
-- single quotes as '\'' (close, escaped quote, reopen).
local function quote(arg: string): string
	return "'" .. (arg:gsub("'", "'\\''")) .. "'"
end

local function run(args: { string }, options: GhOptions?): GhOutcome
	local opts: GhOptions = if options ~= nil then options else {}
	local command = "gh"
	for _, arg in ipairs(args) do
		command ..= " " .. quote(arg)
	end
	local result = ptah.exec(command, { timeoutMs = opts.timeoutMs })
	local parsed: any = nil
	if opts.json and result.exitCode == 0 then
		local ok, value = pcall(ptah.json.parse, trim(result.stdout))
		if not ok then
			error(`gh: expected JSON output from \`{command}\` but could not parse it`)
		end
		parsed = value
	end
	return {
		ok = result.exitCode == 0,
		exitCode = result.exitCode,
		stdout = trim(result.stdout),
		stderr = trim(result.stderr),
		json = parsed,
	}
end

return {
	run = run,
}
