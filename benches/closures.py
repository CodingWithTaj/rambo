def counter():
    n = 0
    def inc():
        nonlocal n
        n = n + 1
        return n
    return inc
def run():
    c = counter()
    last = 0
    for i in range(2000000):
        last = c()
    return last
print(run())
