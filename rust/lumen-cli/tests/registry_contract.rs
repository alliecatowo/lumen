//! The registry worker (workers/registry) and this CLI agree on the static registry
//! layout through golden files that BOTH sides test against: the worker's vitest
//! suite asserts it serves exactly these documents, and this test parses them with the
//! CLI's own types.

use lumen_cli::wares::{GlobalIndex, RegistryPackageIndex, RegistryVersionMetadata};
use std::path::PathBuf;

fn golden(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../workers/registry/test/contract")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn package_index_parses_with_the_cli_types() {
    let idx: RegistryPackageIndex = serde_json::from_str(&golden("package-index.json")).unwrap();
    assert_eq!(idx.name, "@t/dep");
    assert_eq!(idx.versions, ["1.0.0", "1.1.0-rc.1"]);
    assert_eq!(idx.latest.as_deref(), Some("1.0.0"));
    assert_eq!(idx.prereleases, ["1.1.0-rc.1"]);
    assert!(idx.yanked.is_empty());
}

#[test]
fn version_metadata_parses_and_carries_a_downloadable_artifact() {
    let meta: RegistryVersionMetadata =
        serde_json::from_str(&golden("version-metadata.json")).unwrap();
    assert_eq!(meta.name, "@t/dep");
    assert_eq!(meta.version, "1.0.0");
    assert_eq!(meta.deps.get("@t/leaf").map(String::as_str), Some("^1.0.0"));
    assert!(!meta.yanked);

    let artifact = &meta.artifacts[0];
    assert_eq!(artifact.kind, "tgz");
    assert!(artifact.hash.starts_with("sha256:"));
    // Relative URLs are joined onto the registry base (…/api/v1) by the client.
    let url = artifact.url.as_deref().expect("artifact url");
    assert!(
        !url.contains("://") && url.starts_with("wares/@t/dep/"),
        "{url}"
    );
}

#[test]
fn global_index_parses() {
    let idx: GlobalIndex = serde_json::from_str(&golden("global-index.json")).unwrap();
    assert_eq!(idx.package_count, Some(1));
    assert_eq!(idx.packages[0].name, "@t/dep");
    assert_eq!(idx.packages[0].latest.as_deref(), Some("1.0.0"));
}
