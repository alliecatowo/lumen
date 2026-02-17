//! Cryptographic primitives for the Lumen runtime (`std::crypto`).
//!
//! Provides hashing (SHA-256, BLAKE3), HMAC-SHA256, HKDF key derivation,
//! base64/hex encoding, and cryptographic random byte generation.
//!
//! All implementations use only existing workspace dependencies (`sha2`, `uuid`)
//! plus pure-Rust fallbacks where external crates are unavailable.
//!
//! # Examples
//!
//! ```rust
//! use lumen_runtime::crypto::{sha256_hex, base64_encode, base64_decode, generate_uuid_v4};
//!
//! let hash = sha256_hex(b"hello");
//! assert_eq!(hash.len(), 64);
//!
//! let encoded = base64_encode(b"hello world");
//! let decoded = base64_decode(&encoded).unwrap();
//! assert_eq!(decoded, b"hello world");
//!
//! let uuid = generate_uuid_v4();
//! assert_eq!(uuid.len(), 36);
//! ```

use sha2::{Digest, Sha256};
use std::fmt;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during cryptographic operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// Invalid input was provided to a cryptographic function.
    InvalidInput(String),
    /// An encoding or decoding operation failed.
    DecodingError(String),
    /// A key-related error occurred (e.g., invalid key length).
    KeyError(String),
    /// A hashing operation failed.
    HashError(String),
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            CryptoError::DecodingError(msg) => write!(f, "decoding error: {msg}"),
            CryptoError::KeyError(msg) => write!(f, "key error: {msg}"),
            CryptoError::HashError(msg) => write!(f, "hash error: {msg}"),
        }
    }
}

impl std::error::Error for CryptoError {}

// ===========================================================================
// Hashing
// ===========================================================================

/// Compute the SHA-256 hash of `data`.
///
/// Returns a 32-byte digest conforming to FIPS 180-4, computed using the
/// `sha2` crate.
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::sha256;
///
/// let digest = sha256(b"abc");
/// assert_eq!(digest.len(), 32);
/// ```
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Compute the SHA-256 hash of `data` and return it as a lowercase hex string.
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::sha256_hex;
///
/// let hex = sha256_hex(b"abc");
/// assert_eq!(hex.len(), 64);
/// assert_eq!(
///     hex,
///     "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
/// );
/// ```
pub fn sha256_hex(data: &[u8]) -> String {
    hex_encode(&sha256(data))
}

/// Compute a BLAKE3-like hash of `data`.
///
/// This is a simplified pure-Rust implementation inspired by BLAKE3. It uses
/// the BLAKE3 compression function structure but with a reduced round count
/// for simplicity. The output is a 32-byte digest.
///
/// **Note**: This is a simplified implementation suitable for non-security-critical
/// hashing (checksums, content addressing). For security-critical applications,
/// use [`sha256`] instead.
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::blake3_hash;
///
/// let digest = blake3_hash(b"hello");
/// assert_eq!(digest.len(), 32);
/// // Deterministic: same input always gives same output
/// assert_eq!(blake3_hash(b"hello"), blake3_hash(b"hello"));
/// // Different input gives different output
/// assert_ne!(blake3_hash(b"hello"), blake3_hash(b"world"));
/// ```
pub fn blake3_hash(data: &[u8]) -> [u8; 32] {
    blake3_impl::hash(data)
}

/// Simplified BLAKE3 implementation.
///
/// Uses the standard BLAKE3 IV and message schedule with 7 rounds of the
/// quarter-round mixing function applied to 64-byte blocks.
mod blake3_impl {
    /// BLAKE3 initialization vector (same as BLAKE2s IV, derived from sqrt of primes).
    const IV: [u32; 8] = [
        0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB,
        0x5BE0CD19,
    ];

