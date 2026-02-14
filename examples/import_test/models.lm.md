# Models Module

This module defines data types used by the main application.

```lumen
record User
  name: String
  age: Int
end
```

```lumen
cell create_user(name: String, age: Int) -> User
  return User(name: name, age: age)
end
```

```lumen
cell greet_user(user: User) -> String
  return "Hello, " + user.name + "!"
end
```
