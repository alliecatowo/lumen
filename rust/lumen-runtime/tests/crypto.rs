//! Comprehensive tests for `lumen_runtime::crypto` (T126: std::crypto).
//!
//! Tests SHA-256 against NIST vectors, HMAC-SHA256 against RFC 4231,
//! HKDF key derivation, base64/hex encoding round-trips, random byte
//! generation, UUID v4 format, and error types.

use lumen_runtime::crypto::*;

// ===========================================================================
// SHA-256 — NIST FIPS 180-4 test vectors
// ===========================================================================

#[test]
fn crypto_sha256_empty_string() {
    let digest = sha256(b"");
    let hex = sha256_hex(b"");
    assert_eq!(
        hex,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(digest.len(), 32);
}

#[test]
fn crypto_sha256_abc() {
    let hex = sha256_hex(b"abc");
    assert_eq!(
        hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn crypto_sha256_nist_448bit() {
    // NIST test vector: 448-bit message
    let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    let hex = sha256_hex(input);
    assert_eq!(
        hex,
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn crypto_sha256_hex_format() {
    let hex = sha256_hex(b"test");
    // Must be 64 lowercase hex characters
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(hex, hex.to_lowercase());
}

#[test]
fn crypto_sha256_deterministic() {
    let a = sha256(b"deterministic");
    let b = sha256(b"deterministic");
    assert_eq!(a, b);
}

// ===========================================================================
// BLAKE3
// ===========================================================================

#[test]
fn crypto_blake3_deterministic() {
    let h1 = blake3_hash(b"hello world");
    let h2 = blake3_hash(b"hello world");
    assert_eq!(h1, h2);
}

#[test]
fn crypto_blake3_different_inputs() {
    let h1 = blake3_hash(b"foo");
    let h2 = blake3_hash(b"bar");
    assert_ne!(h1, h2);
}

#[test]
fn crypto_blake3_empty() {
    let h = blake3_hash(b"");
    assert_eq!(h.len(), 32);
    // deterministic for empty input
    assert_eq!(h, blake3_hash(b""));
}

#[test]
fn crypto_blake3_output_length() {
    // Input spanning multiple 64-byte blocks
    let long_input = vec![0xABu8; 200];
    let h = blake3_hash(&long_input);
    assert_eq!(h.len(), 32);
}

// ===========================================================================
// HMAC-SHA256 — RFC 4231 test vectors
// ===========================================================================

#[test]
fn crypto_hmac_rfc4231_test1() {
    // Test Case 1: key = 0x0b * 20, data = "Hi There"
    let key = vec![0x0bu8; 20];
    let mac = hmac_sha256(&key, b"Hi There");
    assert_eq!(
        hex_encode(&mac),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
}

#[test]
fn crypto_hmac_rfc4231_test2() {
    // Test Case 2: key = "Jefe", data = "what do ya want for nothing?"
    let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
    assert_eq!(
        hex_encode(&mac),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
}

#[test]
fn crypto_hmac_rfc4231_test3() {
    // Test Case 3: key = 0xaa * 20, data = 0xdd * 50
    let key = vec![0xaau8; 20];
    let data = vec![0xddu8; 50];
    let mac = hmac_sha256(&key, &data);
    assert_eq!(
        hex_encode(&mac),
        "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe"
    );
}

#[test]
fn crypto_hmac_rfc4231_test6_long_key() {
    // Test Case 6: key longer than block size (131 bytes of 0xaa)
    let key = vec![0xaau8; 131];
    let data = b"Test Using Larger Than Block-Size Key - Hash Key First";
    let mac = hmac_sha256(&key, data);
    assert_eq!(
        hex_encode(&mac),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

#[test]
fn crypto_hmac_different_keys_differ() {
    let m1 = hmac_sha256(b"key-alpha", b"message");
    let m2 = hmac_sha256(b"key-bravo", b"message");
    assert_ne!(m1, m2);
}

#[test]
fn crypto_hmac_different_data_differ() {
    let m1 = hmac_sha256(b"key", b"message-alpha");
    let m2 = hmac_sha256(b"key", b"message-bravo");
    assert_ne!(m1, m2);
}

// ===========================================================================
// HKDF-SHA256
// ===========================================================================

#[test]
fn crypto_hkdf_basic_derivation() {
    let okm = hkdf_sha256(b"secret-key", b"salt-value", b"context-info", 32);
    assert_eq!(okm.len(), 32);
}

#[test]
fn crypto_hkdf_output_lengths() {
    for len in [1, 16, 32, 48, 64, 128] {
        let okm = hkdf_sha256(b"key", b"salt", b"info", len);
        assert_eq!(okm.len(), len, "HKDF output length mismatch for {len}");
    }
}

#[test]
fn crypto_hkdf_prefix_consistency() {
    // Shorter output should be a prefix of longer output
    let short = hkdf_sha256(b"key", b"salt", b"info", 16);
    let long = hkdf_sha256(b"key", b"salt", b"info", 64);
    assert_eq!(&long[..16], &short[..]);
}

#[test]
fn crypto_hkdf_empty_salt() {
    let okm = hkdf_sha256(b"key", b"", b"info", 32);
    assert_eq!(okm.len(), 32);
}

#[test]
fn crypto_hkdf_deterministic() {
    let a = hkdf_sha256(b"key", b"salt", b"info", 32);
    let b = hkdf_sha256(b"key", b"salt", b"info", 32);
    assert_eq!(a, b);
}

#[test]
fn crypto_hkdf_different_info_differ() {
    let a = hkdf_sha256(b"key", b"salt", b"info-a", 32);
    let b = hkdf_sha256(b"key", b"salt", b"info-b", 32);
    assert_ne!(a, b);
}

// ===========================================================================
// Base64 encoding/decoding
// ===========================================================================

#[test]
fn crypto_base64_encode_rfc4648_vectors() {
    // RFC 4648 §10 test vectors
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"f"), "Zg==");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert_eq!(base64_encode(b"foo"), "Zm9v");
    assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
    assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
}

#[test]
fn crypto_base64_decode_rfc4648_vectors() {
    assert_eq!(base64_decode("").unwrap(), b"");
    assert_eq!(base64_decode("Zg==").unwrap(), b"f");
    assert_eq!(base64_decode("Zm8=").unwrap(), b"fo");
    assert_eq!(base64_decode("Zm9v").unwrap(), b"foo");
    assert_eq!(base64_decode("Zm9vYg==").unwrap(), b"foob");
    assert_eq!(base64_decode("Zm9vYmE=").unwrap(), b"fooba");
    assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
}

#[test]
fn crypto_base64_roundtrip() {
    let inputs: &[&[u8]] = &[
        b"",
        b"a",
        b"ab",
        b"abc",
        b"The quick brown fox",
        &[0u8; 100],
        &(0..=255).collect::<Vec<u8>>(),
    ];
    for &input in inputs {
        let encoded = base64_encode(input);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, input);
    }
}

#[test]
fn crypto_base64_decode_rejects_invalid() {
    assert!(base64_decode("abc!def").is_err());
    assert!(base64_decode("hello world").is_err()); // space is invalid
    assert!(base64_decode("$$$$").is_err());
}

// ===========================================================================
// Hex encoding/decoding
// ===========================================================================

#[test]
fn crypto_hex_encode_basic() {
    assert_eq!(hex_encode(&[]), "");
    assert_eq!(hex_encode(&[0xDE, 0xAD, 0xBE, 0xEF]), "deadbeef");
    assert_eq!(hex_encode(&[0x00, 0xFF]), "00ff");
}

#[test]
fn crypto_hex_decode_basic() {
    assert_eq!(hex_decode("").unwrap(), Vec::<u8>::new());
    assert_eq!(
        hex_decode("deadbeef").unwrap(),
        vec![0xDE, 0xAD, 0xBE, 0xEF]
    );
    assert_eq!(
        hex_decode("DEADBEEF").unwrap(),
        vec![0xDE, 0xAD, 0xBE, 0xEF]
    );
    assert_eq!(hex_decode("00ff").unwrap(), vec![0x00, 0xFF]);
}

#[test]
fn crypto_hex_roundtrip() {
    let data: Vec<u8> = (0..=255).collect();
    let encoded = hex_encode(&data);
    let decoded = hex_decode(&encoded).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn crypto_hex_decode_rejects_odd_length() {
    let err = hex_decode("abc").unwrap_err();
    match err {
        CryptoError::DecodingError(msg) => assert!(msg.contains("odd length")),
        other => panic!("expected DecodingError, got: {other}"),
    }
}

#[test]
fn crypto_hex_decode_rejects_invalid_chars() {
    let err = hex_decode("zz").unwrap_err();
    match err {
        CryptoError::DecodingError(msg) => assert!(msg.contains("invalid hex")),
        other => panic!("expected DecodingError, got: {other}"),
    }
}

// ===========================================================================
// Random bytes
// ===========================================================================

#[test]
fn crypto_random_bytes_length() {
    for len in [0, 1, 16, 32, 64, 128, 256] {
        let bytes = random_bytes(len);
        assert_eq!(
            bytes.len(),
            len,
            "random_bytes({len}) returned wrong length"
        );
    }
}

#[test]
fn crypto_random_bytes_not_all_zeros() {
    let bytes = random_bytes(32);
    assert!(
        bytes.iter().any(|&b| b != 0),
        "32 random bytes were all zero"
    );
}

#[test]
fn crypto_random_bytes_different_calls() {
    let a = random_bytes(32);
    let b = random_bytes(32);
    assert_ne!(a, b, "two random_bytes(32) calls returned identical output");
}

// ===========================================================================
// UUID v4
// ===========================================================================

#[test]
fn crypto_uuid_v4_format() {
    let uuid = generate_uuid_v4();
    assert_eq!(uuid.len(), 36, "UUID length should be 36");

    let parts: Vec<&str> = uuid.split('-').collect();
    assert_eq!(parts.len(), 5, "UUID should have 5 dash-separated parts");
    assert_eq!(parts[0].len(), 8);
    assert_eq!(parts[1].len(), 4);
    assert_eq!(parts[2].len(), 4);
    assert_eq!(parts[3].len(), 4);
    assert_eq!(parts[4].len(), 12);
}

#[test]
fn crypto_uuid_v4_version_and_variant() {
    let uuid = generate_uuid_v4();
    let parts: Vec<&str> = uuid.split('-').collect();

    // Version nibble: first char of third group must be '4'
    assert_eq!(
        parts[2].chars().next(),
        Some('4'),
        "UUID version nibble must be 4"
    );

    // Variant nibble: first char of fourth group must be 8, 9, a, or b
    let variant = parts[3].chars().next().unwrap();
    assert!(
        "89ab".contains(variant),
        "UUID variant nibble must be 8/9/a/b, got: {variant}"
    );
}

#[test]
fn crypto_uuid_v4_uniqueness() {
    let uuids: Vec<String> = (0..10).map(|_| generate_uuid_v4()).collect();
    for i in 0..uuids.len() {
        for j in (i + 1)..uuids.len() {
            assert_ne!(uuids[i], uuids[j], "UUIDs at index {i} and {j} collided");
        }
    }
}

// ===========================================================================
// CryptoError Display
// ===========================================================================

#[test]
fn crypto_error_display_all_variants() {
    let cases: Vec<(CryptoError, &str)> = vec![
        (
            CryptoError::InvalidInput("bad".into()),
            "invalid input: bad",
        ),
        (
            CryptoError::DecodingError("corrupt".into()),
            "decoding error: corrupt",
        ),
        (CryptoError::KeyError("short".into()), "key error: short"),
        (CryptoError::HashError("fail".into()), "hash error: fail"),
    ];
    for (err, expected) in cases {
        assert_eq!(err.to_string(), expected);
    }
}

#[test]
fn crypto_error_implements_std_error() {
    let err: Box<dyn std::error::Error> = Box::new(CryptoError::InvalidInput("test".into()));
    assert!(err.to_string().contains("invalid input"));
}

#[test]
fn crypto_error_equality() {
    let a = CryptoError::InvalidInput("x".into());
    let b = CryptoError::InvalidInput("x".into());
    let c = CryptoError::InvalidInput("y".into());
    assert_eq!(a, b);
    assert_ne!(a, c);
}
