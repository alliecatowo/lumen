# Typecheck Pass

```lumen
enum Status
  Open
  Closed
end

record Item
  id: Int
  name: String
  status: Status
end

cell main() -> Int
  let i = Item(id: 1, name: "test", status: Open)
  let s = i.status
  
  match s
    Open -> print("It is open")
    Closed -> print("It is closed")
  end
  
  print("Checking res...")
  let res = check_res(true)
  match res
    ok(v) -> print("OK: " + string(v))
    err(e) -> print("Error: " + e)
  end
  print("Done checking res")
  
  return 0
end

cell check_res(success: Bool) -> result[Int, String]
  if success
    return ok(100)
  else
    print("Returning err")
    return err("Failure")
  end
end
```
