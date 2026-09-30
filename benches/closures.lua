local function counter()
    local n = 0
    return function() n = n + 1; return n end
end
local function run()
    local c = counter()
    local last = 0
    for i = 0, 1999999 do last = c() end
    return last
end
print(run())