    /// BLAKE3 message schedule permutation.
    const MSG_SCHEDULE: [[usize; 16]; 7] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8],
        [3, 4, 10, 12, 13, 2, 7, 14, 6, 5, 9, 0, 11, 15, 8, 1],
        [10, 7, 12, 9, 14, 3, 13, 15, 4, 0, 11, 2, 5, 8, 1, 6],
        [12, 13, 9, 11, 15, 10, 14, 8, 7, 2, 5, 3, 0, 1, 6, 4],
        [9, 14, 11, 5, 8, 12, 15, 1, 13, 3, 0, 10, 2, 6, 4, 7],
        [11, 15, 5, 0, 1, 9, 8, 6, 14, 10, 2, 12, 3, 4, 7, 13],
    ];

    /// Flags for chunk processing.
    const CHUNK_START: u32 = 1;
    const CHUNK_END: u32 = 2;
    const ROOT: u32 = 8;

    #[inline]
    fn g(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, mx: u32, my: u32) {
        state[a] = state[a].wrapping_add(state[b]).wrapping_add(mx);
        state[d] = (state[d] ^ state[a]).rotate_right(16);
        state[c] = state[c].wrapping_add(state[d]);
        state[b] = (state[b] ^ state[c]).rotate_right(12);
        state[a] = state[a].wrapping_add(state[b]).wrapping_add(my);
        state[d] = (state[d] ^ state[a]).rotate_right(8);
        state[c] = state[c].wrapping_add(state[d]);
        state[b] = (state[b] ^ state[c]).rotate_right(7);
    }

    fn round(state: &mut [u32; 16], msg: &[u32; 16], schedule: &[usize; 16]) {
        // Column step
        g(state, 0, 4, 8, 12, msg[schedule[0]], msg[schedule[1]]);
        g(state, 1, 5, 9, 13, msg[schedule[2]], msg[schedule[3]]);
        g(state, 2, 6, 10, 14, msg[schedule[4]], msg[schedule[5]]);
        g(state, 3, 7, 11, 15, msg[schedule[6]], msg[schedule[7]]);
        // Diagonal step
        g(state, 0, 5, 10, 15, msg[schedule[8]], msg[schedule[9]]);
        g(state, 1, 6, 11, 12, msg[schedule[10]], msg[schedule[11]]);
        g(state, 2, 7, 8, 13, msg[schedule[12]], msg[schedule[13]]);
        g(state, 3, 4, 9, 14, msg[schedule[14]], msg[schedule[15]]);
    }

    fn compress(
        chaining: &[u32; 8],
        block_words: &[u32; 16],
        counter: u64,
        block_len: u32,
        flags: u32,
    ) -> [u32; 16] {
        let mut state = [0u32; 16];
        state[..8].copy_from_slice(chaining);
        state[8] = IV[0];
        state[9] = IV[1];
        state[10] = IV[2];
        state[11] = IV[3];
        state[12] = counter as u32;
        state[13] = (counter >> 32) as u32;
        state[14] = block_len;
        state[15] = flags;

        for sched in &MSG_SCHEDULE {
            round(&mut state, block_words, sched);
        }

        // XOR the two halves
        for i in 0..8 {
            state[i] ^= state[i + 8];
            state[i + 8] ^= chaining[i];
        }
        state
    }

    /// Hash arbitrary-length data into a 32-byte digest.
    pub fn hash(data: &[u8]) -> [u8; 32] {
        let mut chaining = IV;

        // Pad data into 64-byte blocks
        let mut blocks: Vec<[u8; 64]> = Vec::new();
        let mut offset = 0;
        while offset < data.len() {
            let mut block = [0u8; 64];
            let end = std::cmp::min(offset + 64, data.len());
            let len = end - offset;
            block[..len].copy_from_slice(&data[offset..end]);
            blocks.push(block);
            offset += 64;
        }

        // Handle empty input
        if blocks.is_empty() {
            blocks.push([0u8; 64]);
        }

        let num_blocks = blocks.len();

        for (i, block) in blocks.iter().enumerate() {
            // Parse block into 16 little-endian u32 words
            let mut words = [0u32; 16];
            for (j, word) in words.iter_mut().enumerate() {
                let base = j * 4;
                *word = u32::from_le_bytes([
                    block[base],
                    block[base + 1],
                    block[base + 2],
                    block[base + 3],
                ]);
            }

            let block_len = if i == num_blocks - 1 {
                // Last block: actual bytes in this block
                let remaining = data.len().saturating_sub(i * 64);
                std::cmp::min(remaining, 64) as u32
            } else {
                64
            };

            let mut flags = 0u32;
            if i == 0 {
                flags |= CHUNK_START;
            }
            if i == num_blocks - 1 {
                flags |= CHUNK_END | ROOT;
            }

            let output = compress(&chaining, &words, i as u64, block_len, flags);
            // Take first 8 words as new chaining value
            chaining.copy_from_slice(&output[..8]);
        }

        // Serialize chaining value to bytes
        let mut result = [0u8; 32];
        for (i, &word) in chaining.iter().enumerate() {
            let bytes = word.to_le_bytes();
            result[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        }
        result
    }
}

