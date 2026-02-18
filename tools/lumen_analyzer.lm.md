# Lumen Source Code Analyzer

A comprehensive source code analyzer for the Lumen project, written in Lumen.
Walks project directories, reads source files, counts lines and constructs,
identifies definitions, and outputs a detailed report with tables.

Task T452.

## String and Number Utilities

```lumen
cell max_int(a: Int, b: Int) -> Int
  if a > b
    return a
  end
  b
end

cell min_int(a: Int, b: Int) -> Int
  if a < b
    return a
  end
  b
end

cell abs_int(n: Int) -> Int
  if n < 0
    return 0 - n
  end
  n
end

cell pad_right(s: String, width: Int) -> String
  let result = s
  while len(result) < width
    result = result + " "
  end
  result
end

cell pad_left(s: String, width: Int) -> String
  let result = s
  while len(result) < width
    result = " " + result
  end
  result
end

cell repeat_char(ch: String, count: Int) -> String
  let result = ""
  let i = 0
  while i < count
    result = result + ch
    i = i + 1
  end
  result
end

cell format_int(n: Int) -> String
  to_string(n)
end

cell format_percent(part: Int, total: Int) -> String
  if total == 0
    return "0.0%"
  end
  let pct_times_10 = (part * 1000) / total
  let whole = pct_times_10 / 10
  let frac = pct_times_10 % 10
  to_string(whole) + "." + to_string(frac) + "%"
end

cell format_ratio(a: Int, b: Int) -> String
  if b == 0
    return "N/A"
  end
  let ratio_times_100 = (a * 100) / b
  let whole = ratio_times_100 / 100
  let frac = ratio_times_100 % 100
  let frac_str = to_string(frac)
  if frac < 10
    frac_str = "0" + frac_str
  end
  to_string(whole) + "." + frac_str
end

cell format_thousands(n: Int) -> String
  to_string(n)
end

cell truncate_string(s: String, max_len: Int) -> String
  # String indexing (s[i]) returns null in Lumen, so we cannot
  # character-slice. Use split on "/" to shorten paths instead.
  if len(s) <= max_len
    return s
  end
  let parts = split(s, "/")
  if len(parts) <= 2
    return s
  end
  # Keep last 2 path components with "..." prefix
  let last = parts[len(parts) - 1]
  let second = parts[len(parts) - 2]
  let short = ".../" + second + "/" + last
  if len(short) <= max_len
    return short
  end
  # Still too long, just keep filename
  ".../" + last
end
```

## List Utilities

```lumen
cell str_list_contains(items: list[String], target: String) -> Bool
  for item in items
    if item == target
      return true
    end
  end
  false
end

cell str_list_unique(items: list[String]) -> list[String]
  let result = []
  for item in items
    if not str_list_contains(result, item)
      result = append(result, item)
    end
  end
  result
end

cell int_list_sum(items: list[Int]) -> Int
  let total = 0
  for item in items
    total = total + item
  end
  total
end

cell int_list_max(items: list[Int]) -> Int
  if len(items) == 0
    return 0
  end
  let result = items[0]
  let i = 1
  while i < len(items)
    if items[i] > result
      result = items[i]
    end
    i = i + 1
  end
  result
end

cell int_list_min(items: list[Int]) -> Int
  if len(items) == 0
    return 0
  end
  let result = items[0]
  let i = 1
  while i < len(items)
    if items[i] < result
      result = items[i]
    end
    i = i + 1
  end
  result
end

cell int_list_mean(items: list[Int]) -> Int
  if len(items) == 0
    return 0
  end
  let total = int_list_sum(items)
  total / len(items)
end

cell int_list_median(items: list[Int]) -> Int
  let n = len(items)
  if n == 0
    return 0
  end
  let sorted = sort(items)
  if n % 2 == 0
    return (sorted[n / 2 - 1] + sorted[n / 2]) / 2
  end
  sorted[n / 2]
end

cell count_matching(items: list[String], target: String) -> Int
  let count = 0
  for item in items
    if item == target
      count = count + 1
    end
  end
  count
end
```

## File Classification

```lumen
cell is_lumen_markdown(path: String) -> Bool
  ends_with(path, ".lm.md")
end

cell is_lumen_raw(path: String) -> Bool
  if ends_with(path, ".lm.md")
    return false
  end
  ends_with(path, ".lm")
end

cell is_lumen_native(path: String) -> Bool
  ends_with(path, ".lumen")
end

cell is_rust_file(path: String) -> Bool
  ends_with(path, ".rs")
end

cell is_toml_file(path: String) -> Bool
  ends_with(path, ".toml")
end

cell is_markdown_file(path: String) -> Bool
  if ends_with(path, ".lm.md")
    return false
  end
  ends_with(path, ".md")
end

cell is_json_file(path: String) -> Bool
  ends_with(path, ".json")
end

cell is_yaml_file(path: String) -> Bool
  ends_with(path, ".yml") or ends_with(path, ".yaml")
end

cell is_javascript_file(path: String) -> Bool
  ends_with(path, ".js")
end

cell is_typescript_file(path: String) -> Bool
  ends_with(path, ".ts")
end

cell classify_file(path: String) -> String
  if is_lumen_markdown(path)
    return "lumen-md"
  end
  if is_lumen_raw(path)
    return "lumen-raw"
  end
  if is_lumen_native(path)
    return "lumen-native"
  end
  if is_rust_file(path)
    return "rust"
  end
  if is_toml_file(path)
    return "toml"
  end
  if is_markdown_file(path)
    return "markdown"
  end
  if is_json_file(path)
    return "json"
  end
  if is_yaml_file(path)
    return "yaml"
  end
  if is_javascript_file(path)
    return "javascript"
  end
  if is_typescript_file(path)
    return "typescript"
  end
  "other"
end

cell file_category(file_type: String) -> String
  if file_type == "lumen-md" or file_type == "lumen-raw" or file_type == "lumen-native"
    return "Lumen Source"
  end
  if file_type == "rust"
    return "Rust Source"
  end
  if file_type == "toml"
    return "Configuration"
  end
  if file_type == "markdown"
    return "Documentation"
  end
  if file_type == "json"
    return "Data"
  end
  if file_type == "yaml"
    return "Configuration"
  end
  if file_type == "javascript" or file_type == "typescript"
    return "JavaScript/TS"
  end
  "Other"
end
```

## Line Classification

```lumen
cell is_blank_line(line: String) -> Bool
  len(trim(line)) == 0
end

cell is_rust_comment(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "//") or starts_with(trimmed, "/*") or starts_with(trimmed, "* ") or starts_with(trimmed, "*/") or trimmed == "*"
end

cell is_lumen_comment(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "#")
end

cell is_toml_comment(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "#")
end

cell is_comment_line(line: String, file_type: String) -> Bool
  if file_type == "rust"
    return is_rust_comment(line)
  end
  if file_type == "lumen-md" or file_type == "lumen-raw" or file_type == "lumen-native"
    return is_lumen_comment(line)
  end
  if file_type == "toml"
    return is_toml_comment(line)
  end
  false
end
```

## Definition Detection for Lumen Files

```lumen
cell is_cell_definition(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "cell ")
end

cell is_record_definition(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "record ")
end

cell is_enum_definition(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "enum ")
end

cell is_import_statement(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "import ")
end

cell is_process_definition(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "process ")
end

cell is_effect_declaration(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "effect ")
end

cell is_grant_statement(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "grant ")
end

cell is_type_alias(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "type ")
end

cell is_extern_declaration(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "extern ")
end

cell is_test_annotation(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "@test")
end
```

