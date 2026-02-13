```lumen
cell main() -> Int
  let name = "Allie"
  
  # 1. Role block as expression
  let r = role assistant: I am {name}'s assistant. end
  print(r)
  
  # 2. Inline role block in call
  print(role system: Confirm {name}.)
  
  # 3. Tool call style (using print as tool placeholder)
  # syntax: tool name(args)
  # But parser expects 'tool' keyword.
  # If I use 'tool' keyword, it lowers to ToolCall opcode.
  # VM ToolCall just prints placeholder.
  # So I can't verify interpolation with 'tool' keyword easily unless I inspect registers?
  # But I can rely on 'print' (Call opcode) to verify logic.
  
  return 0
end
```
