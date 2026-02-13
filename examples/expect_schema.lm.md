```lumen
record User
  id: Int
end

cell main() -> Int
  let u = User(id: 1)
  
  # Valid expected schema
  u expect schema User
  print("User validated successfully")
  
  # Valid primitive
  let i = 10
  i expect schema Int
  print("Int validated successfully")
  
  # Invalid (uncomment to test failure)
  # i expect schema String
  
  return 0
end
```
