use lumen_runtime::tools::*;
use serde_json::json;

// Test provider registry with real providers from other crates
#[test]
#[ignore] // Run with: cargo test -p lumen-runtime --test integration_tests -- --ignored
fn test_provider_registry_with_crypto_providers() {
    use lumen_provider_crypto::CryptoProvider;

    let mut registry = ProviderRegistry::new();

    // Register multiple crypto tools
    registry.register("crypto.sha256", Box::new(CryptoProvider::sha256()));
    registry.register("crypto.uuid", Box::new(CryptoProvider::uuid()));
    registry.register(
        "crypto.base64_encode",
        Box::new(CryptoProvider::base64_encode()),
    );

    assert_eq!(registry.len(), 3);
    assert!(registry.has("crypto.sha256"));
    assert!(registry.has("crypto.uuid"));
    assert!(registry.has("crypto.base64_encode"));

    // Test SHA256 via dispatch
    let request = ToolRequest {
        tool_id: "crypto.sha256".to_string(),
        version: "1.0.0".to_string(),
        args: json!({"input": "hello"}),
        policy: json!({}),
    };

    let response = registry.dispatch(&request).expect("SHA256 should work");
    let hash = response.outputs.as_str().expect("Should be a string");
    assert_eq!(
        hash,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    println!("✓ SHA256 hash: {}", hash);

    // Test UUID generation
    let request = ToolRequest {
        tool_id: "crypto.uuid".to_string(),
        version: "1.0.0".to_string(),
        args: json!({}),
        policy: json!({}),
    };

    let response = registry.dispatch(&request).expect("UUID should work");
    let uuid = response.outputs.as_str().expect("Should be a string");
    assert_eq!(uuid.len(), 36, "UUID should be 36 chars");
    println!("✓ UUID generated: {}", uuid);

    // Test Base64 encoding
    let request = ToolRequest {
        tool_id: "crypto.base64_encode".to_string(),
        version: "1.0.0".to_string(),
        args: json!({"input": "test"}),
        policy: json!({}),
    };

    let response = registry.dispatch(&request).expect("Base64 should work");
    let encoded = response.outputs.as_str().expect("Should be a string");
    assert_eq!(encoded, "dGVzdA==");
    println!("✓ Base64 encoded: {}", encoded);
}

#[test]
#[ignore]
fn test_provider_registry_with_env_providers() {
    use lumen_provider_env::EnvProvider;

    let mut registry = ProviderRegistry::new();

    // Register env tools
    registry.register("env.cwd", Box::new(EnvProvider::cwd()));
    registry.register("env.platform", Box::new(EnvProvider::platform()));

    // Test CWD
    let request = ToolRequest {
        tool_id: "env.cwd".to_string(),
        version: "1.0.0".to_string(),
        args: json!({}),
        policy: json!({}),
    };

    let response = registry.dispatch(&request).expect("CWD should work");
    let cwd = response.outputs.as_str().expect("Should be a string");
    assert!(!cwd.is_empty(), "CWD should not be empty");
    println!("✓ Current directory: {}", cwd);

    // Test platform
    let request = ToolRequest {
        tool_id: "env.platform".to_string(),
        version: "1.0.0".to_string(),
        args: json!({}),
        policy: json!({}),
    };

    let response = registry.dispatch(&request).expect("Platform should work");
    let platform = response.outputs.as_str().expect("Should be a string");
    assert!(["linux", "macos", "windows"].contains(&platform));
    println!("✓ Platform: {}", platform);
}

#[test]
#[ignore]
fn test_provider_registry_with_fs_providers() {
    use lumen_provider_fs::FsProvider;
    use std::fs;

    let mut registry = ProviderRegistry::new();

    // Register fs tools
    registry.register("fs.write", Box::new(FsProvider::write()));
    registry.register("fs.read", Box::new(FsProvider::read()));

    // Write a test file
    let test_file = "/tmp/lumen_integration_test.txt";
    let request = ToolRequest {
        tool_id: "fs.write".to_string(),
        version: "1.0.0".to_string(),
        args: json!({
            "path": test_file,
            "content": "Integration test content"
        }),
        policy: json!({}),
    };

    let response = registry.dispatch(&request).expect("Write should work");
    assert!(response.outputs.as_bool().unwrap_or(false));
    println!("✓ File written to {}", test_file);

    // Read it back
    let request = ToolRequest {
        tool_id: "fs.read".to_string(),
        version: "1.0.0".to_string(),
        args: json!({
            "path": test_file
        }),
        policy: json!({}),
    };

    let response = registry.dispatch(&request).expect("Read should work");
    let content = response.outputs.as_str().expect("Should be a string");
    assert_eq!(content, "Integration test content");
    println!("✓ File read back: {}", content);

    // Cleanup
    fs::remove_file(test_file).ok();
}

#[test]
#[ignore]
fn test_mixed_provider_dispatch() {
    use lumen_provider_crypto::CryptoProvider;
    use lumen_provider_env::EnvProvider;

    let mut registry = ProviderRegistry::new();

    // Register tools from different providers
    registry.register("crypto.sha256", Box::new(CryptoProvider::sha256()));
    registry.register("crypto.md5", Box::new(CryptoProvider::md5()));
    registry.register("env.platform", Box::new(EnvProvider::platform()));
    registry.register("env.cwd", Box::new(EnvProvider::cwd()));

    assert_eq!(registry.len(), 4);

    let tools = registry.list();
    assert_eq!(tools.len(), 4);
    assert!(tools.contains(&"crypto.sha256"));
    assert!(tools.contains(&"crypto.md5"));
    assert!(tools.contains(&"env.platform"));
    assert!(tools.contains(&"env.cwd"));

    // Dispatch to each tool
    let test_cases = vec![
        ("crypto.sha256", json!({"input": "test"})),
        ("crypto.md5", json!({"input": "test"})),
        ("env.platform", json!({})),
        ("env.cwd", json!({})),
    ];

    for (tool_id, args) in test_cases {
        let request = ToolRequest {
            tool_id: tool_id.to_string(),
            version: "1.0.0".to_string(),
            args,
            policy: json!({}),
        };

        let response = registry
            .dispatch(&request)
            .unwrap_or_else(|_| panic!("{tool_id} should work"));
        assert!(response.latency_ms < 1000, "Latency should be reasonable");
        println!(
            "✓ {} dispatched successfully ({}ms)",
            tool_id, response.latency_ms
        );
    }
}

#[test]
#[ignore]
fn test_registry_error_handling() {
    use lumen_provider_crypto::CryptoProvider;

    let mut registry = ProviderRegistry::new();
    registry.register("crypto.sha256", Box::new(CryptoProvider::sha256()));

    // Test missing tool
    let request = ToolRequest {
        tool_id: "nonexistent.tool".to_string(),
        version: "1.0.0".to_string(),
        args: json!({}),
        policy: json!({}),
    };

    let result = registry.dispatch(&request);
    assert!(result.is_err(), "Should fail for missing tool");
    match result.unwrap_err() {
        ToolError::NotRegistered(name) => {
            assert_eq!(name, "nonexistent.tool");
            println!("✓ Missing tool error: NotRegistered({})", name);
        }
        other => panic!("Expected NotRegistered, got: {:?}", other),
    }

    // Test invalid input
    let request = ToolRequest {
        tool_id: "crypto.sha256".to_string(),
        version: "1.0.0".to_string(),
        args: json!({"wrong_field": "value"}), // missing "input" field
        policy: json!({}),
    };

    let result = registry.dispatch(&request);
    assert!(result.is_err(), "Should fail for invalid input");
    match result.unwrap_err() {
        ToolError::InvocationFailed(msg) => {
            assert!(msg.contains("Invalid input") || msg.contains("missing"));
            println!("✓ Invalid input error: {}", msg);
        }
        other => panic!("Expected InvocationFailed, got: {:?}", other),
    }
}

#[test]
#[ignore]
fn test_provider_effects_metadata() {
    use lumen_provider_crypto::CryptoProvider;
    use lumen_provider_env::EnvProvider;

    let mut registry = ProviderRegistry::new();
    registry.register("crypto.uuid", Box::new(CryptoProvider::uuid()));
    registry.register("env.platform", Box::new(EnvProvider::platform()));

    // Verify effects metadata
    let crypto_provider = registry
        .get("crypto.uuid")
        .expect("Should find crypto.uuid");
    assert_eq!(crypto_provider.effects(), vec!["crypto"]);

    let env_provider = registry
        .get("env.platform")
        .expect("Should find env.platform");
    assert_eq!(env_provider.effects(), vec!["env"]);

    println!("✓ Crypto provider effects: {:?}", crypto_provider.effects());
    println!("✓ Env provider effects: {:?}", env_provider.effects());
}

#[test]
#[ignore]
fn test_latency_measurement() {
    use lumen_provider_crypto::CryptoProvider;

    let mut registry = ProviderRegistry::new();
    registry.register("crypto.sha256", Box::new(CryptoProvider::sha256()));

    let request = ToolRequest {
        tool_id: "crypto.sha256".to_string(),
        version: "1.0.0".to_string(),
        args: json!({"input": "test"}),
        policy: json!({}),
    };

    // Run multiple times to check latency consistency
    for i in 0..5 {
        let response = registry.dispatch(&request).expect("Should work");
        assert!(
            response.latency_ms < 100,
            "Should be very fast for local crypto"
        );
        println!("  Run {}: {}ms", i + 1, response.latency_ms);
    }

    println!("✓ Latency measurement works");
}
