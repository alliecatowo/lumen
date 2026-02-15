/**
 * WebCrypto wrappers for Wares Registry
 */

export const ALGORITHM_NAME = 'ECDSA';
export const NAMED_CURVE = 'P-256';
export const HASH_ALGORITHM = 'SHA-256';

/**
 * Import a PEM-encoded private key (PKCS#8).
 */
export async function importPrivateKey(pem: string): Promise<CryptoKey> {
    // Strip PEM headers/footers and newlines
    const b64 = pem
        .replace(/-----BEGIN PRIVATE KEY-----/, '')
        .replace(/-----END PRIVATE KEY-----/, '')
        .replace(/-----BEGIN EC PRIVATE KEY-----/, '') // Handle EC format if needed
        .replace(/-----END EC PRIVATE KEY-----/, '')
        .replace(/\s+/g, '');

    const binary = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));

    return crypto.subtle.importKey(
        'pkcs8',
        binary,
        {
            name: ALGORITHM_NAME,
            namedCurve: NAMED_CURVE,
        },
        false, // not extractable
        ['sign']
    );
}

/**
 * Sign data using the private key.
 */
export async function signData(
    privateKey: CryptoKey,
    data: string | Uint8Array
): Promise<string> {
    const encoder = new TextEncoder();
    const dataBytes = typeof data === 'string' ? encoder.encode(data) : data;

    const signature = await crypto.subtle.sign(
        {
            name: ALGORITHM_NAME,
            hash: HASH_ALGORITHM,
        },
        privateKey,
        dataBytes as any
    );

    // Return base64 encoded signature
    return btoa(String.fromCharCode(...new Uint8Array(signature)));
}

/**
 * Generate a random hex string.
 */
export function generateRandomHex(length: number): string {
    const bytes = new Uint8Array(length);
    crypto.getRandomValues(bytes);
    return Array.from(bytes)
        .map((b) => b.toString(16).padStart(2, '0'))
        .join('');
}

/**
 * Generate a UUID v4.
 */
export function generateUuid(): string {
    return crypto.randomUUID();
}
