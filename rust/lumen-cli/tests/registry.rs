//! Tests for the deployment registry infrastructure (T092).

use lumen_cli::registry::*;
use std::collections::BTreeMap;

// =============================================================================
// VersionComparator — parsing
// =============================================================================

#[test]
fn test_version_comparator_parse_exact() {
    let c = VersionComparator::parse("=1.2.3").unwrap();
    assert_eq!(
        c,
        VersionComparator::Exact {
            major: 1,
            minor: 2,
            patch: 3
        }
    );
}

#[test]
fn test_version_comparator_parse_bare_is_exact() {
    let c = VersionComparator::parse("1.2.3").unwrap();
    assert_eq!(
        c,
        VersionComparator::Exact {
            major: 1,
            minor: 2,
            patch: 3
        }
    );
}

#[test]
fn test_version_comparator_parse_caret() {
    let c = VersionComparator::parse("^1.2.3").unwrap();
    assert_eq!(
        c,
        VersionComparator::Caret {
            major: 1,
            minor: 2,
            patch: 3
        }
    );
}

#[test]
fn test_version_comparator_parse_tilde() {
    let c = VersionComparator::parse("~1.2.3").unwrap();
    assert_eq!(
        c,
        VersionComparator::Tilde {
            major: 1,
            minor: 2,
            patch: 3
        }
    );
}

#[test]
fn test_version_comparator_parse_gte() {
    let c = VersionComparator::parse(">=2.0.0").unwrap();
    assert_eq!(
        c,
        VersionComparator::Gte {
            major: 2,
            minor: 0,
            patch: 0
        }
    );
}

#[test]
fn test_version_comparator_parse_lt() {
    let c = VersionComparator::parse("<3.0.0").unwrap();
    assert_eq!(
        c,
        VersionComparator::Lt {
            major: 3,
            minor: 0,
            patch: 0
        }
    );
}

#[test]
fn test_version_comparator_parse_any() {
    let c = VersionComparator::parse("*").unwrap();
    assert_eq!(c, VersionComparator::Any);
}

#[test]
fn test_version_comparator_parse_invalid() {
    assert!(VersionComparator::parse("not-a-version").is_err());
    assert!(VersionComparator::parse("1.2").is_err());
}

// =============================================================================
// VersionComparator — matching
// =============================================================================

#[test]
fn test_comparator_exact_matches() {
    let c = VersionComparator::Exact {
        major: 1,
        minor: 2,
        patch: 3,
    };
    assert!(c.matches(1, 2, 3));
    assert!(!c.matches(1, 2, 4));
    assert!(!c.matches(2, 0, 0));
}

#[test]
fn test_comparator_caret_major_nonzero() {
    let c = VersionComparator::Caret {
        major: 1,
        minor: 2,
        patch: 3,
    };
    assert!(c.matches(1, 2, 3));
    assert!(c.matches(1, 2, 4));
    assert!(c.matches(1, 3, 0));
    assert!(c.matches(1, 99, 0));
    assert!(!c.matches(2, 0, 0));
    assert!(!c.matches(1, 2, 2));
    assert!(!c.matches(0, 9, 0));
}

#[test]
fn test_comparator_caret_zero_major() {
    // ^0.2.3 matches 0.2.x where x >= 3
    let c = VersionComparator::Caret {
        major: 0,
        minor: 2,
        patch: 3,
    };
    assert!(c.matches(0, 2, 3));
    assert!(c.matches(0, 2, 9));
    assert!(!c.matches(0, 3, 0));
    assert!(!c.matches(0, 2, 2));
    assert!(!c.matches(1, 0, 0));
}

#[test]
fn test_comparator_caret_zero_zero() {
    // ^0.0.3 matches only 0.0.3
    let c = VersionComparator::Caret {
        major: 0,
        minor: 0,
        patch: 3,
    };
    assert!(c.matches(0, 0, 3));
    assert!(!c.matches(0, 0, 4));
    assert!(!c.matches(0, 1, 0));
}

