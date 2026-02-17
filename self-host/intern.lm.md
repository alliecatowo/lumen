# String Interning

A string interner maps strings to integer indices for compact storage in
the LIR string table. Identical strings share the same index. The
interner provides O(1) lookup in both directions.

```lumen
record StringInterner(
  to_index: map[String, Int],
  to_string: list[String]
)

cell new_interner() -> StringInterner
  StringInterner(to_index: {}, to_string: [])
end

cell intern(interner: StringInterner, s: String) -> tuple[Int, StringInterner]
  if contains(interner.to_index, s) then
    let idx = interner.to_index[s]
    (idx, interner)
  else
    let idx = length(interner.to_string)
    let new_map = merge(interner.to_index, {s: idx})
    let new_list = append(interner.to_string, s)
    (idx, StringInterner(to_index: new_map, to_string: new_list))
  end
end

cell resolve(interner: StringInterner, index: Int) -> String
  interner.to_string[index]
end

cell intern_len(interner: StringInterner) -> Int
  length(interner.to_string)
end
```

## Batch Interning

Intern multiple strings at once, returning the updated interner and
the list of indices.

```lumen
cell intern_all(interner: StringInterner, strings: list[String]) -> tuple[list[Int], StringInterner]
  let indices = []
  let current = interner
  for s in strings
    let (idx, updated) = intern(current, s)
    indices = append(indices, idx)
    current = updated
  end
  (indices, current)
end
```

## String Table Export

Convert the interner to a flat list of strings for inclusion in the
LIR module output.

```lumen
cell to_string_table(interner: StringInterner) -> list[String]
  interner.to_string
end

cell from_string_table(strings: list[String]) -> StringInterner
  let to_index = {}
  let i = 0
  for s in strings
    to_index = merge(to_index, {s: i})
    i = i + 1
  end
  StringInterner(to_index: to_index, to_string: strings)
end
```