## Definition Detection for Rust Files

```lumen
cell is_rust_fn(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "pub fn ") or starts_with(trimmed, "fn ") or starts_with(trimmed, "pub(crate) fn ") or starts_with(trimmed, "pub async fn ") or starts_with(trimmed, "async fn ")
end

cell is_rust_struct(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "pub struct ") or starts_with(trimmed, "struct ") or starts_with(trimmed, "pub(crate) struct ")
end

cell is_rust_enum(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "pub enum ") or starts_with(trimmed, "enum ") or starts_with(trimmed, "pub(crate) enum ")
end

cell is_rust_impl(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "impl ") or starts_with(trimmed, "impl<")
end

cell is_rust_trait(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "pub trait ") or starts_with(trimmed, "trait ") or starts_with(trimmed, "pub(crate) trait ")
end

cell is_rust_use(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "use ") or starts_with(trimmed, "pub use ")
end

cell is_rust_mod(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "mod ") or starts_with(trimmed, "pub mod ") or starts_with(trimmed, "pub(crate) mod ")
end

cell is_rust_test_attr(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "#[test]") or starts_with(trimmed, "#[cfg(test)]")
end

cell is_rust_macro(line: String) -> Bool
  let trimmed = trim(line)
  starts_with(trimmed, "macro_rules!")
end
```

## File Analysis Record

```lumen
record FileStats
  path: String
  file_type: String
  total_lines: Int
  blank_lines: Int
  comment_lines: Int
  code_lines: Int
  cell_defs: Int
  record_defs: Int
  enum_defs: Int
  import_stmts: Int
  fn_defs: Int
  struct_defs: Int
  rust_enum_defs: Int
  trait_defs: Int
  impl_blocks: Int
  test_annotations: Int
  process_defs: Int
  effect_decls: Int
  grant_stmts: Int
  type_aliases: Int
  extern_decls: Int
  use_stmts: Int
  mod_decls: Int
  macro_defs: Int
  max_line_length: Int
  has_lumen_code: Bool
end
```

## Single File Analyzer

```lumen
cell count_lumen_defs(lines: list[String]) -> list[Int]
  let cells = 0
  let records = 0
  let enums = 0
  let imports = 0
  let processes = 0
  let effects = 0
  let grants = 0
  let type_als = 0
  let externs = 0
  let tests = 0
  for line in lines
    if is_cell_definition(line)
      cells = cells + 1
    end
    if is_record_definition(line)
      records = records + 1
    end
    if is_enum_definition(line)
      enums = enums + 1
    end
    if is_import_statement(line)
      imports = imports + 1
    end
    if is_process_definition(line)
      processes = processes + 1
    end
    if is_effect_declaration(line)
      effects = effects + 1
    end
    if is_grant_statement(line)
      grants = grants + 1
    end
    if is_type_alias(line)
      type_als = type_als + 1
    end
    if is_extern_declaration(line)
      externs = externs + 1
    end
    if is_test_annotation(line)
      tests = tests + 1
    end
  end
  [cells, records, enums, imports, processes, effects, grants, type_als, externs, tests]
end

cell count_rust_defs(lines: list[String]) -> list[Int]
  let fns = 0
  let structs = 0
  let enums = 0
  let traits = 0
  let impls = 0
  let tests = 0
  let uses = 0
  let mods = 0
  let macros = 0
  for line in lines
    if is_rust_fn(line)
      fns = fns + 1
    end
    if is_rust_struct(line)
      structs = structs + 1
    end
    if is_rust_enum(line)
      enums = enums + 1
    end
    if is_rust_trait(line)
      traits = traits + 1
    end
    if is_rust_impl(line)
      impls = impls + 1
    end
    if is_rust_test_attr(line)
      tests = tests + 1
    end
    if is_rust_use(line)
      uses = uses + 1
    end
    if is_rust_mod(line)
      mods = mods + 1
    end
    if is_rust_macro(line)
      macros = macros + 1
    end
  end
  [fns, structs, enums, traits, impls, tests, uses, mods, macros]
end

cell count_line_types_lumen_md(lines: list[String]) -> list[Int]
  let blank = 0
  let comments = 0
  let code = 0
  let max_line_len = 0
  let in_code_block = false

  let i = 0
  while i < len(lines)
    let line = lines[i]
    let line_len = len(line)
    if line_len > max_line_len
      max_line_len = line_len
    end

    let trimmed = trim(line)
    if starts_with(trimmed, "```lumen")
      in_code_block = true
      i = i + 1
      continue
    end
    if in_code_block and starts_with(trimmed, "```")
      in_code_block = false
      i = i + 1
      continue
    end

    if in_code_block
      if is_blank_line(line)
        blank = blank + 1
      else
        if is_lumen_comment(line)
          comments = comments + 1
        else
          code = code + 1
        end
      end
    end

    i = i + 1
  end
  [blank, comments, code, max_line_len]
end

cell extract_code_block_lines(lines: list[String]) -> list[String]
  let result = []
  let in_code_block = false
  for line in lines
    let trimmed = trim(line)
    if starts_with(trimmed, "```lumen")
      in_code_block = true
      continue
    end
    if in_code_block and starts_with(trimmed, "```")
      in_code_block = false
      continue
    end
    if in_code_block
      result = append(result, line)
    end
  end
  result
end

cell count_line_types_general(lines: list[String], file_type: String) -> list[Int]
  let blank = 0
  let comments = 0
  let code = 0
  let max_line_len = 0

  for line in lines
    let line_len = len(line)
    if line_len > max_line_len
      max_line_len = line_len
    end
    if is_blank_line(line)
      blank = blank + 1
    else
      if is_comment_line(line, file_type)
        comments = comments + 1
      else
        code = code + 1
      end
    end
  end
  [blank, comments, code, max_line_len]
end

cell make_empty_stats(path: String, file_type: String) -> FileStats
  FileStats(
    path: path,
    file_type: file_type,
    total_lines: 0,
    blank_lines: 0,
    comment_lines: 0,
    code_lines: 0,
    cell_defs: 0,
    record_defs: 0,
    enum_defs: 0,
    import_stmts: 0,
    fn_defs: 0,
    struct_defs: 0,
    rust_enum_defs: 0,
    trait_defs: 0,
    impl_blocks: 0,
    test_annotations: 0,
    process_defs: 0,
    effect_decls: 0,
    grant_stmts: 0,
    type_aliases: 0,
    extern_decls: 0,
    use_stmts: 0,
    mod_decls: 0,
    macro_defs: 0,
    max_line_length: 0,
    has_lumen_code: false
  )
end

cell analyze_lumen_md_file(path: String) -> FileStats
  let content = read_file(path)
  let lines = split(content, "\n")
  let total = len(lines)
  let line_counts = count_line_types_lumen_md(lines)
  let code_lines_list = extract_code_block_lines(lines)
  let defs = count_lumen_defs(code_lines_list)

  FileStats(
    path: path,
    file_type: "lumen-md",
    total_lines: total,
    blank_lines: line_counts[0],
    comment_lines: line_counts[1],
    code_lines: line_counts[2],
    cell_defs: defs[0],
    record_defs: defs[1],
    enum_defs: defs[2],
    import_stmts: defs[3],
    fn_defs: 0,
    struct_defs: 0,
    rust_enum_defs: 0,
    trait_defs: 0,
    impl_blocks: 0,
    test_annotations: defs[9],
    process_defs: defs[4],
    effect_decls: defs[5],
    grant_stmts: defs[6],
    type_aliases: defs[7],
    extern_decls: defs[8],
    use_stmts: 0,
    mod_decls: 0,
    macro_defs: 0,
    max_line_length: line_counts[3],
    has_lumen_code: true
  )
