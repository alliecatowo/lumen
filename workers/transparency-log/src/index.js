/**
 * Wares Transparency Log Service
 * 
 * An append-only, cryptographically verifiable transparency log for package publishes.
 * Based on Sigstore's Rekor design but simplified for Wares.
 * 
 * Features:
 * - Append-only log with monotonic indices
 * - Cryptographic chain of entries (each entry includes hash of previous)
 * - Inclusion proofs for verification
 * - Query by package name, version, or time range
 * 
 * Storage schema in D1 (SQLite):
 * - log_entries: id, "index", uuid, package_name, version, content_hash, 
 *                identity, entry_body, prev_hash, this_hash, integrated_at
 * - checkpoints: id, tree_size, root_hash, timestamp, signed_tree_head
 */

import { Router } from './router';
import {
  verifyPackageSignature,
  generateInclusionProof,
  generateConsistencyProof,
  computeHash
} from './crypto';

// Main entry point
export default {
  async fetch(request, env, ctx) {
    const router = new Router();

    // Health check
    router.get('/health', async () => {
      return json({ status: 'ok', service: 'wares-transparency-log' });
    });

    // Get log info (tree size, root hash)
    router.get('/api/v1/log', async () => {
      const checkpoint = await getLatestCheckpoint(env.wares_transparency_log);
      const count = await getLogCount(env.wares_transparency_log);
      return json({
        tree_size: count,
        root_hash: checkpoint?.root_hash || null,
        signed_tree_head: checkpoint?.signed_tree_head || null,
        timestamp: checkpoint?.timestamp || Date.now(),
      });
    });

    // Get entry by index
    router.get('/api/v1/log/entries/:index', async (req, params) => {
      const index = parseInt(params.index);
      if (isNaN(index)) {
        return error(400, 'Invalid index');
      }

      const entry = await getLogEntry(env.wares_transparency_log, index);
      if (!entry) {
        return error(404, 'Entry not found');
      }

      return json(entry);
    });

    // Query log entries
    router.get('/api/v1/log/query', async (req) => {
      const url = new URL(req.url);
      const packageName = url.searchParams.get('package');
      const version = url.searchParams.get('version');
      const identity = url.searchParams.get('identity');
      const limit = Math.min(parseInt(url.searchParams.get('limit') || '100'), 1000);
      const offset = parseInt(url.searchParams.get('offset') || '0');

      const entries = await queryLogEntries(env.wares_transparency_log, {
        packageName, version, identity, limit, offset
      });

      const total = await countLogEntries(env.wares_transparency_log, { packageName, version, identity });

      return json({
        entries,
        total,
        limit,
        offset,
      });
    });

    // Submit new entry (called by registry after package publish)
    router.post('/api/v1/log/entries', async (req) => {
      // Verify API key
      const apiKey = req.headers.get('X-API-Key');
      if (!apiKey || apiKey !== env.REGISTRY_API_KEY) {
        return error(401, 'Unauthorized');
      }

      const body = await req.json();

      // Validate required fields
      const required = ['package_name', 'version', 'content_hash', 'identity', 'signature', 'certificate'];
      for (const field of required) {
        if (!body[field]) {
          return error(400, `Missing required field: ${field}`);
        }
      }

      // Verify the package signature
      const sigValid = await verifyPackageSignature(body);
      if (!sigValid) {
        return error(400, 'Invalid package signature');
      }

      // Add entry to log
      try {
        const entry = await addLogEntry(env.wares_transparency_log, body);
        return json({
          inserted: true,
          index: entry.index,
          uuid: entry.uuid,
        }, 201);
      } catch (e) {
        return error(500, `Failed to append to log: ${e.message}`);
      }
    });

    // Verify inclusion proof
    router.get('/api/v1/log/proof/:index', async (req, params) => {
      const index = parseInt(params.index);
      if (isNaN(index)) {
        return error(400, 'Invalid index');
      }

      const proof = await generateInclusionProof(env.wares_transparency_log, index);
      if (!proof) {
        return error(404, 'Entry not found');
      }

      return json(proof);
    });

    // Get consistency proof
    router.get('/api/v1/log/consistency/:size1/:size2', async (req, params) => {
      const size1 = parseInt(params.size1);
      const size2 = parseInt(params.size2);

      if (isNaN(size1) || isNaN(size2)) {
        return error(400, 'Invalid tree sizes');
      }

      const proof = await generateConsistencyProof(env.wares_transparency_log, size1, size2);
      if (!proof) {
        return error(404, 'Consistency proof unavailable');
      }

      return json(proof);
    });

    // Get checkpoint (signed tree head)
    router.get('/api/v1/log/checkpoint', async () => {
      const checkpoint = await getLatestCheckpoint(env.wares_transparency_log);
      if (!checkpoint) {
        return error(404, 'No checkpoint found');
      }
      return json(checkpoint);
    });

    return router.handle(request);
  },
};

async function getLogCount(db) {
  const result = await db.prepare('SELECT COUNT(*) as count FROM log_entries').first();
  return result?.count || 0;
}

// =============================================================================
// Database Operations
// =============================================================================

async function getLogEntry(db, index) {
  const stmt = db.prepare(`
    SELECT * FROM log_entries WHERE "index" = ?
  `);
  const result = await stmt.bind(index).first();
  return result;
}

async function queryLogEntries(db, { packageName, version, identity, limit, offset }) {
  let sql = 'SELECT * FROM log_entries WHERE 1=1';
  const params = [];

  if (packageName) {
    sql += ' AND package_name = ?';
    params.push(packageName);
  }
  if (version) {
    sql += ' AND version = ?';
    params.push(version);
  }
  if (identity) {
    sql += ' AND identity = ?';
    params.push(identity);
  }

  sql += ' ORDER BY "index" DESC LIMIT ? OFFSET ?';
  params.push(limit, offset);

  const stmt = db.prepare(sql);
  const result = await stmt.bind(...params).all();
  return result.results || [];
}

