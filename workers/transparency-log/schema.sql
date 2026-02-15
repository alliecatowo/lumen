-- Wares Transparency Log Database Schema
-- Run: wrangler d1 execute wares-transparency-log --file=schema.sql

-- Log entries table (the actual transparency log)
CREATE TABLE IF NOT EXISTS log_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    "index" INTEGER UNIQUE NOT NULL,         -- Monotonic index (0, 1, 2, ...)
    uuid TEXT UNIQUE NOT NULL,               -- Entry UUID
    package_name TEXT NOT NULL,              -- Package name
    version TEXT NOT NULL,                   -- Package version
    content_hash TEXT NOT NULL,              -- SHA-256 hash of package content
    identity TEXT NOT NULL,                  -- OIDC identity that signed
    entry_body TEXT NOT NULL,                -- Full entry JSON
    prev_hash TEXT NOT NULL,                 -- Hash of previous entry
    this_hash TEXT NOT NULL,                 -- Hash of this entry
    integrated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Checkpoints table (signed tree heads)
CREATE TABLE IF NOT EXISTS checkpoints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tree_size INTEGER NOT NULL,              -- Number of entries in tree
    root_hash TEXT NOT NULL,                 -- Merkle root hash
    timestamp INTEGER NOT NULL,              -- Unix timestamp
    signed_tree_head TEXT NOT NULL           -- Signature of tree head
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_entries_package ON log_entries(package_name);
CREATE INDEX IF NOT EXISTS idx_entries_package_version ON log_entries(package_name, version);
CREATE INDEX IF NOT EXISTS idx_entries_identity ON log_entries(identity);
CREATE INDEX IF NOT EXISTS idx_entries_integrated_at ON log_entries(integrated_at);

-- Monitoring table (for detecting split-view attacks)
CREATE TABLE IF NOT EXISTS monitoring_clients (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    client_id TEXT UNIQUE NOT NULL,          -- Client identifier
    last_checked_index INTEGER DEFAULT 0,    -- Last index client saw
    first_seen DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_seen DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Initialize with empty checkpoint
INSERT OR IGNORE INTO checkpoints (tree_size, root_hash, timestamp, signed_tree_head)
VALUES (0, '0' || hex(zeroblob(63)), 0, 'initial');
