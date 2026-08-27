--!strict
--
-- Exec + JSON + one agent turn: deterministic pipeline steps around a
-- probabilistic agent step. Runs fully offline (git is used when the
-- working directory has history; the printf fallback covers bare
-- directories and machines without git — a nonzero exit code is data,
-- not an error).
--
-- Demonstrates:
--   * `ptah.exec` pipelines and the result table (exit code as data)
--   * `ptah.json.parse` turning captured command output into script data
--   * an agent turn progressing while a command is in flight (exec
--     blocks its coroutine, not the run)

local agent = ptah.agent("demo")

-- Deterministic step 1: harvest subjects. `git log` when available;
-- otherwise a printf-built stand-in — either way the script ends up
-- with JSON text to decode.
local json: string
local git = ptah.exec("git log --oneline -3 2>/dev/null")
if git.exitCode == 0 and #git.stdout > 0 then
	local subjects = {}
	for line in git.stdout:gmatch("[^\n]+") do
		-- Quote for JSON: escape backslashes, then double quotes.
		local escaped = line:gsub("\\", "\\\\"):gsub('"', '\\"')
		table.insert(subjects, string.format('{"subject": "%s"}', escaped))
	end
	json = "[" .. table.concat(subjects, ",") .. "]"
else
	json = ptah.exec([[printf '[{"subject": "no git history"}]']]).stdout
end

local entries = ptah.json.parse(json)
assert(#entries >= 1, "pipeline produced no entries")
for i, entry in ipairs(entries) do
	ptah.log(("%d. %s"):format(i, tostring(entry.subject)))
end

-- Probabilistic step: one agent turn runs while a slow command is in
-- flight — spawned tasks keep progressing during an exec.
local session = agent:session({ id = "summarizer" })
local turn = ptah.spawn(function()
	local r = session:prompt("summarize the pipeline output")
	return r.text
end)
local slow = ptah.exec("sleep 0.1; printf ready")
assert(slow.exitCode == 0, "slow exec failed: " .. slow.stderr)
assert(slow.stdout == "ready", "slow exec stdout: " .. slow.stdout)
local reply = turn:await()
ptah.log("agent replied: " .. tostring(reply))
session:close()
