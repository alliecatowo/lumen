# Version Bumper

Reads Cargo.toml files, finds `version = "X.Y.Z"` strings, and prints what would
change for major, minor, and patch version bumps. Does not modify files — only
reports what the new version would be.

```lumen
cell parse_version(version_str: String) -> list[Int]
  let parts = split(version_str, ".")
  if len(parts) != 3
    return [0, 0, 0]
  end
  let major = to_int(parts[0])
  let minor = to_int(parts[1])
  let patch = to_int(parts[2])
  [major, minor, patch]
end

cell format_version(parts: list[Int]) -> String
  to_string(parts[0]) + "." + to_string(parts[1]) + "." + to_string(parts[2])
end

cell bump_major(parts: list[Int]) -> list[Int]
  [parts[0] + 1, 0, 0]
end

cell bump_minor(parts: list[Int]) -> list[Int]
  [parts[0], parts[1] + 1, 0]
end

cell bump_patch(parts: list[Int]) -> list[Int]
  [parts[0], parts[1], parts[2] + 1]
end

cell find_version_in_toml(content: String) -> String
  # Use regex to find version = "X.Y.Z"
  let matches = regex_match("version\\s*=\\s*\"([0-9]+\\.[0-9]+\\.[0-9]+)\"", content)
  if len(matches) > 1
    return matches[1]
  end
  return ""
end

cell process_cargo_toml(path: String) -> Null
  print("--- {path} ---")

  let content = read_file(path)
  let version = find_version_in_toml(content)

  if version == ""
    print("  No version found.")
    print("")
    return null
  end

  let parts = parse_version(version)
  let current = format_version(parts)

  let major_bumped = format_version(bump_major(parts))
  let minor_bumped = format_version(bump_minor(parts))
  let patch_bumped = format_version(bump_patch(parts))

  print("  Current version: {current}")
  print("  Patch bump:      {current} -> {patch_bumped}")
  print("  Minor bump:      {current} -> {minor_bumped}")
  print("  Major bump:      {current} -> {major_bumped}")
  print("")
  null
end

cell main() -> Null
  print("=== Version Bumper ===")
  print("")

  # Find all Cargo.toml files
  let toml_files = glob("**/Cargo.toml")

  if len(toml_files) == 0
    print("No Cargo.toml files found.")
    return null
  end

  print("Found {len(toml_files)} Cargo.toml file(s):")
  print("")

  let i = 0
  while i < len(toml_files)
    process_cargo_toml(toml_files[i])
    i = i + 1
  end

  print("=== Done ===")
  null
end
```
