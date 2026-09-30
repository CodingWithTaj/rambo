local function run()
    local total = 0
    local i = 0
    while i < 10000000 do
        total = total + i % 7
        i = i + 1
    end
    return total
end
print(run())
