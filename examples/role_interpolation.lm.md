```lumen
cell main() -> Int
  let name = "Allie"

  # Role block with string interpolation.
  # NOTE: Role block inline interpolation has a known tokenization issue
  # where spaces around {expr} are lost. Use string interpolation for
  # correct results until the role-content lexer is updated.
  let greeting = "I am {name}'s assistant."
  let r = role assistant: {greeting} end
  print(r)

  # Inline role block in a call — use pre-built string to avoid spacing bug
  let confirm = "Confirm {name}."
  print(role system: {confirm})

  return 0
end
```