#[test]
fn test_comparator_tilde_matches() {
    let c = VersionComparator::Tilde {
        major: 1,
        minor: 2,
        patch: 3,
    };
    assert!(c.matches(1, 2, 3));
    assert!(c.matches(1, 2, 99));
    assert!(!c.matches(1, 3, 0));
    assert!(!c.matches(1, 2, 2));
}

#[test]
fn test_comparator_gte_matches() {
    let c = VersionComparator::Gte {
        major: 1,
        minor: 5,
        patch: 0,
    };
    assert!(c.matches(1, 5, 0));
    assert!(c.matches(1, 5, 1));
    assert!(c.matches(1, 6, 0));
    assert!(c.matches(2, 0, 0));
    assert!(!c.matches(1, 4, 9));
    assert!(!c.matches(0, 9, 0));
}

#[test]
fn test_comparator_lt_matches() {
    let c = VersionComparator::Lt {
        major: 2,
        minor: 0,
        patch: 0,
    };
    assert!(c.matches(1, 9, 9));
    assert!(c.matches(0, 0, 1));
    assert!(!c.matches(2, 0, 0));
    assert!(!c.matches(2, 0, 1));
    assert!(!c.matches(3, 0, 0));
}

#[test]
fn test_comparator_any_matches_everything() {
    let c = VersionComparator::Any;
    assert!(c.matches(0, 0, 0));
    assert!(c.matches(999, 999, 999));
}

#[test]
fn test_comparator_matches_str() {
    let c = VersionComparator::Caret {
        major: 1,
        minor: 0,
        patch: 0,
    };
    assert!(c.matches_str("1.2.3"));
    assert!(!c.matches_str("2.0.0"));
    assert!(!c.matches_str("not-a-version"));
}

#[test]
fn test_comparator_display() {
    assert_eq!(
        VersionComparator::Exact {
            major: 1,
            minor: 2,
            patch: 3
        }
        .to_string(),
        "=1.2.3"
    );
    assert_eq!(
        VersionComparator::Caret {
            major: 1,
            minor: 0,
            patch: 0
        }
        .to_string(),
        "^1.0.0"
    );
    assert_eq!(VersionComparator::Any.to_string(), "*");
}

// =============================================================================
// VersionReq
// =============================================================================

#[test]
fn test_version_req_parse_single() {
    let req = VersionReq::parse("^1.2.3").unwrap();
    assert_eq!(req.comparators.len(), 1);
    assert!(req.matches(1, 3, 0));
    assert!(!req.matches(2, 0, 0));
}

#[test]
fn test_version_req_parse_range() {
    let req = VersionReq::parse(">=1.0.0 <2.0.0").unwrap();
    assert_eq!(req.comparators.len(), 2);
    assert!(req.matches(1, 0, 0));
    assert!(req.matches(1, 9, 9));
    assert!(!req.matches(2, 0, 0));
    assert!(!req.matches(0, 9, 9));
}

#[test]
fn test_version_req_any() {
    let req = VersionReq::any();
    assert!(req.matches(0, 0, 0));
    assert!(req.matches(100, 200, 300));
}

#[test]
fn test_version_req_matches_str() {
    let req = VersionReq::parse("~1.2.0").unwrap();
    assert!(req.matches_str("1.2.5"));
    assert!(!req.matches_str("1.3.0"));
    assert!(!req.matches_str("garbage"));
}

#[test]
fn test_version_req_parse_empty() {
    assert!(VersionReq::parse("").is_err());
}

#[test]
fn test_version_req_display() {
    let req = VersionReq::parse(">=1.0.0 <2.0.0").unwrap();
    let s = req.to_string();
    assert!(s.contains(">=1.0.0"));
    assert!(s.contains("<2.0.0"));
}

