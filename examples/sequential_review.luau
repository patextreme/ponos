-- Sequential review: one session, one turn at a time.
local agent = ponos.agent("demo")
local s = agent:session({ id = "reviewer" })

local files = { "src/main.rs", "src/config.rs", "src/acp/mod.rs" }
for _, file in ipairs(files) do
    local r = s:prompt(("Review %s for obvious bugs; be terse."):format(file))
    ponos.log(("%s -> %s"):format(file, tostring(r)))
end

s:close()
