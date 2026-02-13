```lumen
record User
  id: Int
  name: String
end

cell main() -> Int
  # 1. Valid
  let u1 = User(id: 1, name: "Alice")
  
  # 2. Missing field
  # let u2 = User(id: 2) 
  
  # 3. Wrong type
  # let u3 = User(id: "3", name: "Bob")
  
  # 4. Unknown field
  # let u4 = User(id: 4, name: "Dave", age: 30)

  return 0
end
```
