//! Merkle tree-based transparency log for package publishing.
//!
//! This module implements a cryptographic transparency log that ensures
//! all package publications are recorded in an append-only, tamper-evident
//! data structure. It uses a Merkle tree to provide efficient inclusion
//! and consistency proofs.
//!
//! ## Design
//!
//! - **MerkleTree**: Binary hash tree with SHA-256 leaf and node hashing
//! - **InclusionProof**: Proves a leaf exists in the tree at a given root
//! - **ConsistencyProof**: Proves a smaller tree is a prefix of a larger tree
//! - **TransparencyLog**: Append-only log of package publish events
//!
//! ## Security Properties
//!
//! - Append-only: Old entries cannot be removed or modified
//! - Tamper-evident: Any modification changes the root hash
//! - Verifiable: Third parties can verify inclusion and consistency

use sha2::{Digest, Sha256};
use std::fmt;

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur during transparency log operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransparencyError {
    /// A proof failed verification.
    InvalidProof(String),
    /// The log data is corrupted.
    CorruptedLog(String),
    /// An entry is invalid.
    InvalidEntry(String),
    /// Serialization or deserialization failed.
    SerializationError(String),
}

impl fmt::Display for TransparencyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransparencyError::InvalidProof(msg) => write!(f, "invalid proof: {}", msg),
            TransparencyError::CorruptedLog(msg) => write!(f, "corrupted log: {}", msg),
            TransparencyError::InvalidEntry(msg) => write!(f, "invalid entry: {}", msg),
            TransparencyError::SerializationError(msg) => {
                write!(f, "serialization error: {}", msg)
            }
        }
    }
}

impl std::error::Error for TransparencyError {}

// =============================================================================
// Proof Types
// =============================================================================

/// Position of a sibling node in an inclusion proof path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofPosition {
    /// The sibling is on the left side.
    Left,
    /// The sibling is on the right side.
    Right,
}

/// A single node in an inclusion proof path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofNode {
    /// The hash of the sibling node.
    pub hash: [u8; 32],
    /// Whether the sibling is on the left or right.
    pub position: ProofPosition,
}

/// Proof that a leaf is included in the Merkle tree at a given root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InclusionProof {
    /// Index of the leaf being proved.
    pub leaf_index: usize,
    /// Total number of leaves in the tree when the proof was generated.
    pub tree_size: usize,
    /// Path of sibling hashes from leaf to root.
    pub hashes: Vec<ProofNode>,
}

/// Proof that an older tree is a consistent prefix of a newer tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsistencyProof {
    /// Size of the older tree.
    pub old_size: usize,
    /// Size of the newer tree.
    pub new_size: usize,
    /// Hashes needed to verify consistency.
    pub hashes: Vec<[u8; 32]>,
}

// =============================================================================
// MerkleTree
// =============================================================================

/// A Merkle tree supporting incremental leaf addition and proof generation.
///
/// The tree uses SHA-256 for hashing. Leaf hashes use a `0x00` domain separator
/// prefix, and internal node hashes use a `0x01` prefix, preventing second
/// pre-image attacks.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    /// The leaf hashes (level 0).
    leaves: Vec<[u8; 32]>,
    /// Internal nodes by level. `nodes[0]` is the level above leaves.
    nodes: Vec<Vec<[u8; 32]>>,
}

impl MerkleTree {
    /// Create an empty Merkle tree.
    pub fn new() -> Self {
        Self {
            leaves: Vec::new(),
            nodes: Vec::new(),
        }
    }

    /// Create a Merkle tree from pre-computed leaf hashes.
    pub fn from_leaves(leaves: Vec<[u8; 32]>) -> Self {
        let mut tree = Self {
            leaves,
            nodes: Vec::new(),
        };
        tree.rebuild();
        tree
    }

    /// Add a leaf to the tree by hashing raw data.
    pub fn add_leaf(&mut self, data: &[u8]) {
        let hash = Self::hash_leaf(data);
        self.leaves.push(hash);
        self.rebuild();
    }

    /// Get the root hash. Returns `None` if the tree is empty.
    pub fn root(&self) -> Option<[u8; 32]> {
        if self.leaves.is_empty() {
            return None;
        }
        if self.leaves.len() == 1 {
            return Some(self.leaves[0]);
        }
        self.nodes.last().and_then(|level| {
            if level.len() == 1 {
                Some(level[0])
            } else {
                None
            }
        })
    }