end

cell analyze_lumen_raw_file(path: String, file_type: String) -> FileStats
  let content = read_file(path)
  let lines = split(content, "\n")
  let total = len(lines)
  let line_counts = count_line_types_general(lines, file_type)
  let defs = count_lumen_defs(lines)

  FileStats(
    path: path,
    file_type: file_type,
    total_lines: total,
    blank_lines: line_counts[0],
    comment_lines: line_counts[1],
    code_lines: line_counts[2],
    cell_defs: defs[0],
    record_defs: defs[1],
    enum_defs: defs[2],
    import_stmts: defs[3],
    fn_defs: 0,
    struct_defs: 0,
    rust_enum_defs: 0,
    trait_defs: 0,
    impl_blocks: 0,
    test_annotations: defs[9],
    process_defs: defs[4],
    effect_decls: defs[5],
    grant_stmts: defs[6],
    type_aliases: defs[7],
    extern_decls: defs[8],
    use_stmts: 0,
    mod_decls: 0,
    macro_defs: 0,
    max_line_length: line_counts[3],
    has_lumen_code: true
  )
end

cell analyze_rust_file(path: String) -> FileStats
  let content = read_file(path)
  let lines = split(content, "\n")
  let total = len(lines)
  let line_counts = count_line_types_general(lines, "rust")
  let defs = count_rust_defs(lines)

  FileStats(
    path: path,
    file_type: "rust",
    total_lines: total,
    blank_lines: line_counts[0],
    comment_lines: line_counts[1],
    code_lines: line_counts[2],
    cell_defs: 0,
    record_defs: 0,
    enum_defs: 0,
    import_stmts: 0,
    fn_defs: defs[0],
    struct_defs: defs[1],
    rust_enum_defs: defs[2],
    trait_defs: defs[3],
    impl_blocks: defs[4],
    test_annotations: defs[5],
    process_defs: 0,
    effect_decls: 0,
    grant_stmts: 0,
    type_aliases: 0,
    extern_decls: 0,
    use_stmts: defs[6],
    mod_decls: defs[7],
    macro_defs: defs[8],
    max_line_length: line_counts[3],
    has_lumen_code: false
  )
end

cell analyze_other_file(path: String) -> FileStats
  let file_type = classify_file(path)
  let content = read_file(path)
  let lines = split(content, "\n")
  let total = len(lines)
  let line_counts = count_line_types_general(lines, file_type)

  FileStats(
    path: path,
    file_type: file_type,
    total_lines: total,
    blank_lines: line_counts[0],
    comment_lines: line_counts[1],
    code_lines: line_counts[2],
    cell_defs: 0,
    record_defs: 0,
    enum_defs: 0,
    import_stmts: 0,
    fn_defs: 0,
    struct_defs: 0,
    rust_enum_defs: 0,
    trait_defs: 0,
    impl_blocks: 0,
    test_annotations: 0,
    process_defs: 0,
    effect_decls: 0,
    grant_stmts: 0,
    type_aliases: 0,
    extern_decls: 0,
    use_stmts: 0,
    mod_decls: 0,
    macro_defs: 0,
    max_line_length: line_counts[3],
    has_lumen_code: false
  )
end

cell is_key_rust_file(path: String) -> Bool
  ends_with(path, "/lib.rs") or ends_with(path, "/main.rs")
end

cell analyze_rust_file_fast(path: String) -> FileStats
  let content = read_file(path)
  let lines = split(content, "\n")
  let total = len(lines)
  let defs = count_rust_defs(lines)

  FileStats(
    path: path,
    file_type: "rust",
    total_lines: total,
    blank_lines: 0,
    comment_lines: 0,
    code_lines: total,
    cell_defs: 0,
    record_defs: 0,
    enum_defs: 0,
    import_stmts: 0,
    fn_defs: defs[0],
    struct_defs: defs[1],
    rust_enum_defs: defs[2],
    trait_defs: defs[3],
    impl_blocks: defs[4],
    test_annotations: defs[5],
    process_defs: 0,
    effect_decls: 0,
    grant_stmts: 0,
    type_aliases: 0,
    extern_decls: 0,
    use_stmts: defs[6],
    mod_decls: defs[7],
    macro_defs: defs[8],
    max_line_length: 0,
    has_lumen_code: false
  )
end

cell analyze_rust_file_lines_only(path: String) -> FileStats
  let content = read_file(path)
  let lines = split(content, "\n")
  let total = len(lines)

  FileStats(
    path: path,
    file_type: "rust",
    total_lines: total,
    blank_lines: 0,
    comment_lines: 0,
    code_lines: total,
    cell_defs: 0,
    record_defs: 0,
    enum_defs: 0,
    import_stmts: 0,
    fn_defs: 0,
    struct_defs: 0,
    rust_enum_defs: 0,
    trait_defs: 0,
    impl_blocks: 0,
    test_annotations: 0,
    process_defs: 0,
    effect_decls: 0,
    grant_stmts: 0,
    type_aliases: 0,
    extern_decls: 0,
    use_stmts: 0,
    mod_decls: 0,
    macro_defs: 0,
    max_line_length: 0,
    has_lumen_code: false
  )
end

cell analyze_file(path: String) -> FileStats
  let file_type = classify_file(path)
  if file_type == "lumen-md"
    return analyze_lumen_md_file(path)
  end
  if file_type == "lumen-raw" or file_type == "lumen-native"
    return analyze_lumen_raw_file(path, file_type)
  end
  if file_type == "rust"
    if is_key_rust_file(path)
      return analyze_rust_file_fast(path)
    end
    return analyze_rust_file_lines_only(path)
  end
  analyze_other_file(path)
end
```

## Directory Scanning

```lumen
cell collect_lumen_md_files(base: String) -> list[String]
  let files = glob("**/*.lm.md")
  let result = []
  let seen = []
  for f in files
    if not contains(f, "/target/") and not contains(f, "node_modules")
      let dup = false
      for s in seen
        if s == f
          dup = true
          break
        end
      end
      if not dup
        seen = append(seen, f)
        result = append(result, f)
      end
    end
  end
  result
end

cell collect_lumen_raw_files(base: String) -> list[String]
  let files = glob("**/*.lm")
  let result = []
  let seen = []
  for f in files
    if not contains(f, "/target/") and not contains(f, "node_modules") and not ends_with(f, ".lm.md")
      let dup = false
      for s in seen
        if s == f
          dup = true
          break
        end
      end
      if not dup
        seen = append(seen, f)
        result = append(result, f)
      end
    end
  end
  result
end

cell collect_lumen_native_files(base: String) -> list[String]
  let files = glob("**/*.lumen")
  let result = []
  let seen = []
  for f in files
    if not contains(f, "/target/") and not contains(f, "node_modules") and f != "./.lumen" and f != ".lumen"
      let dup = false
      for s in seen
        if s == f
          dup = true
          break
        end
      end
      if not dup
        seen = append(seen, f)
        result = append(result, f)
      end
    end
  end
  result
end

cell collect_rust_src_files(dir: String) -> list[String]
  let files = glob(dir + "/**/*.rs")
  let result = []
  let seen = []
  for f in files
    if not contains(f, "/target/")
      let dup = false
      for s in seen
        if s == f
          dup = true
          break
        end
      end
      if not dup
        seen = append(seen, f)
        result = append(result, f)
      end
    end
  end
  result
end

