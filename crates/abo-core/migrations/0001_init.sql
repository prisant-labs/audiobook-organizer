-- 0001_init.sql - the first (frozen) schema for Audiobook Organizer.
--
-- v0.1.0 spine, Phase 2 (database). Creates the five spine tables from the
-- feature breakdown Section 7 / architecture Section 4: scans, entries, jobs,
-- settings, activity_records. Types are kept simple and SQLite-idiomatic;
-- timestamps are stored as TEXT (ISO-8601 strings), so the core carries no
-- chrono dependency (this phase's brief item 7).
--
-- Applied at open by sqlx::migrate!() using the RUNTIME query API (no
-- compile-time query! macros, so builds never need DATABASE_URL). Pre-v1 the
-- schema is resettable; later features add columns additively (spec OQ-2).

-- One row per snapshot (a live scan or an imported WizTree CSV). F-105.
CREATE TABLE scans (
    id           INTEGER PRIMARY KEY,
    source       TEXT CHECK (source IN ('live', 'csv')),
    root_path    TEXT NOT NULL,
    started_at   TEXT NOT NULL,
    completed_at TEXT,
    entry_count  INTEGER,
    total_bytes  INTEGER,
    status       TEXT NOT NULL   -- running | completed | failed
);

-- One row per file/folder captured in a snapshot. F-101 / F-105.
--
-- parent_id is a LOGICAL reference to entries.id (the tree edge), but is
-- deliberately declared WITHOUT a hard FOREIGN KEY. Rationale: the scanner
-- bulk-inserts entries in a single batched transaction and a parent may not
-- yet be committed when a child row is written; a hard self-FK would force
-- insert ordering (or deferred-constraint gymnastics) and slow the hot path.
-- Tree integrity is enforced in the scanner, not the schema. scan_id keeps a
-- hard FK to scans(id) because scans are always written before their entries.
CREATE TABLE entries (
    id         INTEGER PRIMARY KEY,
    scan_id    INTEGER NOT NULL REFERENCES scans(id),
    parent_id  INTEGER,          -- logical entries.id parent; no hard FK (bulk-insert speed)
    path       TEXT NOT NULL,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL,    -- file | dir
    file_class TEXT,
    size       INTEGER NOT NULL DEFAULT 0,
    mtime      TEXT,
    depth      INTEGER NOT NULL
);

-- The two query indexes that back tree navigation and path lookups (FD-20).
CREATE INDEX idx_entries_scan_parent ON entries (scan_id, parent_id);
CREATE INDEX idx_entries_scan_path   ON entries (scan_id, path);

-- One row per long-running job (scan/hash/apply/rollback). F-104.
CREATE TABLE jobs (
    id            INTEGER PRIMARY KEY,
    kind          TEXT NOT NULL,
    state         TEXT NOT NULL,
    progress_json TEXT,
    started_at    TEXT,
    finished_at   TEXT,
    error_code    TEXT
);

-- Singleton app settings (spec OQ-2): a single enforced row (CHECK id = 1).
-- Minimal at spine; F-803 (v0.4.0) adds theme/roots/retention columns via
-- additive migrations. The migration seeds the one and only row.
CREATE TABLE settings (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    updated_at TEXT
);
INSERT INTO settings (id) VALUES (1);

-- App-level audit trail, distinct from the per-operation journal. F-1001.
CREATE TABLE activity_records (
    id          INTEGER PRIMARY KEY,
    action      TEXT NOT NULL,
    params_json TEXT,
    outcome     TEXT,
    created_at  TEXT NOT NULL
);
