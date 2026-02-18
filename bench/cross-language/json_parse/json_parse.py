import json


def main():
    # Build a dict with 10000 entries
    data = {}
    for i in range(10000):
        data[f"key_{i}"] = f"value_{i}"

    # Serialize to JSON
    json_str = json.dumps(data)

    # Parse back
    parsed = json.loads(json_str)

    # Access a field
    found = parsed["key_9999"]
    print(f"Found: {found}")
    print(f"Count: {len(parsed)}")


if __name__ == "__main__":
    main()
