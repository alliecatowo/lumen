#!/usr/bin/env python3
"""
Simple MCP test server for integration testing.
Responds to JSON-RPC 2.0 requests via stdio.
"""
import sys
import json

def main():
    for line in sys.stdin:
        try:
            req = json.loads(line.strip())
            method = req.get("method", "")
            req_id = req.get("id", 1)

            if method == "tools/list":
                # Return a list of available tools
                response = {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "tools": [
                            {
                                "name": "echo",
                                "description": "Echo back the input",
                                "input_schema": {"type": "object"}
                            },
                            {
                                "name": "greet",
                                "description": "Greet someone",
                                "input_schema": {
                                    "type": "object",
                                    "required": ["name"],
                                    "properties": {
                                        "name": {"type": "string"}
                                    }
                                }
                            }
                        ]
                    }
                }
            elif method == "tools/call":
                # Handle tool calls
                params = req.get("params", {})
                tool_name = params.get("name", "")
                arguments = params.get("arguments", {})

                if tool_name == "echo":
                    result = {"echoed": arguments}
                elif tool_name == "greet":
                    name = arguments.get("name", "stranger")
                    result = {"greeting": f"Hello, {name}!"}
                else:
                    response = {
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "error": {
                            "code": -32601,
                            "message": f"Unknown tool: {tool_name}"
                        }
                    }
                    print(json.dumps(response), flush=True)
                    continue

                response = {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": result
                }
            else:
                # Unknown method
                response = {
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": {
                        "code": -32601,
                        "message": f"Method not found: {method}"
                    }
                }

            print(json.dumps(response), flush=True)

        except Exception as e:
            # Send error response
            error_response = {
                "jsonrpc": "2.0",
                "id": 1,
                "error": {
                    "code": -32603,
                    "message": f"Internal error: {str(e)}"
                }
            }
            print(json.dumps(error_response), flush=True)

if __name__ == "__main__":
    main()