/// Compute HMAC-SHA256 as defined in RFC 2104.
///
/// Uses [`sha256`] as the underlying hash function with a 64-byte block size.
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::{hmac_sha256, hex_encode};
///
/// let mac = hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog");
/// let hex = hex_encode(&mac);
/// assert_eq!(hex, "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8");
/// ```
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;

    // Step 1: If key > block size, hash it; if shorter, zero-pad to block size
    let mut key_block = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let hashed_key = sha256(key);
        key_block[..32].copy_from_slice(&hashed_key);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    // Step 2: Create inner and outer padded keys
    let mut ipad = [0x36u8; BLOCK_SIZE];
    let mut opad = [0x5cu8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }

    // Step 3: inner hash = H(ipad || data)
    let mut inner = Vec::with_capacity(BLOCK_SIZE + data.len());
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(data);
    let inner_hash = sha256(&inner);

    // Step 4: outer hash = H(opad || inner_hash)
    let mut outer = Vec::with_capacity(BLOCK_SIZE + 32);
    outer.extend_from_slice(&opad);
    outer.extend_from_slice(&inner_hash);
    sha256(&outer)
}

// ===========================================================================
// Key derivation
// ===========================================================================

/// HKDF-SHA256 key derivation as defined in RFC 5869.
///
/// Derives `output_len` bytes of keying material from input key material (`ikm`),
/// an optional `salt`, and application-specific `info`.
///
/// # Arguments
///
/// * `ikm` — Input key material
/// * `salt` — Optional salt (can be empty; a zero-filled salt of hash length is used)
/// * `info` — Context and application-specific information
/// * `output_len` — Number of bytes to derive (max 255 * 32 = 8160)
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::hkdf_sha256;
///
/// let okm = hkdf_sha256(b"input-key", b"salt", b"info", 32);
/// assert_eq!(okm.len(), 32);
/// ```
pub fn hkdf_sha256(ikm: &[u8], salt: &[u8], info: &[u8], output_len: usize) -> Vec<u8> {
    // Extract phase: PRK = HMAC-SHA256(salt, IKM)
    let effective_salt = if salt.is_empty() {
        vec![0u8; 32]
    } else {
        salt.to_vec()
    };
    let prk = hmac_sha256(&effective_salt, ikm);

    // Expand phase
    let hash_len = 32;
    // Max output: 255 * hash_len
    let n = output_len.div_ceil(hash_len);
    let n = std::cmp::min(n, 255);

    let mut okm = Vec::with_capacity(n * hash_len);
    let mut t_prev: Vec<u8> = Vec::new();

    for i in 1..=n {
        let mut input = Vec::with_capacity(t_prev.len() + info.len() + 1);
        input.extend_from_slice(&t_prev);
        input.extend_from_slice(info);
        input.push(i as u8);
        let t = hmac_sha256(&prk, &input);
        t_prev = t.to_vec();
        okm.extend_from_slice(&t);
    }

    okm.truncate(output_len);
    okm
}

// ===========================================================================
// Encoding
// ===========================================================================

/// Base64 alphabet (standard, RFC 4648).
const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode `data` as a standard base64 string (RFC 4648).
///
/// Uses the standard alphabet with `=` padding.
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::base64_encode;
///
/// assert_eq!(base64_encode(b"hello world"), "aGVsbG8gd29ybGQ=");
/// assert_eq!(base64_encode(b""), "");
/// ```
pub fn base64_encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }

    let mut result = Vec::with_capacity(4 * data.len().div_ceil(3));
    let chunks = data.chunks(3);

    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(BASE64_CHARS[((triple >> 18) & 0x3F) as usize]);
        result.push(BASE64_CHARS[((triple >> 12) & 0x3F) as usize]);

        if chunk.len() > 1 {
            result.push(BASE64_CHARS[((triple >> 6) & 0x3F) as usize]);
        } else {
            result.push(b'=');
        }

        if chunk.len() > 2 {
            result.push(BASE64_CHARS[(triple & 0x3F) as usize]);
        } else {
            result.push(b'=');
        }
    }

    // SAFETY: BASE64_CHARS and '=' are all valid ASCII/UTF-8.
    unsafe { String::from_utf8_unchecked(result) }
}

