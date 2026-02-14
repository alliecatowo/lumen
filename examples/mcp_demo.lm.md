# MCP Integration Demo
Shows how Lumen connects to MCP tool servers.

```lumen
use tool weather.get_forecast as GetForecast
grant GetForecast timeout_ms 30000
bind effect external to GetForecast

cell main() -> String / {external}
  let forecast = GetForecast(
    location: "San Francisco",
    days: 3
  )
  return forecast
end
```