    /// Get the number of leaves.
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    /// Generate an inclusion proof for the leaf at `index`.
    pub fn proof(&self, index: usize) -> Option<InclusionProof> {
        if index >= self.leaves.len() || self.leaves.is_empty() {
            return None;
        }
        if self.leaves.len() == 1 {
            return Some(InclusionProof {
                leaf_index: index,
                tree_size: self.leaves.len(),
                hashes: Vec::new(),
            });
        }

        let mut path = Vec::new();
        let mut idx = index;

        // Level 0: leaves
        let mut current_level = &self.leaves;
        // Walk up each level
        for level_nodes in &self.nodes {
            let sibling_idx = idx ^ 1; // Toggle last bit
            if sibling_idx < current_level.len() {
                let position = if idx.is_multiple_of(2) {
                    ProofPosition::Right
                } else {
                    ProofPosition::Left
                };
                path.push(ProofNode {
                    hash: current_level[sibling_idx],
                    position,
                });
            }
            idx /= 2;
            current_level = level_nodes;
        }

        Some(InclusionProof {
            leaf_index: index,
            tree_size: self.leaves.len(),
            hashes: path,
        })
    }

    /// Verify an inclusion proof against a root hash.
    pub fn verify_proof(leaf: &[u8; 32], proof: &InclusionProof, root: &[u8; 32]) -> bool {
        if proof.tree_size == 0 {
            return false;
        }
        if proof.tree_size == 1 {
            return proof.hashes.is_empty() && leaf == root;
        }

        let mut current = *leaf;
        for node in &proof.hashes {
            current = match node.position {
                ProofPosition::Left => Self::hash_pair(&node.hash, &current),
                ProofPosition::Right => Self::hash_pair(&current, &node.hash),
            };
        }
        current == *root
    }

    /// Generate a consistency proof between an older tree of size `old_size`
    /// and the current tree.
    pub fn consistency_proof(&self, old_size: usize) -> Option<ConsistencyProof> {
        let new_size = self.leaves.len();
        if old_size == 0 || old_size > new_size {
            return None;
        }
        if old_size == new_size {
            // Trees are the same size; no proof needed beyond the root.
            return Some(ConsistencyProof {
                old_size,
                new_size,
                hashes: Vec::new(),
            });
        }

        // Build the old tree to compute its subtree hashes
        let old_leaves = self.leaves[..old_size].to_vec();
        let old_tree = MerkleTree::from_leaves(old_leaves);

        // Collect the proof hashes: the old root + any additional subtree
        // roots needed to reconstruct the new root from the old.
        let mut hashes = Vec::new();

        // Include the old root
        if let Some(old_root) = old_tree.root() {
            hashes.push(old_root);
        }

        // We need to provide the hashes of the subtrees that, combined with
        // the old tree, produce the new tree root. Walk the new tree levels
        // and collect sibling hashes for the "right spine" of the old tree.
        Self::collect_consistency_hashes(&self.leaves, old_size, new_size, &mut hashes);

        Some(ConsistencyProof {
            old_size,
            new_size,
            hashes,
        })
    }

    /// Verify a consistency proof.
    ///
    /// Checks that the old tree (with `old_root` and `old_size`) is a
    /// consistent prefix of the new tree (with `new_root` and `new_size`).
    pub fn verify_consistency(
        old_root: &[u8; 32],
        old_size: usize,
        new_root: &[u8; 32],
        new_size: usize,
        proof: &ConsistencyProof,
    ) -> bool {
        if proof.old_size != old_size || proof.new_size != new_size {
            return false;
        }
        if old_size == 0 || old_size > new_size {
            return false;
        }
        if old_size == new_size {
            return old_root == new_root && proof.hashes.is_empty();
        }
        // Proof must contain at least the old root
        if proof.hashes.is_empty() {
            return false;
        }
        // First hash must be the old root
        if proof.hashes[0] != *old_root {
            return false;
        }

        // Reconstruct the new root from the proof hashes.
        // Start with the old root (hashes[0]), then combine with
        // each subsequent hash.
        let mut computed = proof.hashes[0];
        for hash in &proof.hashes[1..] {
            computed = Self::hash_pair(&computed, hash);
        }
        computed == *new_root
    }

