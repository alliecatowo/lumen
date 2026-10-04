import { gzipSync } from 'node:zlib';

export interface TarEntry {
  path: string;
  content?: Uint8Array | string;
  type?: string; // '0' file, '5' dir, '2' symlink ...
}

function field(buf: Uint8Array, off: number, len: number, s: string) {
  const b = new TextEncoder().encode(s);
  buf.set(b.subarray(0, len), off);
}

function header(e: TarEntry, size: number): Uint8Array {
  const h = new Uint8Array(512);
  field(h, 0, 100, e.path);
  field(h, 100, 8, '0000644\0');
  field(h, 108, 8, '0000000\0');
  field(h, 116, 8, '0000000\0');
  field(h, 124, 12, size.toString(8).padStart(11, '0') + '\0');
  field(h, 136, 12, '00000000000\0');
  h.fill(32, 148, 156);
  h[156] = (e.type ?? '0').charCodeAt(0);
  field(h, 257, 6, 'ustar\0');
  field(h, 263, 2, '00');
  let sum = 0;
  for (const b of h) sum += b;
  field(h, 148, 8, sum.toString(8).padStart(6, '0') + '\0 ');
  return h;
}

export function makeTar(entries: TarEntry[]): Uint8Array {
  const parts: Uint8Array[] = [];
  for (const e of entries) {
    const body = typeof e.content === 'string' ? new TextEncoder().encode(e.content) : e.content ?? new Uint8Array(0);
    parts.push(header(e, body.length));
    if (body.length) {
      parts.push(body);
      parts.push(new Uint8Array((512 - (body.length % 512)) % 512));
    }
  }
  parts.push(new Uint8Array(1024));
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

/** A well-formed package archive for `name`@`version`, unique per `salt`. */
export function makePackage(name = '@alice/pkg', version = '1.0.0', salt = String(Math.random())): Uint8Array {
  return new Uint8Array(
    gzipSync(
      makeTar([
        { path: 'lumen.toml', content: `[package]\nname = "${name}"\nversion = "${version}"\n` },
        { path: 'src/', type: '5' },
        { path: 'src/main.lm', content: `cell main() -> Int\n  # ${salt}\n  return 1\nend\n` },
      ]),
    ),
  );
}

export function makeArchive(entries: TarEntry[]): Uint8Array {
  return new Uint8Array(gzipSync(makeTar(entries)));
}
