"""Sieve of Eratosthenes — primes up to 1,000,000"""

limit = 1000000
sieve = [False] * (limit + 1)
sieve[0] = True
sieve[1] = True

i = 2
while i * i <= limit:
    if not sieve[i]:
        j = i * i
        while j <= limit:
            sieve[j] = True
            j += i
    i += 1

count = 0
for i in range(2, limit + 1):
    if not sieve[i]:
        count += 1

print(f"primes_sieve(1000000): count = {count}")
