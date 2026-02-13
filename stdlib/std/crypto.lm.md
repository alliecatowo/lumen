# Standard Library: Crypto

Cryptographic and encoding utilities.

```lumen
use tool "sha256"
use tool "uuid"
use tool "random_int"
use tool "base64_encode"
use tool "base64_decode"

effect crypto
effect random

grant crypto_access
  use tool "sha256"
  use tool "base64_encode"
  use tool "base64_decode"
  policy
    timeout_ms: 5000
  end
end

grant random_access
  use tool "uuid"
  use tool "random_int"
  policy
    timeout_ms: 5000
  end
end

bind effect crypto to "sha256"
bind effect crypto to "base64_encode"
bind effect crypto to "base64_decode"
bind effect random to "uuid"
bind effect random to "random_int"

# Generate SHA-256 hash of a string
cell sha256_hash(input: string) -> string / {crypto}
  let result = sha256({data: input})
  return result
end

# Generate a UUID v4
cell uuid_v4() -> string / {random}
  let result = uuid({})
  return result
end

# Generate a random integer in range [min, max)
cell random_int_range(min_val: int, max_val: int) -> int / {random}
  let result = random_int({
    min: min_val,
    max: max_val
  })
  return result
end

# Base64 encode a string
cell base64_enc(input: string) -> string / {crypto}
  let result = base64_encode({data: input})
  return result
end

# Base64 decode a string
cell base64_dec(input: string) -> string / {crypto}
  let result = base64_decode({data: input})
  return result
end

# Generate a random hex string of specified length
cell random_hex(length: int) -> string / {random}
  let hex_chars = "0123456789abcdef"
  let result = ""
  let i = 0
  while i < length
    let idx = random_int_range(0, 16)
    let char = slice(hex_chars, idx, idx + 1)
    result = result + char
    i = i + 1
  end
  return result
end

# Generate a random alphanumeric string
cell random_string(length: int) -> string / {random}
  let chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
  let chars_len = 62
  let result = ""
  let i = 0
  while i < length
    let idx = random_int_range(0, chars_len)
    let char = slice(chars, idx, idx + 1)
    result = result + char
    i = i + 1
  end
  return result
end

# Hash a password (simple SHA-256 based - not suitable for production)
cell hash_password(password: string, salt: string) -> string / {crypto}
  let combined = password + salt
  return sha256_hash(combined)
end

# Verify a password against a hash
cell verify_password(password: string, salt: string, hash_val: string) -> bool / {crypto}
  let computed_hash = hash_password(password, salt)
  return computed_hash == hash_val
end

# Generate a random salt for password hashing
cell generate_salt(length: int) -> string / {random}
  return random_hex(length)
end
```