// =============================================================================
// RequestMethod and RegistryRequest
// =============================================================================

#[test]
fn test_request_method_display() {
    assert_eq!(RequestMethod::Get.to_string(), "GET");
    assert_eq!(RequestMethod::Put.to_string(), "PUT");
    assert_eq!(RequestMethod::Post.to_string(), "POST");
    assert_eq!(RequestMethod::Delete.to_string(), "DELETE");
    assert_eq!(RequestMethod::Head.to_string(), "HEAD");
}

#[test]
fn test_registry_request_get() {
    let req = RegistryRequest::get("https://example.com/index.json", "fetch index");
    assert_eq!(req.method, RequestMethod::Get);
    assert_eq!(req.url, "https://example.com/index.json");
    assert!(req.body.is_none());
    assert_eq!(req.description, "fetch index");
}

#[test]
fn test_registry_request_put_with_body() {
    let body = b"hello".to_vec();
    let req = RegistryRequest::put("https://example.com/upload", body.clone(), "upload");
    assert_eq!(req.method, RequestMethod::Put);
    assert_eq!(req.body, Some(body));
}

#[test]
fn test_registry_request_with_header() {
    let req = RegistryRequest::get("https://example.com", "test")
        .with_header("Authorization", "Bearer token123")
        .with_header("Accept", "application/json");
    assert_eq!(req.headers.len(), 2);
    assert_eq!(req.headers[0].0, "Authorization");
    assert_eq!(req.headers[0].1, "Bearer token123");
}

// =============================================================================
// DeploymentRegistryConfig
// =============================================================================

#[test]
fn test_deployment_config_new() {
    let config = DeploymentRegistryConfig::new("https://my-registry.dev/api");
    assert_eq!(config.base_url, "https://my-registry.dev/api");
    assert!(config.api_token.is_none());
    assert_eq!(config.timeout_secs, 30);
    assert!(config.verify_tls);
    assert!(!config.include_prereleases);
}

#[test]
fn test_deployment_config_builder() {
    let config = DeploymentRegistryConfig::new("https://r.dev")
        .with_token("mytoken")
        .with_timeout(60)
        .with_tls_verification(false)
        .with_prereleases(true);
    assert_eq!(config.api_token, Some("mytoken".to_string()));
    assert_eq!(config.timeout_secs, 60);
    assert!(!config.verify_tls);
    assert!(config.include_prereleases);
}

#[test]
fn test_deployment_config_default() {
    let config = DeploymentRegistryConfig::default();
    assert_eq!(config.base_url, "https://registry.lumen.dev/api/v1");
}

// =============================================================================
// DeploymentPackageMetadata
// =============================================================================

#[test]
fn test_package_metadata_new() {
    let meta = DeploymentPackageMetadata::new("my-package");
    assert_eq!(meta.name, "my-package");
    assert!(meta.versions.is_empty());
    assert!(meta.latest.is_none());
    assert!(meta.yanked.is_empty());
}

#[test]
fn test_package_metadata_yanked() {
    let mut meta = DeploymentPackageMetadata::new("pkg");
    meta.versions = vec!["1.0.0".into(), "1.1.0".into(), "1.2.0".into()];
    meta.yanked
        .insert("1.1.0".into(), "security vulnerability".into());

    assert!(!meta.is_yanked("1.0.0"));
    assert!(meta.is_yanked("1.1.0"));
    assert!(!meta.is_yanked("1.2.0"));
}

#[test]
fn test_package_metadata_available_versions() {
    let mut meta = DeploymentPackageMetadata::new("pkg");
    meta.versions = vec!["1.0.0".into(), "1.1.0".into(), "1.2.0".into()];
    meta.yanked.insert("1.1.0".into(), "bad".into());

    let available = meta.available_versions();
    assert_eq!(available, vec!["1.0.0", "1.2.0"]);
}