async function countLogEntries(db, { packageName, version, identity }) {
  let sql = 'SELECT COUNT(*) as count FROM log_entries WHERE 1=1';
  const params = [];

  if (packageName) {
    sql += ' AND package_name = ?';
    params.push(packageName);
  }
  if (version) {
    sql += ' AND version = ?';
    params.push(version);
  }
  if (identity) {
    sql += ' AND identity = ?';
    params.push(identity);
  }

  const stmt = db.prepare(sql);
  const result = await stmt.bind(...params).first();
  return result?.count || 0;
}

async function addLogEntry(db, body) {
  const uuid = crypto.randomUUID();
  const index = await getNextIndex(db);
  const prevEntry = index > 0 ? await getLogEntry(db, index - 1) : null;
  const prevHash = prevEntry ? prevEntry.this_hash : '0'.repeat(64);

  // Build entry body
  const entryBody = JSON.stringify({
    package_name: body.package_name,
    version: body.version,
    content_hash: body.content_hash,
    identity: body.identity,
    certificate: body.certificate,
    signature: body.signature,
    timestamp: Date.now(),
  });

  // Compute this entry's hash (includes previous hash for chain)
  const thisHash = await computeEntryHash(index, entryBody, prevHash);

  const stmt = db.prepare(`
    INSERT INTO log_entries (
      "index", uuid, package_name, version, content_hash, 
      identity, entry_body, prev_hash, this_hash, integrated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `);

  await stmt.bind(
    index,
    uuid,
    body.package_name,
    body.version,
    body.content_hash,
    body.identity,
    entryBody,
    prevHash,
    thisHash,
    new Date().toISOString()
  ).run();

  // Update checkpoint
  await updateCheckpoint(db);

  return {
    index,
    uuid,
    package_name: body.package_name,
    version: body.version,
    content_hash: body.content_hash,
    identity: body.identity,
    integrated_at: new Date().toISOString(),
  };
}

async function getNextIndex(db) {
  const result = await db.prepare('SELECT MAX("index") as max_index FROM log_entries').first();
  return (result?.max_index ?? -1) + 1;
}

async function getLatestCheckpoint(db) {
  const result = await db.prepare(`
    SELECT * FROM checkpoints ORDER BY id DESC LIMIT 1
  `).first();
  return result;
}

async function updateCheckpoint(db) {
  const count = await db.prepare('SELECT COUNT(*) as count FROM log_entries').first();
  const treeSize = count?.count || 0;

  // Get root hash (hash of all entry hashes)
  const entries = await db.prepare('SELECT this_hash FROM log_entries ORDER BY "index"').all();
  const hashes = entries.results?.map(e => e.this_hash) || [];
  const rootHash = await computeMerkleRoot(hashes);

  // Create signed tree head
  const timestamp = Date.now();
  const sthData = `${treeSize}-${rootHash}-${timestamp}`;
  const signature = await signTreeHead(sthData);

  const stmt = db.prepare(`
    INSERT INTO checkpoints (tree_size, root_hash, timestamp, signed_tree_head)
    VALUES (?, ?, ?, ?)
  `);

  await stmt.bind(treeSize, rootHash, timestamp, signature).run();
}

async function getEntriesSince(db, startIndex) {
  const stmt = db.prepare(`
    SELECT * FROM log_entries WHERE "index" >= ? ORDER BY "index"
  `);
  const result = await stmt.bind(startIndex).all();
  return result.results || [];
}

// =============================================================================
// Cryptographic Operations
// =============================================================================

async function computeEntryHash(index, entryBody, prevHash) {
  const data = `${index}:${entryBody}:${prevHash}`;
  const encoder = new TextEncoder();
  const hashBuffer = await crypto.subtle.digest('SHA-256', encoder.encode(data));
  return Array.from(new Uint8Array(hashBuffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

async function computeMerkleRoot(hashes) {
  if (hashes.length === 0) {
    return '0'.repeat(64);
  }
  if (hashes.length === 1) {
    return hashes[0];
  }

  const nextLevel = [];
  for (let i = 0; i < hashes.length; i += 2) {
    const left = hashes[i];
    const right = hashes[i + 1] || left;
    const combined = await hashPair(left, right);
    nextLevel.push(combined);
  }

  return computeMerkleRoot(nextLevel);
}

async function hashPair(left, right) {
  const data = left + right;
  const encoder = new TextEncoder();
  const hashBuffer = await crypto.subtle.digest('SHA-256', encoder.encode(data));
  return Array.from(new Uint8Array(hashBuffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

async function signTreeHead(sthData) {
  // In production, this would be signed with the log's private key
  // For now, we just return a placeholder
  const encoder = new TextEncoder();
  const hashBuffer = await crypto.subtle.digest('SHA-256', encoder.encode(sthData + '-signed'));
  return Array.from(new Uint8Array(hashBuffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

// =============================================================================
// Helpers
// =============================================================================

function json(data, status = 200) {
  return new Response(JSON.stringify(data, null, 2), {
    status,
    headers: {
      'Content-Type': 'application/json',
      'Access-Control-Allow-Origin': '*',
    },
  });
}

function error(status, message) {
  return new Response(JSON.stringify({ error: message }), {
    status,
    headers: {
      'Content-Type': 'application/json',
      'Access-Control-Allow-Origin': '*',
    },
  });
}
