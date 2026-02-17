//! Tests for the Merkle tree-based transparency log (T091).

use lumen_cli::transparency::*;

// =============================================================================
// MerkleTree — construction and basics
// =============================================================================

#[test]
fn test_merkle_empty_tree_has_no_root() {
    let tree = MerkleTree::new();
    assert!(tree.root().is_none());
    assert_eq!(tree.leaf_count(), 0);
}

#[test]
fn test_merkle_default_is_empty() {
    let tree = MerkleTree::default();
    assert!(tree.root().is_none());
    assert_eq!(tree.leaf_count(), 0);
}

#[test]
fn test_merkle_single_leaf_root() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"hello");
    assert_eq!(tree.leaf_count(), 1);
    let root = tree.root().expect("single-leaf tree must have a root");
    // Root of a single-leaf tree equals the leaf hash
    let leaf = MerkleTree::hash_leaf(b"hello");
    assert_eq!(root, leaf);
}

#[test]
fn test_merkle_two_leaves_root_differs_from_single() {
    let mut t1 = MerkleTree::new();
    t1.add_leaf(b"a");
    let root1 = t1.root().unwrap();

    let mut t2 = MerkleTree::new();
    t2.add_leaf(b"a");
    t2.add_leaf(b"b");
    let root2 = t2.root().unwrap();

    assert_ne!(root1, root2);
}

#[test]
fn test_merkle_deterministic_root() {
    let mut t1 = MerkleTree::new();
    t1.add_leaf(b"x");
    t1.add_leaf(b"y");
    t1.add_leaf(b"z");

    let mut t2 = MerkleTree::new();
    t2.add_leaf(b"x");
    t2.add_leaf(b"y");
    t2.add_leaf(b"z");

    assert_eq!(t1.root(), t2.root());
}

#[test]
fn test_merkle_from_leaves() {
    let leaves = vec![
        MerkleTree::hash_leaf(b"a"),
        MerkleTree::hash_leaf(b"b"),
        MerkleTree::hash_leaf(b"c"),
        MerkleTree::hash_leaf(b"d"),
    ];
    let tree = MerkleTree::from_leaves(leaves);
    assert_eq!(tree.leaf_count(), 4);
    assert!(tree.root().is_some());
}

#[test]
fn test_merkle_from_leaves_matches_incremental() {
    let leaves = vec![MerkleTree::hash_leaf(b"one"), MerkleTree::hash_leaf(b"two")];
    let batch = MerkleTree::from_leaves(leaves);

    let mut incr = MerkleTree::new();
    incr.add_leaf(b"one");
    incr.add_leaf(b"two");

    assert_eq!(batch.root(), incr.root());
}

// =============================================================================
// Inclusion proofs
// =============================================================================

#[test]
fn test_proof_out_of_range_returns_none() {
    let tree = MerkleTree::new();
    assert!(tree.proof(0).is_none());

    let mut t = MerkleTree::new();
    t.add_leaf(b"a");
    assert!(t.proof(1).is_none());
}

#[test]
fn test_proof_single_leaf_empty_path() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"only");
    let proof = tree.proof(0).unwrap();
    assert!(proof.hashes.is_empty());
    assert_eq!(proof.leaf_index, 0);
    assert_eq!(proof.tree_size, 1);
}

#[test]
fn test_proof_verifies_for_two_leaves() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"left");
    tree.add_leaf(b"right");
    let root = tree.root().unwrap();

    for i in 0..2 {
        let data = if i == 0 { b"left" as &[u8] } else { b"right" };
        let leaf = MerkleTree::hash_leaf(data);
        let proof = tree.proof(i).unwrap();
        assert!(MerkleTree::verify_proof(&leaf, &proof, &root));
    }
}

#[test]
fn test_proof_verifies_for_power_of_two_leaves() {
    let items: Vec<&[u8]> = vec![b"a", b"b", b"c", b"d"];
    let mut tree = MerkleTree::new();
    for item in &items {
        tree.add_leaf(item);
    }
    let root = tree.root().unwrap();

    for (i, item) in items.iter().enumerate() {
        let leaf = MerkleTree::hash_leaf(item);
        let proof = tree.proof(i).unwrap();
        assert!(
            MerkleTree::verify_proof(&leaf, &proof, &root),
            "proof failed for leaf {}",
            i
        );
    }
}

