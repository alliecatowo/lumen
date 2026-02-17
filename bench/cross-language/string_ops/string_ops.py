def main():
    parts = []
    for i in range(100000):
        parts.append("x")
    s = "".join(parts)
    print(f"Length: {len(s)}")


if __name__ == "__main__":
    main()
