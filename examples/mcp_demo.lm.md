# MCP Integration Demo
Shows how Lumen connects to MCP tool servers.

```lumen
use tool "mcp.weather.get_forecast" as GetForecast

cell main() -> String
  let forecast = GetForecast({
    location: "San Francisco",
    days: 3
  })
  return forecast
end
```
