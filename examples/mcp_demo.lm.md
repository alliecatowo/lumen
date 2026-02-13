# MCP Integration Demo
Shows how Lumen connects to MCP tool servers.

```lumen
use tool mcp.weather.get_forecast

cell main() -> String
  let forecast = mcp.weather.get_forecast({
    location: "San Francisco",
    days: 3
  })
  return forecast
end
```
