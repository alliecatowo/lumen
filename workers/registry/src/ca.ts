/**
 * Certificate Authority for Wares Registry
 */

import { importPrivateKey, signData, generateRandomHex, generateUuid } from './crypto';

export interface IdentityClaims {
    sub: string;
    iss: string;
    aud: string;
    email?: string;
    name?: string;
    repository?: string;
    workflow_ref?: string;
    event_name?: string;
    iat: number;
    exp: number;
}

export interface IssuedCertificate {
    cert_id: string;
    certificate_pem: string;
    public_key: string;
    identity: IdentityClaims; // Full object for client
    not_before: string;
    not_after: string;
    log_index?: number;
}

export class CertificateAuthority {
    private privateKey: CryptoKey | null = null;

    constructor(private privateKeyPem: string) { }

    /**
     * Initialize the CA (import key).
     */
    async initialize(): Promise<void> {
        if (!this.privateKey) {
            try {
                this.privateKey = await importPrivateKey(this.privateKeyPem);
            } catch (e) {
                console.error('Failed to import CA private key:', e);
                throw new Error('CA initialization failed');
            }
        }
    }

    /**
     * Issue an ephemeral certificate.
     */
    async issueCertificate(
        publicKey: string,
        identity: IdentityClaims,
        issuer: string = 'wares.lumen-lang.com',
        validityMinutes: number = 10
    ): Promise<IssuedCertificate> {
        if (!this.privateKey) {
            await this.initialize();
        }

        const certId = `cert-${generateRandomHex(8)}-${generateRandomHex(
            4
        )}-${generateRandomHex(4)}-${generateRandomHex(4)}-${generateRandomHex(
            12
        )}`;

        const now = new Date();
        const notAfter = new Date(now.getTime() + validityMinutes * 60 * 1000);

        // Create the certificate data (Custom JSON format)
        // NOTE: This JSON structure matches what the Rust client expects inside the PEM
        const certJson = {
            cert_id: certId,
            subject: this.formatIdentityString(identity), // Flattened string for cert subject
            issuer: issuer,
            not_before: now.toISOString(),
            not_after: notAfter.toISOString(),
            public_key: publicKey, // Client sends SPKI base64
            key_algorithm: 'ECDSA P-256',
        };

        const certDataString = JSON.stringify(certJson, null, 2);
        const certDataB64 = btoa(certDataString);

        // Sign the JSON string
        const signatureB64 = await signData(this.privateKey!, certDataString);

        // Create PEM format
        const certificatePem = [
            '-----BEGIN WARES CERTIFICATE-----',
            certDataB64,
            '',
            '-----BEGIN SIGNATURE-----',
            signatureB64,
            '-----END WARES CERTIFICATE-----',
        ].join('\n');

        return {
            cert_id: certId,
            certificate_pem: certificatePem,
            public_key: publicKey,
            identity: identity, // Return full object to client
            not_before: now.toISOString(),
            not_after: notAfter.toISOString(),
        };
    }

    /**
     * Format identity claims into a human-readable string for the subject.
     */
    private formatIdentityString(claims: IdentityClaims): string {
        if (claims.repository) {
            if (claims.workflow_ref) {
                return `${claims.repository}/${claims.workflow_ref}`;
            }
            return claims.repository;
        }
        return claims.sub;
    }
}