    /// Collect additional hashes for the consistency proof.
    fn collect_consistency_hashes(
        leaves: &[[u8; 32]],
        old_size: usize,
        new_size: usize,
        hashes: &mut Vec<[u8; 32]>,
    ) {
        // We need to find the subtree roots that cover leaves[old_size..new_size]
        // and combine them to reconstruct the full root.
        //
        // Strategy: decompose the right side into power-of-two chunks
        // and compute the hash of each chunk.
        let mut remaining_start = old_size;
        let remaining_end = new_size;

        while remaining_start < remaining_end {
            // Find the largest power of 2 that fits
            let chunk_size = largest_power_of_two_leq(remaining_end - remaining_start);
            let chunk_end = remaining_start + chunk_size;
            let chunk_hash = Self::compute_subtree_root(&leaves[remaining_start..chunk_end]);
            hashes.push(chunk_hash);
            remaining_start = chunk_end;
        }
    }

    /// Compute the Merkle root of a slice of leaf hashes.
    fn compute_subtree_root(leaves: &[[u8; 32]]) -> [u8; 32] {
        if leaves.is_empty() {
            return [0u8; 32];
        }
        if leaves.len() == 1 {
            return leaves[0];
        }

        let mut current_level: Vec<[u8; 32]> = leaves.to_vec();
        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            let mut i = 0;
            while i < current_level.len() {
                if i + 1 < current_level.len() {
                    next_level.push(Self::hash_pair(&current_level[i], &current_level[i + 1]));
                } else {
                    // Odd node: promote it
                    next_level.push(current_level[i]);
                }
                i += 2;
            }
            current_level = next_level;
        }
        current_level[0]
    }

    /// Hash two child nodes together to produce a parent hash.
    /// Uses a `0x01` domain separator prefix.
    fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update([0x01]); // Internal node domain separator
        hasher.update(left);
        hasher.update(right);
        let result = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        out
    }

    /// Hash raw leaf data.
    /// Uses a `0x00` domain separator prefix.
    pub fn hash_leaf(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update([0x00]); // Leaf domain separator
        hasher.update(data);
        let result = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        out
    }

    /// Rebuild internal node levels from the current leaves.
    fn rebuild(&mut self) {
        self.nodes.clear();
        if self.leaves.len() <= 1 {
            return;
        }

        let mut current_level = self.leaves.clone();
        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            let mut i = 0;
            while i < current_level.len() {
                if i + 1 < current_level.len() {
                    next_level.push(Self::hash_pair(&current_level[i], &current_level[i + 1]));
                } else {
                    // Odd node: promote to next level
                    next_level.push(current_level[i]);
                }
                i += 2;
            }
            self.nodes.push(next_level.clone());
            current_level = next_level;
        }
    }
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the largest power of 2 that is <= n.
fn largest_power_of_two_leq(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut p = 1;
    while p * 2 <= n {
        p *= 2;
    }
    p
}

// =============================================================================
// LogEntry
// =============================================================================

/// A single entry in the transparency log, recording a package publish event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Monotonically increasing sequence number.
    pub sequence: u64,
    /// Unix timestamp of when the entry was created.
    pub timestamp: u64,
    /// Name of the package.
    pub package_name: String,
    /// Version of the package.
    pub package_version: String,
    /// Content hash of the package artifact (hex-encoded SHA-256).
    pub content_hash: String,
    /// Publisher identity (e.g., email or key fingerprint).
    pub publisher: String,
    /// Optional cryptographic signature over the entry.
    pub signature: Option<String>,
}

impl LogEntry {
    /// Serialize this entry to bytes for hashing.
    fn to_bytes(&self) -> Vec<u8> {
        format!(
            "{}:{}:{}:{}:{}:{}",
            self.sequence,
            self.timestamp,
            self.package_name,
            self.package_version,
            self.content_hash,
            self.publisher,
        )
        .into_bytes()
    }

