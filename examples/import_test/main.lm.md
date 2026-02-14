# Main Application

This demonstrates importing from another module.

```lumen
import models: User, create_user, greet_user
```

```lumen
cell main() -> String
  let user = create_user("Alice", 30)
  return greet_user(user)
end
```

```lumen
cell test_user_creation() -> Bool
  let user = create_user("Bob", 25)
  return user.name == "Bob" and user.age == 25
end
```
