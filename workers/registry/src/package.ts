/**
 * Validation of uploaded package archives. The registry hosts these files on lumen-lang.com
 * origins, so only a real lumen package (gzip'd tar of lumen sources and a manifest) is
 * accepted; everything else, including archives that could be abused as a malware host, is rejected.
 */

/** Largest uncompressed archive (all entries) we will inflate. */
export const MAX_UNCOMPRESSED_BYTES = 30 * 1024 * 1024;
/** Largest single file inside an archive. */
export const MAX_ENTRY_BYTES = 2 * 1024 * 1024;
export const MAX_ENTRIES = 1000;
export const MAX_PATH_LEN = 200;
/** Maximum inflate ratio (decompression-bomb guard). */
export const MAX_RATIO = 100;

export interface PackageCheck {
  ok: boolean;
  error?: string;
  files?: string[];
}

const fail = (error: string): PackageCheck => ({ ok: false, error });

/** Files that may appear at the archive root or anywhere (exact base names). */
const ALLOWED_BASENAMES = new Set(['lumen.toml', 'README.md', 'LICENSE', 'LICENSE.md', 'LICENSE.txt', 'CHANGELOG.md']);
/** Allowed extensions below src/, tests/ and examples/. */
const SOURCE_SUFFIXES = ['.lm', '.lumen', '.lm.md', '.lumen.md'];
const DATA_SUFFIXES = ['.toml', '.json', '.txt', '.md'];
const ALLOWED_DIRS = new Set(['src', 'tests', 'examples']);

/** Gunzip with a hard output cap; returns null if it would exceed `limit`. */
export async function gunzipLimited(data: Uint8Array, limit: number): Promise<Uint8Array | 'too-big' | 'invalid'> {
  if (data.length < 18 || data[0] !== 0x1f || data[1] !== 0x8b || data[2] !== 0x08) return 'invalid';
  try {
    const stream = new Blob([data as unknown as BlobPart]).stream().pipeThrough(new DecompressionStream('gzip'));
    const reader = stream.getReader();
    const chunks: Uint8Array[] = [];
    let total = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      total += value.length;
      if (total > limit) {
        await reader.cancel();
        return 'too-big';
      }
      chunks.push(value);
    }
    const out = new Uint8Array(total);
    let off = 0;
    for (const c of chunks) {
      out.set(c, off);
      off += c.length;
    }
    return out;
  } catch {
    return 'invalid';
  }
}

function cstr(buf: Uint8Array, start: number, len: number): string {
  let end = start;
  while (end < start + len && buf[end] !== 0) end++;
  return new TextDecoder('utf-8', { fatal: false }).decode(buf.subarray(start, end));
}

function octal(buf: Uint8Array, start: number, len: number): number {
  const s = cstr(buf, start, len).trim();
  if (!/^[0-7]*$/.test(s)) return NaN;
  return s === '' ? 0 : parseInt(s, 8);
}

function headerChecksumOk(h: Uint8Array): boolean {
  let sum = 0;
  for (let i = 0; i < 512; i++) sum += i >= 148 && i < 156 ? 32 : h[i];
  return sum === octal(h, 148, 8);
}

function pathAllowed(path: string, isDir: boolean): string | null {
  if (path.length === 0 || path.length > MAX_PATH_LEN) return 'path length';
  if (path.startsWith('/') || path.includes('\\') || path.includes('\0')) return 'absolute or unsafe path';
  const parts = path.split('/').filter((p) => p !== '');
  if (parts.some((p) => p === '.' || p === '..')) return 'path traversal';
  if (parts.some((p) => p.startsWith('.'))) return 'hidden files are not allowed';
  if (isDir) return parts.length === 1 && !ALLOWED_DIRS.has(parts[0]) ? 'directory not allowed' : null;
  const base = parts[parts.length - 1];
  if (parts.length === 1) {
    return ALLOWED_BASENAMES.has(base) || SOURCE_SUFFIXES.some((s) => base.endsWith(s)) ? null : `file type not allowed: ${path}`;
  }
  if (!ALLOWED_DIRS.has(parts[0])) return `unexpected top-level directory: ${parts[0]}`;
  const ok = SOURCE_SUFFIXES.some((s) => base.endsWith(s)) || DATA_SUFFIXES.some((s) => base.endsWith(s)) || ALLOWED_BASENAMES.has(base);
  return ok ? null : `file type not allowed: ${path}`;
}

/** True if the bytes look like text (no NULs, valid UTF-8). */
function isText(b: Uint8Array): boolean {
  if (b.includes(0)) return false;
  try {
    new TextDecoder('utf-8', { fatal: true }).decode(b);
    return true;
  } catch {
    return false;
  }
}

