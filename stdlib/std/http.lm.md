# Standard Library: HTTP

HTTP client utilities for making web requests.

```lumen
use tool "http_get"
use tool "http_post"
use tool "http_put"
use tool "http_delete"

effect http

grant http_access
  use tool "http_get"
  use tool "http_post"
  use tool "http_put"
  use tool "http_delete"
  policy
    timeout_ms: 30000
    domain: ".*"
  end
end

bind effect http to "http_get"
bind effect http to "http_post"
bind effect http to "http_put"
bind effect http to "http_delete"

# Request record type
record HttpRequest
  url: string
  headers: map[string, string]
  body: string
end

# Response record type
record HttpResponse
  status: int
  headers: map[string, string]
  body: string
end

# Make a GET request
cell get(url: string) -> HttpResponse / {http}
  let result = http_get({
    url: url,
    headers: {}
  })
  return HttpResponse{
    status: result["status"],
    headers: result["headers"],
    body: result["body"]
  }
end

# Make a GET request with headers
cell get_with_headers(url: string, headers: map[string, string]) -> HttpResponse / {http}
  let result = http_get({
    url: url,
    headers: headers
  })
  return HttpResponse{
    status: result["status"],
    headers: result["headers"],
    body: result["body"]
  }
end

# Make a POST request
cell post(url: string, body: string) -> HttpResponse / {http}
  let result = http_post({
    url: url,
    headers: {"Content-Type": "application/json"},
    body: body
  })
  return HttpResponse{
    status: result["status"],
    headers: result["headers"],
    body: result["body"]
  }
end

# Make a POST request with custom headers
cell post_with_headers(url: string, body: string, headers: map[string, string]) -> HttpResponse / {http}
  let result = http_post({
    url: url,
    headers: headers,
    body: body
  })
  return HttpResponse{
    status: result["status"],
    headers: result["headers"],
    body: result["body"]
  }
end

# Make a PUT request
cell put(url: string, body: string) -> HttpResponse / {http}
  let result = http_put({
    url: url,
    headers: {"Content-Type": "application/json"},
    body: body
  })
  return HttpResponse{
    status: result["status"],
    headers: result["headers"],
    body: result["body"]
  }
end

# Make a PUT request with custom headers
cell put_with_headers(url: string, body: string, headers: map[string, string]) -> HttpResponse / {http}
  let result = http_put({
    url: url,
    headers: headers,
    body: body
  })
  return HttpResponse{
    status: result["status"],
    headers: result["headers"],
    body: result["body"]
  }
end

# Make a DELETE request
cell delete(url: string) -> HttpResponse / {http}
  let result = http_delete({
    url: url,
    headers: {}
  })
  return HttpResponse{
    status: result["status"],
    headers: result["headers"],
    body: result["body"]
  }
end

# Make a DELETE request with headers
cell delete_with_headers(url: string, headers: map[string, string]) -> HttpResponse / {http}
  let result = http_delete({
    url: url,
    headers: headers
  })
  return HttpResponse{
    status: result["status"],
    headers: result["headers"],
    body: result["body"]
  }
end

# Check if response status indicates success (2xx)
cell is_success(response: HttpResponse) -> bool
  return response.status >= 200 and response.status < 300
end

# Check if response status indicates redirect (3xx)
cell is_redirect(response: HttpResponse) -> bool
  return response.status >= 300 and response.status < 400
end

# Check if response status indicates client error (4xx)
cell is_client_error(response: HttpResponse) -> bool
  return response.status >= 400 and response.status < 500
end

# Check if response status indicates server error (5xx)
cell is_server_error(response: HttpResponse) -> bool
  return response.status >= 500 and response.status < 600
end

# Get a header value from response (case-insensitive key lookup)
cell get_header(response: HttpResponse, key: string) -> string
  let lower_key = lower(key)
  let header_keys = keys(response.headers)

  for header_key in header_keys
    if lower(header_key) == lower_key
      return response.headers[header_key]
    end
  end

  return ""
end
```
