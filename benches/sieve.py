def sieve(n):
    flags = []
    for i in range(n): flags.append(True)
    count = 0
    for i in range(2, n):
        if flags[i]:
            count = count + 1
            m = i * i
            while m < n:
                flags[m] = False
                m = m + i
    return count
print(sieve(2000000))