/** Minimal `[package]` name/version extraction from lumen.toml. */
export function manifestFields(toml: string): { name?: string; version?: string } {
  let inPackage = false;
  const out: { name?: string; version?: string } = {};
  for (const raw of toml.split('\n')) {
    const line = raw.trim();
    if (line.startsWith('[')) {
      inPackage = line === '[package]';
      continue;
    }
    if (!inPackage) continue;
    const m = /^(name|version)\s*=\s*"([^"\n]*)"\s*(#.*)?$/.exec(line);
    if (m) (out as any)[m[1]] = m[2];
  }
  return out;
}

/**
 * Validate an uploaded archive against the publish request. Checks gzip framing and size /
 * ratio limits, tar structure (regular files and directories only), safe relative paths,
 * allowed file types (text only), and a `lumen.toml` whose [package] name and version match.
 */
export async function validatePackageArchive(
  data: Uint8Array,
  expect: { name: string; version: string },
): Promise<PackageCheck> {
  const limit = Math.min(MAX_UNCOMPRESSED_BYTES, Math.max(1, data.length) * MAX_RATIO);
  const raw = await gunzipLimited(data, limit);
  if (raw === 'invalid') return fail('Not a gzip archive');
  if (raw === 'too-big') return fail('Archive expands beyond the allowed size (decompression limit)');
  if (raw.length < 1024) return fail('Archive is empty or truncated');

  const files: string[] = [];
  let manifest: string | null = null;
  let pos = 0;
  let entries = 0;
  let pendingLongName: string | null = null;
  let sawEnd = false;

  while (pos + 512 <= raw.length) {
    const h = raw.subarray(pos, pos + 512);
    if (h.every((b) => b === 0)) {
      sawEnd = true;
      break;
    }
    if (!headerChecksumOk(h)) return fail('Corrupt tar header');
    if (cstr(h, 257, 5) !== 'ustar') return fail('Unsupported tar format (expected ustar)');
    const size = octal(h, 124, 12);
    if (!Number.isFinite(size) || size < 0) return fail('Invalid tar entry size');
    const type = String.fromCharCode(h[156] === 0 ? 0x30 : h[156]);
    const dataStart = pos + 512;
    const dataEnd = dataStart + size;
    if (dataEnd > raw.length) return fail('Truncated tar entry');
    const body = raw.subarray(dataStart, dataEnd);
    pos = dataStart + Math.ceil(size / 512) * 512;

    if (type === 'L') {
      pendingLongName = cstr(body, 0, body.length);
      continue;
    }
    if (type !== '0' && type !== '5') {
      return fail(`Entry type '${type}' is not allowed (only regular files and directories)`);
    }
    entries++;
    if (entries > MAX_ENTRIES) return fail(`Too many entries (max ${MAX_ENTRIES})`);

    const prefix = cstr(h, 345, 155);
    let path = pendingLongName ?? (prefix ? `${prefix}/${cstr(h, 0, 100)}` : cstr(h, 0, 100));
    pendingLongName = null;
    path = path.replace(/^\.\//, '').replace(/\/+$/, '');
    const isDir = type === '5';
    if (path === '' || path === '.') continue;
    const bad = pathAllowed(path, isDir);
    if (bad) return fail(`Rejected entry: ${bad}`);
    if (isDir) continue;
    if (size > MAX_ENTRY_BYTES) return fail(`File too large: ${path}`);
    if (!isText(body)) return fail(`Binary content is not allowed: ${path}`);
    files.push(path);
    if (path === 'lumen.toml') manifest = new TextDecoder().decode(body);
  }
  if (!sawEnd) return fail('Archive has no tar end marker');
  if (manifest === null) return fail('Missing lumen.toml at the archive root');
  if (!files.some((f) => SOURCE_SUFFIXES.some((s) => f.endsWith(s)))) return fail('Archive contains no lumen source files');

  const m = manifestFields(manifest);
  if (!m.name || !m.version) return fail('lumen.toml must have a [package] section with name and version');
  const tomlName = m.name.replace(/^@/, '');
  const wantName = expect.name.replace(/^@/, '');
  if (tomlName !== wantName && tomlName !== wantName.split('/').pop()) return fail(`lumen.toml name "${m.name}" does not match the published name "${expect.name}"`);
  if (m.version !== expect.version) return fail(`lumen.toml version "${m.version}" does not match the published version "${expect.version}"`);
  return { ok: true, files };
}
