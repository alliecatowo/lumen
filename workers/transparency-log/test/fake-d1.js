import { DatabaseSync } from 'node:sqlite';
import { readFileSync } from 'node:fs';

/** Tiny D1-shaped adapter over node:sqlite, loaded with the real schema. */
export function createFakeD1() {
  const db = new DatabaseSync(':memory:');
  db.exec(readFileSync(new URL('../schema.sql', import.meta.url), 'utf8'));
  return {
    raw: db,
    prepare(sql) {
      const st = db.prepare(sql);
      const ops = (params) => ({
        first: async () => st.get(...params) ?? null,
        all: async () => ({ results: st.all(...params) }),
        run: async () => {
          st.run(...params);
          return { success: true };
        },
      });
      return { ...ops([]), bind: (...params) => ops(params) };
    },
  };
}
