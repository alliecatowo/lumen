use lumen_provider_mcp::*;
use lumen_rt::services::tools::ToolProvider;
use serde_json::json;
use std::path::PathBuf;

#[test]
#[ignore] // Run with: cargo test -p lumen-provider-mcp --test integration_tests -- --ignored
fn test_stdio_transport_with_real_subprocess() {
    // Find the test server script
    let mut test_server_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_server_path.push("tests/test_mcp_server.py");

    assert!(
        test_server_path.exists(),
        "Test server script not found at {:?}",
        test_server_path
    );

    // Create transport
    let transport = StdioTransport::new("python3", &[test_server_path.to_str().unwrap()]);

    // Test tools/list
    let result = transport.send_request("tools/list", json!({}));
    assert!(result.is_ok(), "tools/list failed: {:?}", result.err());

    let tools_response = result.unwrap();
    let tools = tools_response
        .get("tools")
        .and_then(|t| t.as_array())
        .expect("Response should have 'tools' array");

    assert_eq!(tools.len(), 2, "Should have 2 tools");

    let tool_names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(tool_names.contains(&"echo"), "Should have 'echo' tool");
    assert!(tool_names.contains(&"greet"), "Should have 'greet' tool");

    println!("✓ tools/list succeeded with {} tools", tools.len());
}

#[test]
#[ignore]
fn test_stdio_transport_tool_call() {
    let mut test_server_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_server_path.push("tests/test_mcp_server.py");

    let transport = StdioTransport::new("python3", &[test_server_path.to_str().unwrap()]);

    // Call the echo tool
    let params = json!({
        "name": "echo",
        "arguments": {"message": "hello world"}
    });

    let result = transport.send_request("tools/call", params);
    assert!(result.is_ok(), "tools/call failed: {:?}", result.err());

    let response = result.unwrap();
    assert!(
        response.get("echoed").is_some(),
        "Response should have 'echoed' field: {:?}",
        response
    );

    println!("✓ tools/call echo succeeded: {:?}", response);

    // Call the greet tool
    let params = json!({
        "name": "greet",
        "arguments": {"name": "Lumen"}
    });

    let result = transport.send_request("tools/call", params);
    assert!(
        result.is_ok(),
        "tools/call greet failed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    let greeting = response
        .get("greeting")
        .and_then(|g| g.as_str())
        .expect("Response should have 'greeting' string");

    assert!(
        greeting.contains("Lumen"),
        "Greeting should mention 'Lumen'"
    );
    println!("✓ tools/call greet succeeded: {}", greeting);
}

#[test]
#[ignore]
fn test_stdio_transport_error_handling() {
    let mut test_server_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_server_path.push("tests/test_mcp_server.py");

    let transport = StdioTransport::new("python3", &[test_server_path.to_str().unwrap()]);

    // Call unknown tool
    let params = json!({
        "name": "unknown_tool",
        "arguments": {}
    });

    let result = transport.send_request("tools/call", params);
    assert!(result.is_err(), "Should fail for unknown tool");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("error") || err_msg.contains("Unknown tool"),
        "Error should mention the problem: {}",
        err_msg
    );
    println!("✓ Error handling works: {}", err_msg);
}

#[test]
#[ignore]
fn test_stdio_transport_nonexistent_command() {
    let transport = StdioTransport::new("nonexistent_command_xyz_12345", &[]);
    let result = transport.send_request("test", json!({}));

    assert!(result.is_err(), "Should fail for nonexistent command");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("Failed to spawn"),
        "Error should mention spawn failure: {}",
        err_msg
    );
    println!("✓ Nonexistent command handling works: {}", err_msg);
}

#[test]
#[ignore]
fn test_mcp_tool_discovery() {
    let mut test_server_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_server_path.push("tests/test_mcp_server.py");

    let transport = std::sync::Arc::new(StdioTransport::new(
        "python3",
        &[test_server_path.to_str().unwrap()],
    ));

    let providers = discover_tools("test_server", transport);
    assert!(
        providers.is_ok(),
        "Tool discovery failed: {:?}",
        providers.err()
    );

    let providers = providers.unwrap();
    assert_eq!(providers.len(), 2, "Should discover 2 providers");

    // Verify provider metadata
    let provider = &providers[0];
    assert_eq!(provider.name(), "test_server");
    assert_eq!(provider.version(), "0.1.0");

    let qualified_names: Vec<String> = providers.iter().map(|p| p.qualified_name()).collect();

    assert!(
        qualified_names.contains(&"test_server.echo".to_string()),
        "Should have test_server.echo"
    );
    assert!(
        qualified_names.contains(&"test_server.greet".to_string()),
        "Should have test_server.greet"
    );

    println!("✓ Tool discovery succeeded with {} tools", providers.len());
    for (i, p) in providers.iter().enumerate() {
        println!("  {}. {}", i + 1, p.qualified_name());
    }
}

#[test]
#[ignore]
fn test_mcp_provider_call() {
    let mut test_server_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_server_path.push("tests/test_mcp_server.py");

    let transport = std::sync::Arc::new(StdioTransport::new(
        "python3",
        &[test_server_path.to_str().unwrap()],
    ));

    let providers = discover_tools("test_server", transport).expect("Discovery should work");

    // Find the greet provider
    let greet_provider = providers
        .iter()
        .find(|p| p.qualified_name() == "test_server.greet")
        .expect("Should find greet provider");

    // Call it
    let result = greet_provider.call(json!({"name": "Integration Test"}));
    assert!(result.is_ok(), "Provider call failed: {:?}", result.err());

    let response = result.unwrap();
    let greeting = response
        .get("greeting")
        .and_then(|g| g.as_str())
        .expect("Response should have greeting");

    assert!(
        greeting.contains("Integration Test"),
        "Greeting should mention the name"
    );
    println!("✓ Provider call succeeded: {}", greeting);
}

#[test]
#[ignore]
fn test_mcp_provider_effects() {
    let mut test_server_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_server_path.push("tests/test_mcp_server.py");

    let transport = std::sync::Arc::new(StdioTransport::new(
        "python3",
        &[test_server_path.to_str().unwrap()],
    ));

    let providers = discover_tools("test_server", transport).expect("Discovery should work");

    for provider in &providers {
        let effects = provider.effects();
        assert_eq!(
            effects,
            vec!["mcp"],
            "All MCP providers should have 'mcp' effect"
        );
        let schema = provider.schema();
        assert_eq!(
            schema.effects,
            vec!["mcp"],
            "Schema should match provider effects"
        );
    }

    println!("✓ All {} providers have correct effects", providers.len());
}