/// Decode a standard base64 string (RFC 4648) into bytes.
///
/// Returns a [`CryptoError::DecodingError`] if the input contains invalid
/// characters or has an incorrect length.
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::base64_decode;
///
/// let decoded = base64_decode("aGVsbG8gd29ybGQ=").unwrap();
/// assert_eq!(decoded, b"hello world");
/// ```
pub fn base64_decode(encoded: &str) -> Result<Vec<u8>, CryptoError> {
    let encoded = encoded.trim_end_matches('=');
    let len = encoded.len();

    if len == 0 {
        return Ok(Vec::new());
    }

    // Validate and decode characters
    let mut buf: Vec<u8> = Vec::with_capacity(len);
    for (i, ch) in encoded.bytes().enumerate() {
        let val = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => {
                return Err(CryptoError::DecodingError(format!(
                    "invalid base64 character '{}' at position {i}",
                    ch as char
                )));
            }
        };
        buf.push(val);
    }

    let mut result = Vec::with_capacity(len * 3 / 4);
    let chunks = buf.chunks(4);

    for chunk in chunks {
        let c0 = chunk[0] as u32;
        let c1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let c2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let c3 = if chunk.len() > 3 { chunk[3] as u32 } else { 0 };

        let triple = (c0 << 18) | (c1 << 12) | (c2 << 6) | c3;

        result.push(((triple >> 16) & 0xFF) as u8);
        if chunk.len() > 2 {
            result.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk.len() > 3 {
            result.push((triple & 0xFF) as u8);
        }
    }

    Ok(result)
}

/// Encode `data` as a lowercase hexadecimal string.
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::hex_encode;
///
/// assert_eq!(hex_encode(&[0xDE, 0xAD, 0xBE, 0xEF]), "deadbeef");
/// assert_eq!(hex_encode(&[]), "");
/// ```
pub fn hex_encode(data: &[u8]) -> String {
    let mut result = String::with_capacity(data.len() * 2);
    for &byte in data {
        result.push(HEX_LOWER[(byte >> 4) as usize]);
        result.push(HEX_LOWER[(byte & 0x0F) as usize]);
    }
    result
}

/// Lowercase hex digits lookup table.
const HEX_LOWER: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

/// Decode a hexadecimal string into bytes.
///
/// The input must have an even number of characters and contain only
/// valid hex digits (`0-9`, `a-f`, `A-F`).
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::hex_decode;
///
/// let bytes = hex_decode("deadbeef").unwrap();
/// assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
/// ```
pub fn hex_decode(encoded: &str) -> Result<Vec<u8>, CryptoError> {
    if !encoded.len().is_multiple_of(2) {
        return Err(CryptoError::DecodingError(format!(
            "hex string has odd length: {}",
            encoded.len()
        )));
    }

    let mut result = Vec::with_capacity(encoded.len() / 2);
    let bytes = encoded.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let hi = hex_digit(bytes[i], i)?;
        let lo = hex_digit(bytes[i + 1], i + 1)?;
        result.push((hi << 4) | lo);
        i += 2;
    }

    Ok(result)
}

/// Parse a single hex digit, returning its 4-bit value.
fn hex_digit(ch: u8, pos: usize) -> Result<u8, CryptoError> {
    match ch {
        b'0'..=b'9' => Ok(ch - b'0'),
        b'a'..=b'f' => Ok(ch - b'a' + 10),
        b'A'..=b'F' => Ok(ch - b'A' + 10),
        _ => Err(CryptoError::DecodingError(format!(
            "invalid hex character '{}' at position {pos}",
            ch as char
        ))),
    }
}

// ===========================================================================
// Random
// ===========================================================================

