#!/usr/bin/env python3
"""String processing benchmark: concat + search"""


def main():
    unit = "hello world "
    parts = [unit] * 10000
    s = "".join(parts)
    c = s.count("world")
    print(f"string_len={len(s)} occurrences={c}")


if __name__ == "__main__":
    main()
