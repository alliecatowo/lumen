# Tutorial: Data Structures

Learn how to define and use custom data types in Lumen.

## Records

Records are typed structures with named fields.

### Basic Records

```lumen
record Point
  x: Float
  y: Float
end

cell origin() -> Point
  return Point(x: 0.0, y: 0.0)
end

cell move(p: Point, dx: Float, dy: Float) -> Point
  return Point(x: p.x + dx, y: p.y + dy)
end
```

### Field Constraints

Add validation with `where` clauses:

```lumen
record Product
  name: String where length(name) > 0
  price: Float where price >= 0.0
  quantity: Int where quantity >= 0
end

cell total(product: Product) -> Float
  return product.price * product.quantity
end
```

Constraints are validated when records are constructed.

### Default Values

```lumen
record Config
  host: String = "localhost"
  port: Int = 8080
  debug: Bool = false
end

cell default_config() -> Config
  return Config()  # Uses all defaults
end

cell custom_config() -> Config
  return Config(host: "api.example.com", port: 443)
end
```

### Nested Records

```lumen
record Address
  street: String
  city: String
  country: String
end

record Person
  name: String
  address: Address
end

cell example() -> Person
  return Person(
    name: "Alice",
    address: Address(
      street: "123 Main St",
      city: "Boston",
      country: "USA"
    )
  )
end
```

## Enums

Enums define a set of possible values.

### Simple Enums

```lumen
enum Status
  Pending
  Active
  Completed
  Cancelled
end

cell can_modify(status: Status) -> Bool
  match status
    Pending -> return true
    Active -> return true
    Completed -> return false
    Cancelled -> return false
  end
end
```

### Enums with Data

```lumen
enum Result
  Ok(value: Int)
  Err(message: String)
end

cell divide(a: Int, b: Int) -> Result
  if b == 0
    return Err("Division by zero")
  end
  return Ok(a / b)
end

cell safe_divide(a: Int, b: Int) -> String
  match divide(a, b)
    Ok(value) -> return "Result: {value}"
    Err(msg) -> return "Error: {msg}"
  end
end
```

### Complex Enum Example

```lumen
enum Shape
  Circle(radius: Float)
  Rectangle(width: Float, height: Float)
  Triangle(base: Float, height: Float)
end

cell area(shape: Shape) -> Float
  match shape
    Circle(r) -> return 3.14159 * r * r
    Rectangle(w, h) -> return w * h
    Triangle(b, h) -> return 0.5 * b * h
  end
end
```

## Union Types

Combine types with `|`:

```lumen
cell process(value: Int | String) -> String
  match value
    n: Int -> return "Number: {n}"
    s: String -> return "Text: {s}"
  end
end

cell maybe_int() -> Int | Null
  return null  # or return some Int
end
```

## Type Aliases

Create shorthand names for complex types:

```lumen
type UserId = String
type Coordinates = tuple[Float, Float]
type StringMap = map[String, String]

cell get_user(id: UserId) -> String
  return "User {id}"
end
```

## Generic Types

Define types that work with any type:

```lumen
record Box[T]
  value: T
end

cell box_int() -> Box[Int]
  return Box(value: 42)
end

cell box_string() -> Box[String]
  return Box(value: "hello")
end
```

## Lists of Records

```lumen
record Item
  id: Int
  name: String
  price: Float
end

cell total_price(items: list[Item]) -> Float
  let mut sum = 0.0
  for item in items
    sum += item.price
  end
  return sum
end

cell shopping_cart() -> Float
  let items = [
    Item(id: 1, name: "Apple", price: 1.50),
    Item(id: 2, name: "Banana", price: 0.75),
    Item(id: 3, name: "Orange", price: 2.00)
  ]
  return total_price(items)
end
```

## Maps with Custom Types

```lumen
record Student
  name: String
  grade: Int
end

cell top_students() -> map[String, Student]
  return {
    "alice": Student(name: "Alice", grade: 95),
    "bob": Student(name: "Bob", grade: 88),
    "charlie": Student(name: "Charlie", grade: 92)
  }
end
```

## Practice Exercise

Create a simple bank system:

```lumen
record Account
  id: String
  owner: String
  balance: Float where balance >= 0.0
end

cell create_account(id: String, owner: String) -> Account
  return Account(id: id, owner: owner, balance: 0.0)
end

cell deposit(account: Account, amount: Float) -> Account
  return Account(
    id: account.id,
    owner: account.owner,
    balance: account.balance + amount
  )
end

cell withdraw(account: Account, amount: Float) -> result[Account, String]
  if amount > account.balance
    return err("Insufficient funds")
  end
  return ok(Account(
    id: account.id,
    owner: account.owner,
    balance: account.balance - amount
  ))
end

cell main() -> String
  let mut acc = create_account("001", "Alice")
  acc = deposit(acc, 100.0)
  
  match withdraw(acc, 50.0)
    ok(updated) -> return "Balance: {updated.balance}"
    err(msg) -> return "Error: {msg}"
  end
end
```

## Next Steps

- [Functions](/learn/tutorial/functions) — Define reusable code
- [Pattern Matching](/learn/tutorial/pattern-matching) — Destructure data