#[test]
fn test_package_metadata_resolve() {
    let mut meta = DeploymentPackageMetadata::new("pkg");
    meta.versions = vec![
        "1.0.0".into(),
        "1.1.0".into(),
        "1.2.0".into(),
        "2.0.0".into(),
    ];

    let req = VersionReq::parse("^1.0.0").unwrap();
    let resolved = meta.resolve(&req).unwrap();
    assert_eq!(resolved, "1.2.0");
}

#[test]
fn test_package_metadata_resolve_skips_yanked() {
    let mut meta = DeploymentPackageMetadata::new("pkg");
    meta.versions = vec!["1.0.0".into(), "1.1.0".into()];
    meta.yanked.insert("1.1.0".into(), "broken".into());

    let req = VersionReq::parse("^1.0.0").unwrap();
    let resolved = meta.resolve(&req).unwrap();
    assert_eq!(resolved, "1.0.0");
}

#[test]
fn test_package_metadata_resolve_no_match() {
    let mut meta = DeploymentPackageMetadata::new("pkg");
    meta.versions = vec!["1.0.0".into()];

    let req = VersionReq::parse("^2.0.0").unwrap();
    assert!(meta.resolve(&req).is_none());
}

// =============================================================================
// DeploymentRegistryClient — requests
// =============================================================================

fn test_client() -> DeploymentRegistryClient {
    let config =
        DeploymentRegistryConfig::new("https://registry.test/api/v1").with_token("test-token");
    DeploymentRegistryClient::new(config)
}

#[test]
fn test_client_fetch_index_request() {
    let client = test_client();
    let req = client.fetch_index_request();
    assert_eq!(req.method, RequestMethod::Get);
    assert_eq!(req.url, "https://registry.test/api/v1/index.json");
    assert!(req
        .headers
        .iter()
        .any(|(k, v)| k == "Authorization" && v.contains("test-token")));
}

#[test]
fn test_client_fetch_package_request_simple() {
    let client = test_client();
    let req = client.fetch_package_request("my-pkg");
    assert_eq!(
        req.url,
        "https://registry.test/api/v1/packages/my-pkg/index.json"
    );
}

#[test]
fn test_client_fetch_package_request_scoped() {
    let client = test_client();
    let req = client.fetch_package_request("@org/my-pkg");
    assert_eq!(
        req.url,
        "https://registry.test/api/v1/packages/@org/my-pkg/index.json"
    );
}

#[test]
fn test_client_fetch_version_request() {
    let client = test_client();
    let req = client.fetch_version_request("my-pkg", "1.2.3");
    assert_eq!(
        req.url,
        "https://registry.test/api/v1/packages/my-pkg/1.2.3.json"
    );
}

#[test]
fn test_client_download_artifact_request() {
    let client = test_client();
    let req = client
        .download_artifact_request("sha256:abcdef1234567890")
        .unwrap();
    assert_eq!(req.method, RequestMethod::Get);
    assert!(req.url.contains("artifacts/sha256/ab/cdef1234567890"));
}

#[test]
fn test_client_download_artifact_bad_cid() {
    let client = test_client();
    assert!(client.download_artifact_request("invalid:xyz").is_err());
}

#[test]
fn test_client_publish_request() {
    let client = test_client();
    let body = b"{}".to_vec();
    let req = client.publish_request("my-pkg", "1.0.0", body.clone());
    assert_eq!(req.method, RequestMethod::Put);
    assert_eq!(
        req.url,
        "https://registry.test/api/v1/packages/my-pkg/1.0.0.json"
    );
    assert_eq!(req.body, Some(body));
    assert!(req.headers.iter().any(|(k, _)| k == "Content-Type"));
}

#[test]
fn test_client_yank_request() {
    let client = test_client();
    let req = client.yank_request("my-pkg", "1.0.0");
    assert_eq!(req.method, RequestMethod::Post);
    assert!(req.url.ends_with("/my-pkg/1.0.0/yank"));
}

