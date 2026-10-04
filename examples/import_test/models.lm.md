# Models Module

This module defines data types used by the main application.

```lumen
pub record User
  name: String
  age: Int
end
```

```lumen
pub cell create_user(name: String, age: Int) -> User
  return User(name: name, age: age)
end
```

```lumen
pub cell greet_user(user: User) -> String
  return "Hello, " + user.name + "!"
end
```