cell collect_toml_files(base: String) -> list[String]
  let files = glob("*.toml")
  let result = []
  for f in files
    if not contains(f, "/target/")
      result = append(result, f)
    end
  end
  result
end

cell collect_markdown_files(base: String) -> list[String]
  let files = glob("*.md")
  let result = []
  for f in files
    if not contains(f, "/target/") and not ends_with(f, ".lm.md")
      result = append(result, f)
    end
  end
  result
end

cell collect_all_project_files() -> list[String]
  let all_files = []

  let lm_md = collect_lumen_md_files(".")
  for f in lm_md
    all_files = append(all_files, f)
  end

  let lm_raw = collect_lumen_raw_files(".")
  for f in lm_raw
    all_files = append(all_files, f)
  end

  let lm_native = collect_lumen_native_files(".")
  for f in lm_native
    all_files = append(all_files, f)
  end

  # Collect Rust files - top-level src for each crate only (to stay under instruction limit)
  let rs_compiler = glob("rust/lumen-compiler/src/*.rs")
  for f in rs_compiler
    all_files = append(all_files, f)
  end

  let rs_compiler_sub = glob("rust/lumen-compiler/src/compiler/*.rs")
  for f in rs_compiler_sub
    all_files = append(all_files, f)
  end

  let rs_vm_files = glob("rust/lumen-vm/src/*.rs")
  for f in rs_vm_files
    all_files = append(all_files, f)
  end

  let rs_vm_sub = glob("rust/lumen-vm/src/vm/*.rs")
  for f in rs_vm_sub
    all_files = append(all_files, f)
  end

  let rs_runtime_files = glob("rust/lumen-runtime/src/*.rs")
  for f in rs_runtime_files
    all_files = append(all_files, f)
  end

  let rs_cli_files = glob("rust/lumen-cli/src/*.rs")
  for f in rs_cli_files
    all_files = append(all_files, f)
  end

  let rs_lsp_files = glob("rust/lumen-lsp/src/*.rs")
  for f in rs_lsp_files
    all_files = append(all_files, f)
  end

  let toml_files = collect_toml_files(".")
  for f in toml_files
    all_files = append(all_files, f)
  end

  let md_files = collect_markdown_files(".")
  for f in md_files
    all_files = append(all_files, f)
  end

  all_files
end
```

## Batch Analysis

```lumen
cell analyze_all_files(files: list[String]) -> list[FileStats]
  let results = []
  let i = 0
  let total = len(files)
  while i < total
    let path = files[i]
    let stats = analyze_file(path)
    results = append(results, stats)
    i = i + 1
  end
  results
end
```

## Aggregation Helpers

```lumen
cell total_lines_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.total_lines
  end
  total
end

cell total_blank_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.blank_lines
  end
  total
end

cell total_comment_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.comment_lines
  end
  total
end

cell total_code_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.code_lines
  end
  total
end

cell total_cells_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.cell_defs
  end
  total
end

cell total_records_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.record_defs
  end
  total
end

cell total_enums_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.enum_defs
  end
  total
end

cell total_imports_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.import_stmts
  end
  total
end

cell total_fns_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.fn_defs
  end
  total
end

cell total_structs_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.struct_defs
  end
  total
end

cell total_rust_enums_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.rust_enum_defs
  end
  total
end

cell total_traits_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.trait_defs
  end
  total
end

cell total_impls_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.impl_blocks
  end
  total
end

cell total_tests_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.test_annotations
  end
  total
end

cell total_processes_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.process_defs
  end
  total
end

cell total_effects_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.effect_decls
  end
  total
end

cell total_grants_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.grant_stmts
  end
  total
end

cell total_type_aliases_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.type_aliases
  end
  total
end

cell total_externs_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.extern_decls
  end
  total
end

cell total_uses_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.use_stmts
  end
  total
end

cell total_mods_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.mod_decls
  end
  total
end

cell total_macros_all(stats: list[FileStats]) -> Int
  let total = 0
  for s in stats
    total = total + s.macro_defs
  end
  total
end
```

## Filtered Aggregation by File Type

```lumen
cell filter_by_type(stats: list[FileStats], file_type: String) -> list[FileStats]
  let result = []
  for s in stats
    if s.file_type == file_type
      result = append(result, s)
    end
  end
  result
end

cell filter_by_category(stats: list[FileStats], category: String) -> list[FileStats]
  let result = []
  for s in stats
    if file_category(s.file_type) == category
      result = append(result, s)
    end
  end
  result
end

cell filter_lumen(stats: list[FileStats]) -> list[FileStats]
  let result = []
  for s in stats
    if s.file_type == "lumen-md" or s.file_type == "lumen-raw" or s.file_type == "lumen-native"
      result = append(result, s)
    end
  end
  result
end

cell filter_rust(stats: list[FileStats]) -> list[FileStats]
  filter_by_type(stats, "rust")
end

cell get_file_types(stats: list[FileStats]) -> list[String]
  let types = []
  for s in stats
    if not str_list_contains(types, s.file_type)
      types = append(types, s.file_type)
    end
  end
  sort(types)
end

cell get_categories(stats: list[FileStats]) -> list[String]
  let cats = []
  for s in stats
    let cat = file_category(s.file_type)
    if not str_list_contains(cats, cat)
      cats = append(cats, cat)
    end
  end
  sort(cats)
end
```

## Top Files Analysis

```lumen
cell get_line_counts(stats: list[FileStats]) -> list[Int]
  let counts = []
  for s in stats
    counts = append(counts, s.total_lines)
  end
  counts
end

cell find_largest_files(stats: list[FileStats], n: Int) -> list[FileStats]
  let result = []
  let used = []
  let count = 0
  while count < n and count < len(stats)
    let best_idx = -1
    let best_lines = -1
    let i = 0
    while i < len(stats)
      if not str_list_contains(used, to_string(i))
        if stats[i].total_lines > best_lines
          best_lines = stats[i].total_lines
          best_idx = i
        end
      end
      i = i + 1
    end
    if best_idx >= 0
      result = append(result, stats[best_idx])
      used = append(used, to_string(best_idx))
    end
    count = count + 1
  end
  result
end

cell find_most_complex_lumen(stats: list[FileStats], n: Int) -> list[FileStats]
  let lumen = filter_lumen(stats)
  let result = []
  let used = []
  let count = 0
  while count < n and count < len(lumen)
    let best_idx = -1
    let best_count = -1
    let i = 0
    while i < len(lumen)
      if not str_list_contains(used, to_string(i))
        let complexity = lumen[i].cell_defs + lumen[i].record_defs + lumen[i].enum_defs
        if complexity > best_count
          best_count = complexity
          best_idx = i
        end
      end
      i = i + 1
    end
    if best_idx >= 0
      result = append(result, lumen[best_idx])
      used = append(used, to_string(best_idx))
    end
    count = count + 1
  end
  result
end

cell find_most_complex_rust(stats: list[FileStats], n: Int) -> list[FileStats]
  let rust = filter_rust(stats)
  let result = []
  let used = []
  let count = 0
  while count < n and count < len(rust)
    let best_idx = -1
    let best_count = -1
    let i = 0
    while i < len(rust)
      if not str_list_contains(used, to_string(i))
        let complexity = rust[i].fn_defs + rust[i].struct_defs + rust[i].rust_enum_defs + rust[i].trait_defs
        if complexity > best_count
          best_count = complexity
          best_idx = i
        end
      end
      i = i + 1
    end
    if best_idx >= 0
      result = append(result, rust[best_idx])
      used = append(used, to_string(best_idx))
    end
    count = count + 1
  end
  result
