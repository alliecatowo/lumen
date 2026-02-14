# Integration Test - Environment Tools

```lumen
use tool env.cwd as Cwd
use tool env.platform as Platform

grant Cwd
grant Platform

bind effect env to Cwd
bind effect env to Platform

cell test_cwd() -> String / {env}
  return Cwd()
end

cell test_platform() -> String / {env}
  return Platform()
end

cell main() -> String / {env}
  let dir = test_cwd()
  let os = test_platform()
  return os ++ " @ " ++ dir
end
```
