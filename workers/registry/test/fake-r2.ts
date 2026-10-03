/** Minimal in-memory R2 bucket honouring `onlyIf` etag conditions and list pagination. */
export class FakeR2 {
  store = new Map<string, { data: Uint8Array; etag: string }>();
  private n = 0;
  pageSize = 1000;

  async get(key: string) {
    const o = this.store.get(key);
    if (!o) return null;
    const data = o.data;
    return {
      key,
      etag: o.etag,
      body: new Blob([data as any]).stream(),
      json: async () => JSON.parse(new TextDecoder().decode(data)),
      text: async () => new TextDecoder().decode(data),
      arrayBuffer: async () => data.buffer,
    };
  }

  async put(key: string, value: string | Uint8Array, opts: any = {}) {
    const cur = this.store.get(key);
    const cond = opts.onlyIf;
    if (cond?.etagDoesNotMatch === '*' && cur) return null;
    if (cond?.etagMatches && (!cur || cur.etag !== cond.etagMatches)) return null;
    const data = typeof value === 'string' ? new TextEncoder().encode(value) : value;
    this.store.set(key, { data, etag: `etag-${++this.n}` });
    return { key };
  }

  async delete(key: string) {
    this.store.delete(key);
  }

  async list(opts: { prefix?: string; cursor?: string } = {}) {
    const all = [...this.store.keys()].filter((k) => k.startsWith(opts.prefix ?? '')).sort();
    const start = opts.cursor ? parseInt(opts.cursor, 10) : 0;
    const page = all.slice(start, start + this.pageSize);
    const truncated = start + this.pageSize < all.length;
    return { objects: page.map((key) => ({ key })), truncated, cursor: truncated ? String(start + this.pageSize) : undefined };
  }
}
