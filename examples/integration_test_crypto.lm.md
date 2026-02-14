# Integration Test - Crypto Tools

```lumen
use tool crypto.sha256 as Sha256
use tool crypto.uuid as Uuid

grant Sha256
grant Uuid

bind effect crypto to Sha256
bind effect crypto to Uuid

cell test_sha256() -> String / {crypto, external}
  return Sha256(input: "hello")
end

cell test_uuid() -> String / {crypto, external}
  return Uuid()
end

cell main() -> String / {crypto, external}
  let hash = test_sha256()
  let id = test_uuid()
  return hash ++ " | " ++ id
end
```
