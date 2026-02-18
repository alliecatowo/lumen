"""Fannkuch-Redux benchmark, N=10"""

N = 10


def fannkuch(n):
    perm1 = list(range(n))
    count = [0] * n
    max_flips = 0
    checksum = 0
    r = n
    perm_count = 0

    while True:
        while r > 1:
            count[r - 1] = r
            r -= 1

        perm = perm1[:]

        # Count flips
        flips = 0
        k = perm[0]
        while k != 0:
            # Reverse first k+1 elements
            perm[: k + 1] = perm[: k + 1][::-1]
            flips += 1
            k = perm[0]

        if flips > max_flips:
            max_flips = flips
        if perm_count % 2 == 0:
            checksum += flips
        else:
            checksum -= flips
        perm_count += 1

        # Next permutation
        done = False
        while True:
            if r == n:
                done = True
                break
            p0 = perm1[0]
            for i in range(r):
                perm1[i] = perm1[i + 1]
            perm1[r] = p0
            count[r] -= 1
            if count[r] > 0:
                break
            r += 1

        if done:
            break

    print(checksum)
    print(f"Pfannkuchen({n}) = {max_flips}")


if __name__ == "__main__":
    fannkuch(N)
