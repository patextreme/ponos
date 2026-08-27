--!strict
-- Watchdog cancel: a slow turn is cancelled after a grace period; the
-- prompt returns normally with stopReason == "cancelled".
local agent = ptah.agent("demo")
local s = agent:session({ id = "slow" })

local work = ptah.spawn(function()
	local r = s:prompt("Take your time on this one")
	ptah.log(("turn finished: stopReason=%s"):format(r.stopReason))
	return r.stopReason
end)

ptah.sleep(300)
ptah.log("watchdog: cancelling")
s:cancel()

local stopReason = work:await()
assert(stopReason == "cancelled", "expected cancelled, got " .. tostring(stopReason))
s:close()
