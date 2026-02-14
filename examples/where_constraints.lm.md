```lumen
record Account
  balance: Int where balance >= 0
  age: Int where age > 18
end

cell main() -> Int
  let a1 = Account(balance: 100, age: 25)
  print("Account 1 created")
  
  # Should fail
  # let a2 = Account(balance: -10, age: 30)
  
  # Should fail
  # let a3 = Account(balance: 50, age: 10)
  
  return 0
end
```