#[test]
fn test_proof_verifies_for_odd_leaf_count() {
    let items: Vec<&[u8]> = vec![b"1", b"2", b"3", b"4", b"5"];
    let mut tree = MerkleTree::new();
    for item in &items {
        tree.add_leaf(item);
    }
    let root = tree.root().unwrap();

    for (i, item) in items.iter().enumerate() {
        let leaf = MerkleTree::hash_leaf(item);
        let proof = tree.proof(i).unwrap();
        assert!(
            MerkleTree::verify_proof(&leaf, &proof, &root),
            "proof failed for leaf {}",
            i
        );
    }
}

#[test]
fn test_proof_fails_with_wrong_leaf() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"correct");
    tree.add_leaf(b"other");
    let root = tree.root().unwrap();

    let proof = tree.proof(0).unwrap();
    let wrong = MerkleTree::hash_leaf(b"wrong");
    assert!(!MerkleTree::verify_proof(&wrong, &proof, &root));
}

#[test]
fn test_proof_fails_with_wrong_root() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"a");
    tree.add_leaf(b"b");

    let proof = tree.proof(0).unwrap();
    let leaf = MerkleTree::hash_leaf(b"a");
    let wrong_root = [0xffu8; 32];
    assert!(!MerkleTree::verify_proof(&leaf, &proof, &wrong_root));
}

#[test]
fn test_verify_proof_empty_tree_size_zero() {
    let leaf = MerkleTree::hash_leaf(b"x");
    let proof = InclusionProof {
        leaf_index: 0,
        tree_size: 0,
        hashes: Vec::new(),
    };
    assert!(!MerkleTree::verify_proof(&leaf, &proof, &leaf));
}

// =============================================================================
// Consistency proofs
// =============================================================================

#[test]
fn test_consistency_proof_same_size() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"a");
    tree.add_leaf(b"b");

    let cp = tree.consistency_proof(2).unwrap();
    assert_eq!(cp.old_size, 2);
    assert_eq!(cp.new_size, 2);
    assert!(cp.hashes.is_empty());
}

#[test]
fn test_consistency_proof_invalid_sizes() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"a");
    // old_size = 0 is invalid
    assert!(tree.consistency_proof(0).is_none());
    // old_size > new_size is invalid
    assert!(tree.consistency_proof(5).is_none());
}

#[test]
fn test_consistency_proof_verifies() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"a");
    tree.add_leaf(b"b");
    let old_root = tree.root().unwrap();
    let old_size = tree.leaf_count();

    tree.add_leaf(b"c");
    tree.add_leaf(b"d");
    let new_root = tree.root().unwrap();
    let new_size = tree.leaf_count();

    let cp = tree.consistency_proof(old_size).unwrap();
    assert!(MerkleTree::verify_consistency(
        &old_root, old_size, &new_root, new_size, &cp
    ));
}

#[test]
fn test_consistency_proof_fails_wrong_old_root() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"a");
    tree.add_leaf(b"b");
    let old_size = tree.leaf_count();

    tree.add_leaf(b"c");
    let new_root = tree.root().unwrap();
    let new_size = tree.leaf_count();

    let cp = tree.consistency_proof(old_size).unwrap();
    let wrong_old_root = [0xaau8; 32];
    assert!(!MerkleTree::verify_consistency(
        &wrong_old_root,
        old_size,
        &new_root,
        new_size,
        &cp
    ));
}

#[test]
fn test_consistency_size_mismatch_fails() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"a");
    tree.add_leaf(b"b");
    let old_root = tree.root().unwrap();

    tree.add_leaf(b"c");
    let new_root = tree.root().unwrap();

    let cp = tree.consistency_proof(2).unwrap();
    // Pass wrong old_size
    assert!(!MerkleTree::verify_consistency(
        &old_root, 1, &new_root, 3, &cp
    ));
}

// =============================================================================
// TransparencyLog — append and query
// =============================================================================

fn make_entry(name: &str, version: &str) -> LogEntry {
    LogEntry {
        sequence: 0,
        timestamp: 1700000000,
        package_name: name.to_string(),
        package_version: version.to_string(),
        content_hash: "sha256:abcdef".to_string(),
        publisher: "alice@example.com".to_string(),
        signature: None,
    }
}

#[test]
fn test_log_new_is_empty() {
    let log = TransparencyLog::new();
    assert_eq!(log.size(), 0);
    assert!(log.root_hash().is_none());
}

#[test]
fn test_log_default_is_empty() {
    let log = TransparencyLog::default();
    assert_eq!(log.size(), 0);
}

#[test]
fn test_log_append_assigns_sequence() {
    let mut log = TransparencyLog::new();
    let s0 = log.append(make_entry("pkg-a", "1.0.0"));
    let s1 = log.append(make_entry("pkg-a", "1.1.0"));
    assert_eq!(s0, 0);
    assert_eq!(s1, 1);
    assert_eq!(log.size(), 2);
}

