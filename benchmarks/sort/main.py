#!/usr/bin/env python3
"""Mergesort benchmark: sort 100,000 integers [100000..1], print sum"""


def main():
    n = 100000
    data = list(range(n, 0, -1))  # [100000, 99999, ..., 1]
    data.sort()
    s = sum(data)
    print(f"sort({n}) sum={s}")


if __name__ == "__main__":
    main()
