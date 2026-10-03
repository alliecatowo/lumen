const b64 = (bytes) => Buffer.from(bytes).toString('base64');

export async function genKey() {
  return crypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, true, ['sign', 'verify']);
}
export async function spki(key) {
  return b64(new Uint8Array(await crypto.subtle.exportKey('spki', key.publicKey)));
}
export async function sign(privateKey, data) {
  const sig = await crypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, privateKey, new TextEncoder().encode(data));
  return b64(new Uint8Array(sig));
}

/** Build a certificate in the same format the registry CA issues. */
export async function issueCert(caKey, userKey, { subject, notBefore = Date.now() - 1000, notAfter = Date.now() + 600000 }) {
  const certJson = JSON.stringify(
    {
      cert_id: 'cert-test',
      subject,
      issuer: 'wares.lumen-lang.com',
      not_before: new Date(notBefore).toISOString(),
      not_after: new Date(notAfter).toISOString(),
      public_key: await spki(userKey),
      key_algorithm: 'ECDSA P-256',
    },
    null,
    2,
  );
  const caSig = await sign(caKey.privateKey, certJson);
  return ['-----BEGIN WARES CERTIFICATE-----', btoa(certJson), '', '-----BEGIN SIGNATURE-----', caSig, '-----END WARES CERTIFICATE-----'].join('\n');
}

export const HASH = 'sha256:' + 'ab'.repeat(32);