#[test]
fn test_log_get_entry() {
    let mut log = TransparencyLog::new();
    log.append(make_entry("pkg-x", "0.1.0"));
    let entry = log.get_entry(0).unwrap();
    assert_eq!(entry.package_name, "pkg-x");
    assert_eq!(entry.package_version, "0.1.0");
    assert!(log.get_entry(1).is_none());
}

#[test]
fn test_log_entries_for_package() {
    let mut log = TransparencyLog::new();
    log.append(make_entry("foo", "1.0.0"));
    log.append(make_entry("bar", "1.0.0"));
    log.append(make_entry("foo", "1.1.0"));

    let foo_entries = log.entries_for_package("foo");
    assert_eq!(foo_entries.len(), 2);
    assert_eq!(foo_entries[0].package_version, "1.0.0");
    assert_eq!(foo_entries[1].package_version, "1.1.0");

    let bar_entries = log.entries_for_package("bar");
    assert_eq!(bar_entries.len(), 1);

    let nope = log.entries_for_package("nope");
    assert!(nope.is_empty());
}

#[test]
fn test_log_latest_version() {
    let mut log = TransparencyLog::new();
    log.append(make_entry("pkg", "1.0.0"));
    log.append(make_entry("other", "2.0.0"));
    log.append(make_entry("pkg", "1.1.0"));
    log.append(make_entry("pkg", "1.2.0"));

    let latest = log.latest_version("pkg").unwrap();
    assert_eq!(latest.package_version, "1.2.0");
    assert!(log.latest_version("missing").is_none());
}

// =============================================================================
// TransparencyLog — proofs
// =============================================================================

#[test]
fn test_log_verify_entry_valid() {
    let mut log = TransparencyLog::new();
    log.append(make_entry("a", "1.0.0"));
    log.append(make_entry("b", "1.0.0"));
    log.append(make_entry("c", "1.0.0"));

    assert!(log.verify_entry(0));
    assert!(log.verify_entry(1));
    assert!(log.verify_entry(2));
}

#[test]
fn test_log_verify_entry_out_of_range() {
    let mut log = TransparencyLog::new();
    log.append(make_entry("a", "1.0.0"));
    assert!(!log.verify_entry(5));
}

#[test]
fn test_log_get_proof() {
    let mut log = TransparencyLog::new();
    log.append(make_entry("a", "1.0.0"));
    log.append(make_entry("b", "2.0.0"));

    let proof = log.get_proof(0).unwrap();
    assert_eq!(proof.leaf_index, 0);
    assert_eq!(proof.tree_size, 2);
    assert!(log.get_proof(5).is_none());
}

#[test]
fn test_log_root_hash_changes_on_append() {
    let mut log = TransparencyLog::new();
    log.append(make_entry("a", "1.0.0"));
    let r1 = log.root_hash().unwrap();
    log.append(make_entry("b", "1.0.0"));
    let r2 = log.root_hash().unwrap();
    assert_ne!(r1, r2);
}

// =============================================================================
// Serialization / Deserialization
// =============================================================================

#[test]
fn test_log_serialize_deserialize_roundtrip() {
    let mut log = TransparencyLog::new();
    log.append(make_entry("foo", "1.0.0"));
    log.append(make_entry("bar", "2.0.0"));
    log.append(make_entry("foo", "1.1.0"));

    let serialized = log.serialize();
    let restored = TransparencyLog::deserialize(&serialized).unwrap();

    assert_eq!(restored.size(), log.size());
    assert_eq!(restored.root_hash(), log.root_hash());
}

#[test]
fn test_log_serialize_header() {
    let log = TransparencyLog::new();
    let s = log.serialize();
    assert!(s.starts_with("LUMEN-TRANSPARENCY-LOG-V1"));
    assert!(s.contains("size:0"));
    assert!(s.contains("root:empty"));
}

#[test]
fn test_log_deserialize_bad_header() {
    let result = TransparencyLog::deserialize("BAD-HEADER\nsize:0\nroot:empty\n---");
    assert!(result.is_err());
}

#[test]
fn test_log_deserialize_empty_input() {
    let result = TransparencyLog::deserialize("");
    assert!(result.is_err());
}