end
```

## Table Generation Helpers

```lumen
cell make_table(headers: list[String], rows: list[list[String]]) -> String
  let num_cols = len(headers)
  let widths = []
  let ci = 0
  while ci < num_cols
    widths = append(widths, len(headers[ci]))
    ci = ci + 1
  end

  let ri = 0
  while ri < len(rows)
    let row = rows[ri]
    let new_widths = []
    let ci2 = 0
    while ci2 < num_cols
      let cur = widths[ci2]
      if ci2 < len(row)
        new_widths = append(new_widths, max_int(cur, len(row[ci2])))
      else
        new_widths = append(new_widths, cur)
      end
      ci2 = ci2 + 1
    end
    widths = new_widths
    ri = ri + 1
  end

  let header_cells = []
  let hi = 0
  while hi < num_cols
    header_cells = append(header_cells, pad_right(headers[hi], widths[hi]))
    hi = hi + 1
  end
  let header_line = "| " + join(header_cells, " | ") + " |"

  let sep_cells = []
  let si = 0
  while si < num_cols
    sep_cells = append(sep_cells, repeat_char("-", widths[si]))
    si = si + 1
  end
  let sep_line = "| " + join(sep_cells, " | ") + " |"

  let lines = [header_line, sep_line]
  let di = 0
  while di < len(rows)
    let row = rows[di]
    let cells = []
    let ci3 = 0
    while ci3 < num_cols
      let val = ""
      if ci3 < len(row)
        val = row[ci3]
      end
      cells = append(cells, pad_right(val, widths[ci3]))
      ci3 = ci3 + 1
    end
    lines = append(lines, "| " + join(cells, " | ") + " |")
    di = di + 1
  end

  join(lines, "\n")
end

cell make_bar(value: Int, max_value: Int, width: Int) -> String
  if max_value == 0
    return repeat_char(" ", width)
  end
  let bar_len = (value * width) / max_value
  if bar_len > width
    bar_len = width
  end
  if bar_len < 1 and value > 0
    bar_len = 1
  end
  let bar = repeat_char("#", bar_len)
  let space = repeat_char(" ", width - bar_len)
  bar + space
end
```

## Report Section Generators

```lumen
cell generate_header_section() -> String
  let lines = []
  lines = append(lines, "==========================================================")
  lines = append(lines, "           LUMEN PROJECT SOURCE CODE ANALYSIS")
  lines = append(lines, "==========================================================")
  lines = append(lines, "")
  join(lines, "\n")
end

cell generate_summary_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  OVERALL SUMMARY")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let total_files = len(stats)
  let total_lines = total_lines_all(stats)
  let total_blank = total_blank_all(stats)
  let total_comments = total_comment_all(stats)
  let total_code = total_code_all(stats)

  lines = append(lines, "  Total files analyzed:  " + format_thousands(total_files))
  lines = append(lines, "  Total lines:           " + format_thousands(total_lines))
  lines = append(lines, "  Code lines:            " + format_thousands(total_code) + " (" + format_percent(total_code, total_lines) + ")")
  lines = append(lines, "  Comment lines:         " + format_thousands(total_comments) + " (" + format_percent(total_comments, total_lines) + ")")
  lines = append(lines, "  Blank lines:           " + format_thousands(total_blank) + " (" + format_percent(total_blank, total_lines) + ")")
  lines = append(lines, "")

  let avg_lines = 0
  if total_files > 0
    avg_lines = total_lines / total_files
  end
  lines = append(lines, "  Average lines/file:    " + format_thousands(avg_lines))
  lines = append(lines, "")

  join(lines, "\n")
end