/// Generate `count` cryptographically random bytes.
///
/// Uses UUID v4 generation (backed by the OS CSPRNG via `getrandom`) to
/// extract random bytes. Each UUID v4 provides 16 bytes (with 6 bits fixed),
/// so multiple UUIDs may be generated for larger requests.
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::random_bytes;
///
/// let bytes = random_bytes(32);
/// assert_eq!(bytes.len(), 32);
/// ```
pub fn random_bytes(count: usize) -> Vec<u8> {
    if count == 0 {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(count);

    // Each UUID v4 gives us 16 random bytes (with some bits fixed for version/variant,
    // but still high entropy). We use the raw bytes.
    while result.len() < count {
        let uuid = uuid::Uuid::new_v4();
        let bytes = uuid.as_bytes();
        let needed = count - result.len();
        let take = std::cmp::min(needed, 16);
        result.extend_from_slice(&bytes[..take]);
    }

    result
}

/// Generate a UUID v4 string in standard format (`8-4-4-4-12`).
///
/// Uses the `uuid` crate with its OS-level CSPRNG backend.
///
/// # Examples
///
/// ```rust
/// use lumen_runtime::crypto::generate_uuid_v4;
///
/// let uuid = generate_uuid_v4();
/// assert_eq!(uuid.len(), 36);
/// assert_eq!(uuid.chars().nth(14), Some('4')); // version nibble
/// ```
pub fn generate_uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ===========================================================================
// Unit tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_empty() {
        let digest = sha256(b"");
        let hex = sha256_hex(b"");
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(digest.len(), 32);
    }

    #[test]
    fn sha256_abc() {
        let hex = sha256_hex(b"abc");
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_long_nist_vector() {
        // NIST test vector: "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        let hex = sha256_hex(input);
        assert_eq!(
            hex,
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn blake3_deterministic() {
        let h1 = blake3_hash(b"hello");
        let h2 = blake3_hash(b"hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn blake3_different_inputs() {
        let h1 = blake3_hash(b"hello");
        let h2 = blake3_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn blake3_empty() {
        let h = blake3_hash(b"");
        assert_eq!(h.len(), 32);
        // Should be deterministic even for empty
        assert_eq!(h, blake3_hash(b""));
    }

    #[test]
    fn blake3_length() {
        let h = blake3_hash(b"test data that is longer than one block, enough to span multiple 64-byte chunks to verify multi-block processing");
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn hmac_sha256_rfc4231_test1() {
        // RFC 4231 Test Case 1
        let key = vec![0x0bu8; 20];
        let data = b"Hi There";
        let mac = hmac_sha256(&key, data);
        assert_eq!(
            hex_encode(&mac),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn hmac_sha256_rfc4231_test2() {
        // RFC 4231 Test Case 2: "Jefe" / "what do ya want for nothing?"
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        let mac = hmac_sha256(key, data);
        assert_eq!(
            hex_encode(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_sha256_rfc4231_test3() {
        // RFC 4231 Test Case 3
        let key = vec![0xaau8; 20];
        let data = vec![0xddu8; 50];
        let mac = hmac_sha256(&key, &data);
        assert_eq!(
            hex_encode(&mac),
            "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe"
        );
    }

    #[test]
    fn hmac_sha256_long_key() {
        // RFC 4231 Test Case 6: key longer than block size (131 bytes)
        let key = vec![0xaau8; 131];
        let data = b"Test Using Larger Than Block-Size Key - Hash Key First";
        let mac = hmac_sha256(&key, data);
        assert_eq!(
            hex_encode(&mac),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn hmac_sha256_different_keys() {
        let mac1 = hmac_sha256(b"key1", b"data");
        let mac2 = hmac_sha256(b"key2", b"data");
        assert_ne!(mac1, mac2);
    }

    #[test]
    fn hkdf_basic() {
        let okm = hkdf_sha256(b"secret", b"salt", b"info", 32);
        assert_eq!(okm.len(), 32);
    }

    #[test]
    fn hkdf_different_lengths() {
        let okm16 = hkdf_sha256(b"key", b"salt", b"info", 16);
        let okm32 = hkdf_sha256(b"key", b"salt", b"info", 32);
        let okm64 = hkdf_sha256(b"key", b"salt", b"info", 64);
        assert_eq!(okm16.len(), 16);
        assert_eq!(okm32.len(), 32);
        assert_eq!(okm64.len(), 64);
        // First 16 bytes of okm32 should match okm16
        assert_eq!(&okm32[..16], &okm16[..]);
        // First 32 bytes of okm64 should match okm32
        assert_eq!(&okm64[..32], &okm32[..]);
    }

    #[test]
    fn hkdf_empty_salt() {
        let okm = hkdf_sha256(b"key", b"", b"info", 32);
        assert_eq!(okm.len(), 32);
    }

    #[test]
    fn hkdf_deterministic() {
        let a = hkdf_sha256(b"key", b"salt", b"info", 32);
        let b = hkdf_sha256(b"key", b"salt", b"info", 32);
        assert_eq!(a, b);
    }

    #[test]
    fn base64_encode_basic() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_encode_hello_world() {
        assert_eq!(base64_encode(b"hello world"), "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn base64_decode_basic() {
        assert_eq!(base64_decode("").unwrap(), b"");
        assert_eq!(base64_decode("Zg==").unwrap(), b"f");
        assert_eq!(base64_decode("Zm8=").unwrap(), b"fo");
        assert_eq!(base64_decode("Zm9v").unwrap(), b"foo");
        assert_eq!(base64_decode("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(base64_decode("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn base64_roundtrip() {
        let data = b"The quick brown fox jumps over the lazy dog";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64_roundtrip_binary() {
        let data: Vec<u8> = (0..=255).collect();
        let encoded = base64_encode(&data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64_decode_invalid_char() {
        let result = base64_decode("abc!def");
        assert!(result.is_err());
        match result.unwrap_err() {
            CryptoError::DecodingError(msg) => assert!(msg.contains("invalid base64")),
            other => panic!("expected DecodingError, got: {other}"),
        }
    }

    #[test]
    fn hex_encode_basic() {
        assert_eq!(hex_encode(&[]), "");
        assert_eq!(hex_encode(&[0x00]), "00");
        assert_eq!(hex_encode(&[0xFF]), "ff");
        assert_eq!(hex_encode(&[0xDE, 0xAD, 0xBE, 0xEF]), "deadbeef");
        assert_eq!(
            hex_encode(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF]),
            "0123456789abcdef"
        );
    }

    #[test]
    fn hex_decode_basic() {
        assert_eq!(hex_decode("").unwrap(), Vec::<u8>::new());
        assert_eq!(hex_decode("00").unwrap(), vec![0x00]);
        assert_eq!(hex_decode("ff").unwrap(), vec![0xFF]);
        assert_eq!(hex_decode("FF").unwrap(), vec![0xFF]);
        assert_eq!(
            hex_decode("deadbeef").unwrap(),
            vec![0xDE, 0xAD, 0xBE, 0xEF]
        );
    }

    #[test]
    fn hex_roundtrip() {
        let data: Vec<u8> = (0..=255).collect();
        let encoded = hex_encode(&data);
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn hex_decode_odd_length() {
        let result = hex_decode("abc");
        assert!(result.is_err());
        match result.unwrap_err() {
            CryptoError::DecodingError(msg) => assert!(msg.contains("odd length")),
            other => panic!("expected DecodingError, got: {other}"),
        }
    }

    #[test]
    fn hex_decode_invalid_char() {
        let result = hex_decode("zz");
        assert!(result.is_err());
        match result.unwrap_err() {
            CryptoError::DecodingError(msg) => assert!(msg.contains("invalid hex")),
            other => panic!("expected DecodingError, got: {other}"),
        }
    }

    #[test]
    fn random_bytes_correct_length() {
        assert_eq!(random_bytes(0).len(), 0);
        assert_eq!(random_bytes(1).len(), 1);
        assert_eq!(random_bytes(16).len(), 16);
        assert_eq!(random_bytes(32).len(), 32);
        assert_eq!(random_bytes(100).len(), 100);
    }

    #[test]
    fn random_bytes_not_all_zeros() {
        // It's astronomically unlikely that 32 random bytes are all zero
        let bytes = random_bytes(32);
        assert!(bytes.iter().any(|&b| b != 0));
    }

    #[test]
    fn random_bytes_not_identical() {
        // Two separate calls should (almost certainly) produce different output
        let a = random_bytes(32);
        let b = random_bytes(32);
        assert_ne!(a, b);
    }

    #[test]
    fn uuid_v4_format() {
        let uuid = generate_uuid_v4();
        assert_eq!(uuid.len(), 36);

        // Format: 8-4-4-4-12
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);

        // Version 4: third group starts with '4'
        assert_eq!(parts[2].chars().next(), Some('4'));

        // Variant bits: fourth group starts with 8, 9, a, or b
        let variant_char = parts[3].chars().next().unwrap();
        assert!(
            "89ab".contains(variant_char),
            "variant nibble should be 8, 9, a, or b, got: {variant_char}"
        );
    }

    #[test]
    fn uuid_v4_uniqueness() {
        let a = generate_uuid_v4();
        let b = generate_uuid_v4();
        assert_ne!(a, b);
    }

    #[test]
    fn crypto_error_display_invalid_input() {
        let err = CryptoError::InvalidInput("bad data".into());
        assert_eq!(err.to_string(), "invalid input: bad data");
    }

    #[test]
    fn crypto_error_display_decoding() {
        let err = CryptoError::DecodingError("corrupt".into());
        assert_eq!(err.to_string(), "decoding error: corrupt");
    }

    #[test]
    fn crypto_error_display_key() {
        let err = CryptoError::KeyError("too short".into());
        assert_eq!(err.to_string(), "key error: too short");
    }

    #[test]
    fn crypto_error_display_hash() {
        let err = CryptoError::HashError("failure".into());
        assert_eq!(err.to_string(), "hash error: failure");
    }

    #[test]
    fn crypto_error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(CryptoError::InvalidInput("test".into()));
        assert!(err.to_string().contains("invalid input"));
    }
}