#[test]
fn test_client_artifact_exists_request() {
    let client = test_client();
    let req = client
        .artifact_exists_request("sha256:abcdef1234567890")
        .unwrap();
    assert_eq!(req.method, RequestMethod::Head);
}

// =============================================================================
// DeploymentRegistryClient — cache
// =============================================================================

#[test]
fn test_client_cache_basic() {
    let mut client = test_client();
    assert_eq!(client.cache_size(), 0);

    let meta = DeploymentPackageMetadata::new("cached-pkg");
    client.cache_package(meta, Some("etag-123".into()));

    assert_eq!(client.cache_size(), 1);
    let cached = client.get_cached("cached-pkg").unwrap();
    assert_eq!(cached.metadata.name, "cached-pkg");
    assert_eq!(cached.etag, Some("etag-123".to_string()));
}

#[test]
fn test_client_cache_miss() {
    let client = test_client();
    assert!(client.get_cached("missing").is_none());
}

#[test]
fn test_client_clear_cache() {
    let mut client = test_client();
    client.cache_package(DeploymentPackageMetadata::new("a"), None);
    client.cache_package(DeploymentPackageMetadata::new("b"), None);
    assert_eq!(client.cache_size(), 2);

    client.clear_cache();
    assert_eq!(client.cache_size(), 0);
}

#[test]
fn test_client_resolve_cached() {
    let mut client = test_client();
    let mut meta = DeploymentPackageMetadata::new("pkg");
    meta.versions = vec!["1.0.0".into(), "1.1.0".into(), "2.0.0".into()];
    client.cache_package(meta, None);

    let req = VersionReq::parse("^1.0.0").unwrap();
    let resolved = client.resolve_cached("pkg", &req).unwrap();
    assert_eq!(resolved, "1.1.0");

    assert!(client.resolve_cached("missing", &req).is_none());
}

#[test]
fn test_client_resolve_all_cached() {
    let mut client = test_client();

    let mut meta_a = DeploymentPackageMetadata::new("a");
    meta_a.versions = vec!["1.0.0".into(), "1.2.0".into()];
    client.cache_package(meta_a, None);

    let mut meta_b = DeploymentPackageMetadata::new("b");
    meta_b.versions = vec!["2.0.0".into(), "2.1.0".into()];
    client.cache_package(meta_b, None);

    let requirements = vec![
        ("a".into(), VersionReq::parse("^1.0.0").unwrap()),
        ("b".into(), VersionReq::parse("^2.0.0").unwrap()),
        ("missing".into(), VersionReq::parse("*").unwrap()),
    ];

    let resolved = client.resolve_all_cached(&requirements);
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved["a"], "1.2.0");
    assert_eq!(resolved["b"], "2.1.0");
    assert!(!resolved.contains_key("missing"));
}

#[test]
fn test_client_conditional_request_with_etag() {
    let mut client = test_client();
    let meta = DeploymentPackageMetadata::new("etag-pkg");
    client.cache_package(meta, Some("W/\"abc\"".into()));

    let req = client.fetch_package_request("etag-pkg");
    assert!(req
        .headers
        .iter()
        .any(|(k, v)| k == "If-None-Match" && v == "W/\"abc\""));
}

#[test]
fn test_client_no_auth_without_token() {
    let config = DeploymentRegistryConfig::new("https://r.dev");
    let client = DeploymentRegistryClient::new(config);
    let req = client.fetch_index_request();
    assert!(!req.headers.iter().any(|(k, _)| k == "Authorization"));
}

#[test]
fn test_client_trailing_slash_base_url() {
    let config = DeploymentRegistryConfig::new("https://r.dev/api/v1/");
    let client = DeploymentRegistryClient::new(config);
    let req = client.fetch_index_request();
    // Should not have double slashes
    assert!(!req.url.contains("//index.json"));
    assert!(req.url.ends_with("/index.json"));
}
