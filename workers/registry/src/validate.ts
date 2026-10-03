/** Input validation shared by the registry routes. */

/** Maximum decoded tarball size accepted by publish (10 MiB). */
export const MAX_TARBALL_BYTES = 10 * 1024 * 1024;
/** Maximum request body accepted by publish (base64 overhead included). */
export const MAX_PUBLISH_BODY_BYTES = 15 * 1024 * 1024;
export const MAX_DESCRIPTION_LEN = 1000;
export const MAX_PROOF_BYTES = 64 * 1024;

const NAME_PART = '[a-z0-9]+(?:-[a-z0-9]+)*';
/** Names accepted for reads (scoped or legacy unscoped). */
const READ_NAME_RE = new RegExp(`^(?:@${NAME_PART}/)?${NAME_PART}$`);
/** Names accepted for publish: scoped `@ns/name`, same rule as the CLI. */
const PUBLISH_NAME_RE = new RegExp(`^@${NAME_PART}/${NAME_PART}$`);

/** True when `name` is a syntactically safe package name for read routes. */
export function isValidReadName(name: unknown): name is string {
  return typeof name === 'string' && name.length > 0 && name.length <= 64 && READ_NAME_RE.test(name);
}

/** True when `name` may be published (mirrors lumen-cli `is_valid_package_name`). */
export function isValidPublishName(name: unknown): name is string {
  return typeof name === 'string' && name.length > 0 && name.length <= 64 && PUBLISH_NAME_RE.test(name);
}

/** Decode standard base64 into bytes, returning null on malformed input. */
export function decodeBase64(b64: unknown): Uint8Array | null {
  if (typeof b64 !== 'string' || b64.length === 0) return null;
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(b64) || b64.length % 4 !== 0) return null;
  try {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  } catch {
    return null;
  }
}

/** Lowercase hex SHA-256 of `data`. */
export async function sha256Hex(data: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', data as unknown as ArrayBuffer);
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

/** Constant-time string comparison. */
export function timingSafeEqual(a: string, b: string): boolean {
  const enc = new TextEncoder();
  const x = enc.encode(a);
  const y = enc.encode(b);
  let diff = x.length ^ y.length;
  const n = Math.max(x.length, y.length);
  for (let i = 0; i < n; i++) diff |= (x[i] ?? 0) ^ (y[i] ?? 0);
  return diff === 0;
}