    /// Serialize to a single line for persistence.
    fn serialize_line(&self) -> String {
        let sig = self.signature.as_deref().unwrap_or("");
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.sequence,
            self.timestamp,
            self.package_name,
            self.package_version,
            self.content_hash,
            self.publisher,
            sig,
        )
    }

    /// Deserialize from a single line.
    fn deserialize_line(line: &str) -> Result<Self, TransparencyError> {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 6 {
            return Err(TransparencyError::SerializationError(format!(
                "expected at least 6 tab-separated fields, got {}",
                parts.len()
            )));
        }
        let sequence = parts[0].parse::<u64>().map_err(|e| {
            TransparencyError::SerializationError(format!("invalid sequence: {}", e))
        })?;
        let timestamp = parts[1].parse::<u64>().map_err(|e| {
            TransparencyError::SerializationError(format!("invalid timestamp: {}", e))
        })?;
        let signature = if parts.len() > 6 && !parts[6].is_empty() {
            Some(parts[6].to_string())
        } else {
            None
        };
        Ok(LogEntry {
            sequence,
            timestamp,
            package_name: parts[2].to_string(),
            package_version: parts[3].to_string(),
            content_hash: parts[4].to_string(),
            publisher: parts[5].to_string(),
            signature,
        })
    }
}

// =============================================================================
// TransparencyLog
// =============================================================================

/// An append-only transparency log backed by a Merkle tree.
///
/// Each appended entry is hashed and added as a leaf to the Merkle tree,
/// enabling efficient inclusion and consistency proofs.
#[derive(Debug, Clone)]
pub struct TransparencyLog {
    tree: MerkleTree,
    entries: Vec<LogEntry>,
}

impl TransparencyLog {
    /// Create an empty transparency log.
    pub fn new() -> Self {
        Self {
            tree: MerkleTree::new(),
            entries: Vec::new(),
        }
    }

    /// Append an entry to the log. Returns the assigned sequence number.
    ///
    /// The entry's `sequence` field is overwritten with the next monotonic
    /// sequence number before insertion.
    pub fn append(&mut self, mut entry: LogEntry) -> u64 {
        let seq = self.entries.len() as u64;
        entry.sequence = seq;
        let data = entry.to_bytes();
        self.tree.add_leaf(&data);
        self.entries.push(entry);
        seq
    }

    /// Get an entry by sequence number.
    pub fn get_entry(&self, sequence: u64) -> Option<&LogEntry> {
        self.entries.get(sequence as usize)
    }

    /// Get an inclusion proof for the entry at `sequence`.
    pub fn get_proof(&self, sequence: u64) -> Option<InclusionProof> {
        self.tree.proof(sequence as usize)
    }

    /// Verify that an entry at `sequence` is included in the current tree.
    pub fn verify_entry(&self, sequence: u64) -> bool {
        let entry = match self.entries.get(sequence as usize) {
            Some(e) => e,
            None => return false,
        };
        let root = match self.tree.root() {
            Some(r) => r,
            None => return false,
        };
        let proof = match self.tree.proof(sequence as usize) {
            Some(p) => p,
            None => return false,
        };
        let leaf_hash = MerkleTree::hash_leaf(&entry.to_bytes());
        MerkleTree::verify_proof(&leaf_hash, &proof, &root)
    }

    /// Get the current root hash.
    pub fn root_hash(&self) -> Option<[u8; 32]> {
        self.tree.root()
    }