#[test]
fn test_log_deserialize_size_mismatch() {
    // Manually craft a log with wrong size
    let input = "LUMEN-TRANSPARENCY-LOG-V1\nsize:5\nroot:abc\n---\n";
    let result = TransparencyLog::deserialize(input);
    assert!(result.is_err());
    match result {
        Err(TransparencyError::CorruptedLog(msg)) => {
            assert!(msg.contains("expected 5"));
        }
        other => panic!("expected CorruptedLog, got {:?}", other),
    }
}

#[test]
fn test_log_entry_with_signature() {
    let mut log = TransparencyLog::new();
    let mut entry = make_entry("signed-pkg", "1.0.0");
    entry.signature = Some("sig_abc123".to_string());
    log.append(entry);

    let serialized = log.serialize();
    let restored = TransparencyLog::deserialize(&serialized).unwrap();
    let e = restored.get_entry(0).unwrap();
    assert_eq!(e.signature.as_deref(), Some("sig_abc123"));
}

#[test]
fn test_log_entry_without_signature_roundtrip() {
    let mut log = TransparencyLog::new();
    log.append(make_entry("unsigned", "1.0.0"));

    let serialized = log.serialize();
    let restored = TransparencyLog::deserialize(&serialized).unwrap();
    let e = restored.get_entry(0).unwrap();
    assert!(e.signature.is_none());
}

// =============================================================================
// Error types
// =============================================================================

#[test]
fn test_transparency_error_display_variants() {
    let e1 = TransparencyError::InvalidProof("bad hash".into());
    assert!(e1.to_string().contains("invalid proof"));
    assert!(e1.to_string().contains("bad hash"));

    let e2 = TransparencyError::CorruptedLog("missing entry".into());
    assert!(e2.to_string().contains("corrupted log"));

    let e3 = TransparencyError::InvalidEntry("no name".into());
    assert!(e3.to_string().contains("invalid entry"));

    let e4 = TransparencyError::SerializationError("parse fail".into());
    assert!(e4.to_string().contains("serialization error"));
}

#[test]
fn test_transparency_error_is_error_trait() {
    let e: Box<dyn std::error::Error> = Box::new(TransparencyError::InvalidProof("test".into()));
    assert!(!e.to_string().is_empty());
}

// =============================================================================
// Large tree tests
// =============================================================================

#[test]
fn test_merkle_large_tree_proofs() {
    let mut tree = MerkleTree::new();
    for i in 0..64u32 {
        tree.add_leaf(&i.to_le_bytes());
    }
    assert_eq!(tree.leaf_count(), 64);
    let root = tree.root().unwrap();

    // Verify every leaf
    for i in 0..64u32 {
        let leaf = MerkleTree::hash_leaf(&i.to_le_bytes());
        let proof = tree.proof(i as usize).unwrap();
        assert!(
            MerkleTree::verify_proof(&leaf, &proof, &root),
            "proof failed for leaf {}",
            i
        );
    }
}

#[test]
fn test_log_many_entries_verification() {
    let mut log = TransparencyLog::new();
    for i in 0..20 {
        log.append(make_entry("pkg", &format!("0.{}.0", i)));
    }
    assert_eq!(log.size(), 20);

    for seq in 0..20u64 {
        assert!(log.verify_entry(seq), "entry {} failed verification", seq);
    }
}

// =============================================================================
// Edge cases and proof-position coverage
// =============================================================================

#[test]
fn test_proof_position_variants() {
    // Just ensure we can construct both variants
    let left = ProofPosition::Left;
    let right = ProofPosition::Right;
    assert_ne!(left, right);
    assert_eq!(left, ProofPosition::Left);
}

#[test]
fn test_proof_node_clone_and_eq() {
    let node = ProofNode {
        hash: [0x42u8; 32],
        position: ProofPosition::Left,
    };
    let clone = node.clone();
    assert_eq!(node, clone);
}

#[test]
fn test_inclusion_proof_fields() {
    let proof = InclusionProof {
        leaf_index: 7,
        tree_size: 16,
        hashes: vec![],
    };
    assert_eq!(proof.leaf_index, 7);
    assert_eq!(proof.tree_size, 16);
}

#[test]
fn test_consistency_proof_clone_and_eq() {
    let cp = ConsistencyProof {
        old_size: 2,
        new_size: 4,
        hashes: vec![[0u8; 32]],
    };
    let clone = cp.clone();
    assert_eq!(cp, clone);
}

#[test]
fn test_merkle_tree_clone() {
    let mut tree = MerkleTree::new();
    tree.add_leaf(b"data");
    let clone = tree.clone();
    assert_eq!(tree.root(), clone.root());
    assert_eq!(tree.leaf_count(), clone.leaf_count());
}