cell generate_file_type_table(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  FILES BY TYPE")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let types = get_file_types(stats)
  let headers = ["File Type", "Count", "Lines", "Code", "Comments", "Blank", "Code%"]
  let rows = []

  for ft in types
    let filtered = filter_by_type(stats, ft)
    let count = len(filtered)
    let tl = total_lines_all(filtered)
    let tc = total_code_all(filtered)
    let tcm = total_comment_all(filtered)
    let tb = total_blank_all(filtered)
    let pct = format_percent(tc, tl)
    rows = append(rows, [ft, to_string(count), format_thousands(tl), format_thousands(tc), format_thousands(tcm), format_thousands(tb), pct])
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end

cell generate_category_table(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  FILES BY CATEGORY")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let cats = get_categories(stats)
  let headers = ["Category", "Files", "Total Lines", "Code Lines", "Pct of Total"]
  let rows = []
  let grand_total = total_lines_all(stats)

  for cat in cats
    let filtered = filter_by_category(stats, cat)
    let count = len(filtered)
    let tl = total_lines_all(filtered)
    let tc = total_code_all(filtered)
    let pct = format_percent(tl, grand_total)
    rows = append(rows, [cat, to_string(count), format_thousands(tl), format_thousands(tc), pct])
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end
```

## Lumen-Specific Report Sections

```lumen
cell make_construct_row(label: String, count: Int, max_val: Int) -> list[String]
  [label, to_string(count), make_bar(count, max_val, 30)]
end

cell lumen_construct_rows(lumen: list[FileStats]) -> list[list[String]]
  let tc = total_cells_all(lumen)
  let tr = total_records_all(lumen)
  let te = total_enums_all(lumen)
  let ti = total_imports_all(lumen)
  let tp = total_processes_all(lumen)
  let max_val = max_int(tc, max_int(tr, max_int(te, max_int(ti, tp))))

  let rows = []
  rows = append(rows, make_construct_row("cell definitions", tc, max_val))
  rows = append(rows, make_construct_row("record definitions", tr, max_val))
  rows = append(rows, make_construct_row("enum definitions", te, max_val))
  rows = append(rows, make_construct_row("import statements", ti, max_val))
  rows = append(rows, make_construct_row("process definitions", tp, max_val))
  rows
end

cell lumen_construct_rows2(lumen: list[FileStats], max_val: Int) -> list[list[String]]
  let te = total_effects_all(lumen)
  let tg = total_grants_all(lumen)
  let ta = total_type_aliases_all(lumen)
  let tx = total_externs_all(lumen)
  let tt = total_tests_all(lumen)

  let rows = []
  rows = append(rows, make_construct_row("effect declarations", te, max_val))
  rows = append(rows, make_construct_row("grant statements", tg, max_val))
  rows = append(rows, make_construct_row("type aliases", ta, max_val))
  rows = append(rows, make_construct_row("extern declarations", tx, max_val))
  rows = append(rows, make_construct_row("@test annotations", tt, max_val))
  rows
end

cell generate_lumen_constructs_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  LUMEN LANGUAGE CONSTRUCTS")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let lumen = filter_lumen(stats)
  let tc = total_cells_all(lumen)
  let tr = total_records_all(lumen)
  let te = total_enums_all(lumen)
  let ti = total_imports_all(lumen)
  let tp = total_processes_all(lumen)
  let max_val = max_int(tc, max_int(tr, max_int(te, max_int(ti, tp))))

  let headers = ["Construct", "Count", "Bar"]
  let rows = lumen_construct_rows(lumen)
  let rows2 = lumen_construct_rows2(lumen, max_val)
  for r in rows2
    rows = append(rows, r)
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end

cell generate_lumen_file_detail_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  LUMEN FILE DETAILS")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let lumen = filter_lumen(stats)

  if len(lumen) == 0
    lines = append(lines, "  No Lumen source files found.")
    lines = append(lines, "")
    return join(lines, "\n")
  end

  let headers = ["File", "Lines", "Code", "Cells", "Records", "Enums", "Imports"]
  let rows = []

  for s in lumen
    let short_path = s.path
    if len(short_path) > 45
      short_path = truncate_string(short_path, 45)
    end
    rows = append(rows, [
      short_path,
      to_string(s.total_lines),
      to_string(s.code_lines),
      to_string(s.cell_defs),
      to_string(s.record_defs),
      to_string(s.enum_defs),
      to_string(s.import_stmts)
    ])
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end
```

## Rust-Specific Report Sections

```lumen
cell rust_construct_rows(rust: list[FileStats]) -> list[list[String]]
  let tf = total_fns_all(rust)
  let ts = total_structs_all(rust)
  let te = total_rust_enums_all(rust)
  let tt = total_traits_all(rust)
  let ti = total_impls_all(rust)
  let max_val = max_int(tf, max_int(ts, max_int(te, max_int(tt, ti))))

  let rows = []
  rows = append(rows, make_construct_row("fn definitions", tf, max_val))
  rows = append(rows, make_construct_row("struct definitions", ts, max_val))
  rows = append(rows, make_construct_row("enum definitions", te, max_val))
  rows = append(rows, make_construct_row("trait definitions", tt, max_val))
  rows = append(rows, make_construct_row("impl blocks", ti, max_val))
  rows
end

cell rust_construct_rows2(rust: list[FileStats], max_val: Int) -> list[list[String]]
  let tt = total_tests_all(rust)
  let tu = total_uses_all(rust)
  let tm = total_mods_all(rust)
  let tma = total_macros_all(rust)

  let rows = []
  rows = append(rows, make_construct_row("#[test] annotations", tt, max_val))
  rows = append(rows, make_construct_row("use statements", tu, max_val))
  rows = append(rows, make_construct_row("mod declarations", tm, max_val))
  rows = append(rows, make_construct_row("macro_rules! defs", tma, max_val))
  rows
end

cell generate_rust_constructs_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  RUST LANGUAGE CONSTRUCTS")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let rust = filter_rust(stats)
  let tf = total_fns_all(rust)
  let ts = total_structs_all(rust)
  let te = total_rust_enums_all(rust)
  let tt = total_traits_all(rust)
  let ti = total_impls_all(rust)
  let max_val = max_int(tf, max_int(ts, max_int(te, max_int(tt, ti))))

  let headers = ["Construct", "Count", "Bar"]
  let rows = rust_construct_rows(rust)
  let rows2 = rust_construct_rows2(rust, max_val)
  for r in rows2
    rows = append(rows, r)
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end

cell generate_rust_crate_breakdown(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  RUST CRATE BREAKDOWN")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let crates = ["lumen-compiler", "lumen-vm", "lumen-runtime", "lumen-cli", "lumen-lsp"]
  let headers = ["Crate", "Files", "Lines", "Code", "Fns", "Structs", "Enums", "Traits"]
  let rows = []

  for crate_name in crates
    let crate_stats = []
    for s in stats
      if contains(s.path, crate_name) and s.file_type == "rust"
        crate_stats = append(crate_stats, s)
      end
    end

    let file_count = len(crate_stats)
    let tl = total_lines_all(crate_stats)
    let tc = total_code_all(crate_stats)
    let tf = total_fns_all(crate_stats)
    let ts = total_structs_all(crate_stats)
    let te = total_rust_enums_all(crate_stats)
    let tt = total_traits_all(crate_stats)

    rows = append(rows, [
      crate_name,
      to_string(file_count),
      format_thousands(tl),
      format_thousands(tc),
      to_string(tf),
      to_string(ts),
      to_string(te),
      to_string(tt)
    ])
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end
```

## Top-N Report Sections

```lumen
cell generate_top_files_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  TOP 15 LARGEST FILES")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let top = find_largest_files(stats, 15)
  let headers = ["Rank", "File", "Lines", "Code", "Type"]
  let rows = []

  let i = 0
  while i < len(top)
    let s = top[i]
    let short_path = s.path
    if len(short_path) > 50
      short_path = truncate_string(short_path, 50)
    end
    rows = append(rows, [
      to_string(i + 1),
      short_path,
      format_thousands(s.total_lines),
      format_thousands(s.code_lines),
      s.file_type
    ])
    i = i + 1
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end

cell generate_top_lumen_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  TOP 10 MOST COMPLEX LUMEN FILES")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let top = find_most_complex_lumen(stats, 10)

  if len(top) == 0
    lines = append(lines, "  No Lumen files found.")
    lines = append(lines, "")
    return join(lines, "\n")
  end

  let headers = ["Rank", "File", "Cells", "Records", "Enums", "Total Defs"]
  let rows = []

  let i = 0
  while i < len(top)
    let s = top[i]
    let short_path = s.path
    if len(short_path) > 45
      short_path = truncate_string(short_path, 45)
    end
    let total_defs = s.cell_defs + s.record_defs + s.enum_defs
    rows = append(rows, [
      to_string(i + 1),
      short_path,
      to_string(s.cell_defs),
      to_string(s.record_defs),
      to_string(s.enum_defs),
      to_string(total_defs)
    ])
    i = i + 1
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end

cell generate_top_rust_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  TOP 10 MOST COMPLEX RUST FILES")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let top = find_most_complex_rust(stats, 10)

  if len(top) == 0
    lines = append(lines, "  No Rust source files found.")
    lines = append(lines, "")
    return join(lines, "\n")
  end

  let headers = ["Rank", "File", "Fns", "Structs", "Enums", "Traits", "Total"]
  let rows = []

  let i = 0
  while i < len(top)
    let s = top[i]
    let short_path = s.path
    if len(short_path) > 45
      short_path = truncate_string(short_path, 45)
    end
    let total_defs = s.fn_defs + s.struct_defs + s.rust_enum_defs + s.trait_defs
    rows = append(rows, [
      to_string(i + 1),
      short_path,
      to_string(s.fn_defs),
      to_string(s.struct_defs),
      to_string(s.rust_enum_defs),
      to_string(s.trait_defs),
      to_string(total_defs)
    ])
    i = i + 1
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end
```

## Code Quality Metrics

```lumen
cell compute_comment_ratio(stats: list[FileStats]) -> String
  let total_code = total_code_all(stats)
  let total_comments = total_comment_all(stats)
  if total_code == 0
    return "N/A"
  end
  format_ratio(total_comments, total_code)
end

cell compute_avg_fn_size(stats: list[FileStats]) -> Int
  let rust = filter_rust(stats)
  let total_fns = total_fns_all(rust)
  let total_code = total_code_all(rust)
  if total_fns == 0
    return 0
  end
  total_code / total_fns
end

cell compute_avg_cell_size(stats: list[FileStats]) -> Int
  let lumen = filter_lumen(stats)
  let total_cells = total_cells_all(lumen)
  let total_code = total_code_all(lumen)
  if total_cells == 0
    return 0
  end
  total_code / total_cells
end

cell compute_max_line_len(stats: list[FileStats]) -> Int
  let max_len = 0
  for s in stats
    if s.max_line_length > max_len
      max_len = s.max_line_length
    end
  end
  max_len
end

cell find_longest_line_file(stats: list[FileStats]) -> String
  let max_len = 0
  let max_path = ""
  for s in stats
    if s.max_line_length > max_len
      max_len = s.max_line_length
      max_path = s.path
    end
  end
  max_path
end

cell generate_quality_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  CODE QUALITY METRICS")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let comment_ratio = compute_comment_ratio(stats)
  let avg_fn = compute_avg_fn_size(stats)
  let avg_cell = compute_avg_cell_size(stats)
  let max_line = compute_max_line_len(stats)
  let longest_file = find_longest_line_file(stats)
  let total_code = total_code_all(stats)
  let total_lines = total_lines_all(stats)
  let code_density = format_percent(total_code, total_lines)

  lines = append(lines, "  Comment-to-code ratio:    " + comment_ratio)
  lines = append(lines, "  Code density:             " + code_density)
  lines = append(lines, "  Avg lines/Rust fn:        " + to_string(avg_fn))
  lines = append(lines, "  Avg lines/Lumen cell:     " + to_string(avg_cell))
  lines = append(lines, "  Max line length:          " + to_string(max_line) + " chars")
  lines = append(lines, "  Longest line in:          " + longest_file)
  lines = append(lines, "")

  lines = append(lines, generate_quality_comparison_table(stats, comment_ratio))

  join(lines, "\n")
end

cell generate_quality_comparison_table(stats: list[FileStats], overall_ratio: String) -> String
  let headers = ["Metric", "Rust", "Lumen", "All"]
  let rows = []

  let rust = filter_rust(stats)
  let lumen = filter_lumen(stats)

  rows = append(rows, [
    "Files",
    to_string(len(rust)),
    to_string(len(lumen)),
    to_string(len(stats))
  ])
  rows = append(rows, [
    "Code lines",
    format_thousands(total_code_all(rust)),
    format_thousands(total_code_all(lumen)),
    format_thousands(total_code_all(stats))
  ])
  rows = append(rows, [
    "Comment lines",
    format_thousands(total_comment_all(rust)),
    format_thousands(total_comment_all(lumen)),
    format_thousands(total_comment_all(stats))
  ])
  rows = append(rows, [
    "Blank lines",
    format_thousands(total_blank_all(rust)),
    format_thousands(total_blank_all(lumen)),
    format_thousands(total_blank_all(stats))
  ])
  rows = append(rows, [
    "Comment ratio",
    compute_comment_ratio(rust),
    compute_comment_ratio(lumen),
    overall_ratio
  ])

  let result = make_table(headers, rows)
  result + "\n"
end
```

## Codebase Health Indicators

```lumen
cell count_files_over_threshold(stats: list[FileStats], threshold: Int) -> Int
  let count = 0
  for s in stats
    if s.total_lines > threshold
      count = count + 1
    end
  end
  count
end

cell count_files_with_no_comments(stats: list[FileStats]) -> Int
  let count = 0
  for s in stats
    if s.comment_lines == 0 and s.code_lines > 10
      count = count + 1
    end
  end
  count
end

cell count_files_with_long_lines(stats: list[FileStats], threshold: Int) -> Int
  let count = 0
  for s in stats
    if s.max_line_length > threshold
      count = count + 1
    end
  end
  count
end

cell health_status(count: Int, warn_threshold: Int, high_threshold: Int) -> String
  if count > high_threshold
    return "HIGH"
  end
  if count > warn_threshold
    return "WARN"
  end
  "OK"
end

cell generate_health_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  CODEBASE HEALTH INDICATORS")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let large_500 = count_files_over_threshold(stats, 500)
  let large_1000 = count_files_over_threshold(stats, 1000)
  let large_2000 = count_files_over_threshold(stats, 2000)
  let large_5000 = count_files_over_threshold(stats, 5000)
  let no_comments = count_files_with_no_comments(stats)
  let long_lines_120 = count_files_with_long_lines(stats, 120)
  let long_lines_200 = count_files_with_long_lines(stats, 200)

  let headers = ["Indicator", "Count", "Status"]
  let rows = []

  rows = append(rows, ["Files > 500 lines", to_string(large_500), "INFO"])
  rows = append(rows, ["Files > 1000 lines", to_string(large_1000), health_status(large_1000, 20, 50)])
  rows = append(rows, ["Files > 2000 lines", to_string(large_2000), "INFO"])
  rows = append(rows, ["Files > 5000 lines", to_string(large_5000), health_status(large_5000, 2, 5)])
  rows = append(rows, ["Files with no comments", to_string(no_comments), health_status(no_comments, 10, 30)])
  rows = append(rows, ["Files with lines > 120ch", to_string(long_lines_120), "INFO"])
  rows = append(rows, ["Files with lines > 200ch", to_string(long_lines_200), health_status(long_lines_200, 10, 30)])

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end
```

## Distribution Analysis

```lumen
cell compute_size_buckets(stats: list[FileStats]) -> list[Int]
  let b0 = 0
  let b1 = 0
  let b2 = 0
  let b3 = 0
  let b4 = 0
  let b5 = 0
  let b6 = 0

  for s in stats
    let tl = s.total_lines
    if tl <= 50
      b0 = b0 + 1
    else
      if tl <= 100
        b1 = b1 + 1
      else
        if tl <= 250
          b2 = b2 + 1
        else
          if tl <= 500
            b3 = b3 + 1
          else
            if tl <= 1000
              b4 = b4 + 1
            else
              if tl <= 2000
                b5 = b5 + 1
              else
                b6 = b6 + 1
              end
            end
          end
        end
      end
    end
  end
  [b0, b1, b2, b3, b4, b5, b6]
end

cell max_of_buckets(buckets: list[Int]) -> Int
  let result = 0
  for b in buckets
    if b > result
      result = b
    end
  end
  result
end

cell generate_size_distribution(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  FILE SIZE DISTRIBUTION")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let buckets = compute_size_buckets(stats)
  let total = len(stats)
  let max_bucket = max_of_buckets(buckets)

  let labels = ["0-50 lines", "51-100 lines", "101-250 lines", "251-500 lines", "501-1000 lines", "1001-2000 lines", "2000+ lines"]
  let headers = ["Size Range", "Count", "Pct", "Distribution"]
  let rows = []

  let i = 0
  while i < len(labels)
    let count = buckets[i]
    rows = append(rows, [labels[i], to_string(count), format_percent(count, total), make_bar(count, max_bucket, 25)])
    i = i + 1
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end
```

## Complexity Score

```lumen
cell compute_file_complexity(s: FileStats) -> Int
  let score = 0
  score = score + s.cell_defs * 3
  score = score + s.record_defs * 2
  score = score + s.enum_defs * 2
  score = score + s.fn_defs * 3
  score = score + s.struct_defs * 2
  score = score + s.rust_enum_defs * 2
  score = score + s.trait_defs * 4
  score = score + s.impl_blocks * 2
  score = score + s.process_defs * 5
  score = score + s.effect_decls * 4
  score = score + s.macro_defs * 3

  if s.total_lines > 1000
    score = score + 5
  end
  if s.total_lines > 2000
    score = score + 10
  end
  if s.total_lines > 5000
    score = score + 20
  end

  score
end

cell generate_complexity_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  COMPLEXITY SCORES (Top 15)")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let result = []
  let used = []
  let count = 0
  while count < 15 and count < len(stats)
    let best_idx = -1
    let best_score = -1
    let i = 0
    while i < len(stats)
      if not str_list_contains(used, to_string(i))
        let score = compute_file_complexity(stats[i])
        if score > best_score
          best_score = score
          best_idx = i
        end
      end
      i = i + 1
    end
    if best_idx >= 0
      result = append(result, stats[best_idx])
      used = append(used, to_string(best_idx))
    end
    count = count + 1
  end

  let headers = ["Rank", "File", "Score", "Lines", "Type"]
  let rows = []

  let i = 0
  while i < len(result)
    let s = result[i]
    let short_path = s.path
    if len(short_path) > 45
      short_path = truncate_string(short_path, 45)
    end
    let score = compute_file_complexity(s)
    rows = append(rows, [
      to_string(i + 1),
      short_path,
      to_string(score),
      format_thousands(s.total_lines),
      s.file_type
    ])
    i = i + 1
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  join(lines, "\n")
end
```

## Project Statistics Summary

```lumen
cell generate_lumen_project_stats(stats: list[FileStats]) -> String
  let lumen = filter_lumen(stats)
  let lumen_code = total_code_all(lumen)
  let lumen_cells = total_cells_all(lumen)
  let lumen_records = total_records_all(lumen)
  let lumen_enums = total_enums_all(lumen)

  let lines = []
  lines = append(lines, "  LUMEN LANGUAGE STATS:")
  lines = append(lines, "    Source files:     " + to_string(len(lumen)))
  lines = append(lines, "    Code lines:       " + format_thousands(lumen_code))
  lines = append(lines, "    Cells defined:    " + to_string(lumen_cells))
  lines = append(lines, "    Records defined:  " + to_string(lumen_records))
  lines = append(lines, "    Enums defined:    " + to_string(lumen_enums))
  lines = append(lines, "")
  join(lines, "\n")
end

cell generate_rust_project_stats(stats: list[FileStats]) -> String
  let rust = filter_rust(stats)
  let rust_code = total_code_all(rust)
  let rust_fns = total_fns_all(rust)
  let rust_structs = total_structs_all(rust)
  let rust_enums = total_rust_enums_all(rust)

  let lines = []
  lines = append(lines, "  RUST IMPLEMENTATION STATS:")
  lines = append(lines, "    Source files:     " + to_string(len(rust)))
  lines = append(lines, "    Code lines:       " + format_thousands(rust_code))
  lines = append(lines, "    Functions:        " + to_string(rust_fns))
  lines = append(lines, "    Structs:          " + to_string(rust_structs))
  lines = append(lines, "    Enums:            " + to_string(rust_enums))
  lines = append(lines, "")
  join(lines, "\n")
end

cell generate_ratio_stats(stats: list[FileStats]) -> String
  let lumen = filter_lumen(stats)
  let rust = filter_rust(stats)
  let lumen_code = total_code_all(lumen)
  let rust_code = total_code_all(rust)
  let total_code = total_code_all(stats)

  let lines = []
  lines = append(lines, "  RATIO:")
  if lumen_code > 0
    lines = append(lines, "    Rust:Lumen code ratio:  " + format_ratio(rust_code, lumen_code) + ":1")
  end
  if total_code > 0
    lines = append(lines, "    Lumen % of total code:  " + format_percent(lumen_code, total_code))
    lines = append(lines, "    Rust % of total code:   " + format_percent(rust_code, total_code))
  end
  lines = append(lines, "")
  join(lines, "\n")
end

cell generate_project_stats(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  PROJECT STATISTICS")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  lines = append(lines, generate_lumen_project_stats(stats))
  lines = append(lines, generate_rust_project_stats(stats))
  lines = append(lines, generate_ratio_stats(stats))

  join(lines, "\n")
end
```

## Dogfood Tools Analysis

```lumen
cell generate_tools_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  LUMEN DOGFOOD TOOLS ANALYSIS")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let tools = []
  for s in stats
    if contains(s.path, "tools/") and s.file_type == "lumen-md"
      tools = append(tools, s)
    end
  end

  if len(tools) == 0
    lines = append(lines, "  No tool files found in tools/ directory.")
    lines = append(lines, "")
    return join(lines, "\n")
  end

  let headers = ["Tool", "Lines", "Code", "Cells", "Records"]
  let rows = []

  for s in tools
    let name = s.path
    if contains(name, "/")
      let parts = split(name, "/")
      name = parts[len(parts) - 1]
    end
    rows = append(rows, [
      name,
      to_string(s.total_lines),
      to_string(s.code_lines),
      to_string(s.cell_defs),
      to_string(s.record_defs)
    ])
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  let tool_total_lines = total_lines_all(tools)
  let tool_total_code = total_code_all(tools)
  let tool_total_cells = total_cells_all(tools)

  lines = append(lines, "  Total tool files:       " + to_string(len(tools)))
  lines = append(lines, "  Total tool lines:       " + format_thousands(tool_total_lines))
  lines = append(lines, "  Total tool code lines:  " + format_thousands(tool_total_code))
  lines = append(lines, "  Total tool cells:       " + to_string(tool_total_cells))
  lines = append(lines, "")

  join(lines, "\n")
end
```

## Examples Analysis

```lumen
cell generate_examples_section(stats: list[FileStats]) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  EXAMPLES ANALYSIS")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")

  let examples = []
  for s in stats
    if contains(s.path, "examples/")
      examples = append(examples, s)
    end
  end

  if len(examples) == 0
    lines = append(lines, "  No example files found.")
    lines = append(lines, "")
    return join(lines, "\n")
  end

  let headers = ["Example", "Lines", "Code", "Cells", "Records", "Enums"]
  let rows = []

  for s in examples
    let name = s.path
    if contains(name, "/")
      let parts = split(name, "/")
      name = parts[len(parts) - 1]
    end
    rows = append(rows, [
      name,
      to_string(s.total_lines),
      to_string(s.code_lines),
      to_string(s.cell_defs),
      to_string(s.record_defs),
      to_string(s.enum_defs)
    ])
  end

  lines = append(lines, make_table(headers, rows))
  lines = append(lines, "")

  let ex_total_lines = total_lines_all(examples)
  let ex_total_code = total_code_all(examples)
  let ex_total_cells = total_cells_all(examples)

  lines = append(lines, "  Total examples:         " + to_string(len(examples)))
  lines = append(lines, "  Total example lines:    " + format_thousands(ex_total_lines))
  lines = append(lines, "  Total example cells:    " + to_string(ex_total_cells))
  lines = append(lines, "")

  join(lines, "\n")
end
```

## Timing and Footer

```lumen
cell generate_footer(elapsed_ms: Int) -> String
  let lines = []
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "  ANALYSIS COMPLETE")
  lines = append(lines, "----------------------------------------------------------")
  lines = append(lines, "")
  lines = append(lines, "  Analysis completed in ~" + to_string(elapsed_ms) + "ms")
  lines = append(lines, "  Generated by: tools/lumen_analyzer.lm.md")
  lines = append(lines, "")
  lines = append(lines, "==========================================================")
  join(lines, "\n")
end
```

## Main Entry Point

```lumen
cell main() -> Null
  let start_time = hrtime()

  print(generate_header_section())

  print("  Scanning project files...")
  let files = collect_all_project_files()
  print("  Found " + to_string(len(files)) + " files to analyze.")
  print("")

  print("  Analyzing files...")
  let stats = analyze_all_files(files)
  print("  Analysis complete.")
  print("")

  print(generate_summary_section(stats))
  print(generate_file_type_table(stats))
  print(generate_category_table(stats))
  print(generate_lumen_constructs_section(stats))
  print(generate_lumen_file_detail_section(stats))
  print(generate_rust_constructs_section(stats))
  print(generate_rust_crate_breakdown(stats))
  print(generate_top_files_section(stats))
  print(generate_top_lumen_section(stats))
  print(generate_top_rust_section(stats))
  print(generate_quality_section(stats))
  print(generate_health_section(stats))
  print(generate_size_distribution(stats))
  print(generate_complexity_section(stats))
  print(generate_project_stats(stats))
  print(generate_tools_section(stats))

  let end_time = hrtime()
  let elapsed = to_int(end_time - start_time)
  print(generate_footer(elapsed))

  null
end
```
