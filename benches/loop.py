def run():
    total = 0
    i = 0
    while i < 10000000:
        total = total + i % 7
        i = i + 1
    return total
print(run())