    /// Get the number of entries in the log.
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// Get all entries for a specific package name.
    pub fn entries_for_package(&self, name: &str) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| e.package_name == name)
            .collect()
    }

    /// Get the latest entry for a specific package (by sequence number).
    pub fn latest_version(&self, name: &str) -> Option<&LogEntry> {
        self.entries.iter().rev().find(|e| e.package_name == name)
    }

    /// Serialize the log to a string representation.
    pub fn serialize(&self) -> String {
        let mut lines = Vec::new();
        // Header
        lines.push("LUMEN-TRANSPARENCY-LOG-V1".to_string());
        lines.push(format!("size:{}", self.entries.len()));
        if let Some(root) = self.tree.root() {
            lines.push(format!("root:{}", hex_encode(&root)));
        } else {
            lines.push("root:empty".to_string());
        }
        lines.push("---".to_string());
        // Entries
        for entry in &self.entries {
            lines.push(entry.serialize_line());
        }
        lines.join("\n")
    }

    /// Deserialize a log from its string representation.
    pub fn deserialize(input: &str) -> Result<Self, TransparencyError> {
        let mut lines = input.lines();

        // Header
        let header = lines
            .next()
            .ok_or_else(|| TransparencyError::SerializationError("empty input".to_string()))?;
        if header != "LUMEN-TRANSPARENCY-LOG-V1" {
            return Err(TransparencyError::SerializationError(format!(
                "unsupported format: {}",
                header
            )));
        }

        let size_line = lines.next().ok_or_else(|| {
            TransparencyError::SerializationError("missing size line".to_string())
        })?;
        let expected_size: usize = size_line
            .strip_prefix("size:")
            .ok_or_else(|| {
                TransparencyError::SerializationError(format!("invalid size line: {}", size_line))
            })?
            .parse()
            .map_err(|e| TransparencyError::SerializationError(format!("invalid size: {}", e)))?;

        // Skip root line and separator
        let _root_line = lines.next();
        let _separator = lines.next();

        // Parse entries
        let mut log = TransparencyLog::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            let entry = LogEntry::deserialize_line(line)?;
            log.append(entry);
        }

        if log.size() != expected_size {
            return Err(TransparencyError::CorruptedLog(format!(
                "expected {} entries, got {}",
                expected_size,
                log.size()
            )));
        }

        Ok(log)
    }
}

impl Default for TransparencyLog {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(nibble_to_hex(b >> 4));
        s.push(nibble_to_hex(b & 0x0f));
    }
    s
}

fn nibble_to_hex(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => '0',
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::new();
        assert_eq!(tree.leaf_count(), 0);
        assert!(tree.root().is_none());
        assert!(tree.proof(0).is_none());
    }

    #[test]
    fn test_single_leaf() {
        let mut tree = MerkleTree::new();
        tree.add_leaf(b"hello");
        assert_eq!(tree.leaf_count(), 1);
        assert!(tree.root().is_some());

        let proof = tree.proof(0).unwrap();
        assert!(proof.hashes.is_empty());
        let leaf = MerkleTree::hash_leaf(b"hello");
        assert!(MerkleTree::verify_proof(
            &leaf,
            &proof,
            &tree.root().unwrap()
        ));
    }

    #[test]
    fn test_two_leaves() {
        let mut tree = MerkleTree::new();
        tree.add_leaf(b"a");
        tree.add_leaf(b"b");
        assert_eq!(tree.leaf_count(), 2);

        let root = tree.root().unwrap();

        // Verify proof for leaf 0
        let proof0 = tree.proof(0).unwrap();
        let leaf0 = MerkleTree::hash_leaf(b"a");
        assert!(MerkleTree::verify_proof(&leaf0, &proof0, &root));

        // Verify proof for leaf 1
        let proof1 = tree.proof(1).unwrap();
        let leaf1 = MerkleTree::hash_leaf(b"b");
        assert!(MerkleTree::verify_proof(&leaf1, &proof1, &root));
    }

    #[test]
    fn test_proof_invalid_leaf() {
        let mut tree = MerkleTree::new();
        tree.add_leaf(b"a");
        tree.add_leaf(b"b");

        let root = tree.root().unwrap();
        let proof0 = tree.proof(0).unwrap();

        // Wrong leaf should fail
        let wrong_leaf = MerkleTree::hash_leaf(b"wrong");
        assert!(!MerkleTree::verify_proof(&wrong_leaf, &proof0, &root));
    }

    #[test]
    fn test_from_leaves() {
        let leaves = vec![
            MerkleTree::hash_leaf(b"a"),
            MerkleTree::hash_leaf(b"b"),
            MerkleTree::hash_leaf(b"c"),
        ];
        let tree = MerkleTree::from_leaves(leaves);
        assert_eq!(tree.leaf_count(), 3);
        assert!(tree.root().is_some());
    }

    #[test]
    fn test_transparency_error_display() {
        let e = TransparencyError::InvalidProof("bad".to_string());
        assert!(e.to_string().contains("bad"));
        let e = TransparencyError::CorruptedLog("oops".to_string());
        assert!(e.to_string().contains("oops"));
        let e = TransparencyError::InvalidEntry("no".to_string());
        assert!(e.to_string().contains("no"));
        let e = TransparencyError::SerializationError("fail".to_string());
        assert!(e.to_string().contains("fail"));
    }
}
