-- Watchdog cancel: a slow turn is cancelled after a grace period; the
-- prompt returns normally with stop_reason == "cancelled".
local agent = ponos.agent("demo")
local s = agent:session({ id = "slow" })

local work = ponos.spawn(function()
    local r = s:prompt("Take your time on this one")
    ponos.log(("turn finished: stop_reason=%s"):format(r.stop_reason))
    return r.stop_reason
end)

ponos.sleep(300)
ponos.log("watchdog: cancelling")
s:cancel()

local stop_reason = work:await()
assert(stop_reason == "cancelled", "expected cancelled, got " .. tostring(stop_reason))
s:close()
