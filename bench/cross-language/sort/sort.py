import sys


def main():
    n = 1_000_000
    # Deterministic pseudo-random fill (LCG)
    val = 42
    data = []
    for _ in range(n):
        val = (val * 1103515245 + 12345) % 2147483648
        data.append(val % 100000)

    data.sort()

    # Verify sorted
    ok = all(data[i] <= data[i + 1] for i in range(len(data) - 1))
    print(f"sort({n}) sorted={ok}")


if __name__ == "__main__":
    main()
