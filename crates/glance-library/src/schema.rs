//! `library.sqlite`'s schema: Swift MaCrate 0.16.4's twelve GRDB migrations as the literal SQL text Swift runs (the
//! whitespace of its multi-line literals included, so `sqlite_master` reads the same), applied with GRDB's
//! procedure, and GRDB's bookkeeping table.

use crate::error::{LibraryError, Result};
use rusqlite::Connection;
use std::collections::HashSet;

/// GRDB's bookkeeping table (`DatabaseMigrator`).
pub const GRDB_MIGRATIONS_DDL: &str = "CREATE TABLE IF NOT EXISTS grdb_migrations (identifier TEXT NOT NULL PRIMARY KEY)";

/// The migration identifiers, in registration order.
pub const MIGRATION_IDS: [&str; 12] = [
    "v1_initial_schema",
    "v2_clip_search_fts5",
    "v3_clip_duration",
    "v4_saved_filters",
    "v5_orphan_dismissed",
    "v6_clip_brief",
    "v7_drop_loop_markers",
    "v8_path_primary_key",
    "v9_stashes",
    "v10_clip_frame_rate",
    "v11_clip_video_size",
    "v12_clip_alpha",
];

const V1: &[&str] = &[
    "CREATE TABLE clips (
    clip_id TEXT PRIMARY KEY,
    current_path TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    file_mtime REAL NOT NULL,
    rating INTEGER NOT NULL DEFAULT 0,
    bpm REAL,
    mood TEXT,
    genre TEXT,
    energy INTEGER,
    loop_start_sec REAL,
    loop_end_sec REAL,
    loop_start_frame INTEGER,
    loop_end_frame INTEGER,
    -- v0.6.20: the four loop_* columns above are dropped
    -- in v7. They remain in the v1 CREATE TABLE
    -- definition because GRDB migrations replay
    -- historically (a fresh-install DB still runs v1
    -- then v7), and rewriting v1 would change the
    -- recorded migration shape. The columns are
    -- short-lived on a fresh install.
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL,
    is_orphan INTEGER NOT NULL DEFAULT 0
);",
    "CREATE TABLE clip_tags (
    clip_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (clip_id, tag),
    FOREIGN KEY (clip_id) REFERENCES clips(clip_id) ON DELETE CASCADE
);",
    "CREATE INDEX idx_clip_tags_tag ON clip_tags(tag);",
    "CREATE INDEX idx_clips_rating ON clips(rating);",
    "CREATE INDEX idx_clips_path ON clips(current_path);",
    "CREATE INDEX idx_clips_orphan ON clips(is_orphan);",
];

const V2: &[&str] = &["CREATE VIRTUAL TABLE clip_search USING fts5(
    clip_id UNINDEXED,
    filename,
    tags,
    mood,
    genre,
    tokenize = 'unicode61'
);"];

const V3: &[&str] = &["ALTER TABLE clips ADD COLUMN duration_sec REAL;"];

const V4: &[&str] = &["CREATE TABLE saved_filters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    filter_json TEXT NOT NULL,
    sort_value TEXT NOT NULL,
    created_at REAL NOT NULL
);"];

const V5: &[&str] = &["ALTER TABLE clips ADD COLUMN dismissed_at REAL;"];

const V6: &[&str] = &[
    "ALTER TABLE clips ADD COLUMN brief TEXT;",
    "DROP TABLE clip_search;",
    "CREATE VIRTUAL TABLE clip_search USING fts5(
    clip_id UNINDEXED,
    filename,
    tags,
    mood,
    genre,
    brief,
    tokenize = 'unicode61'
);",
];

const V7: &[&str] = &[
    "ALTER TABLE clips DROP COLUMN loop_start_sec;",
    "ALTER TABLE clips DROP COLUMN loop_end_sec;",
    "ALTER TABLE clips DROP COLUMN loop_start_frame;",
    "ALTER TABLE clips DROP COLUMN loop_end_frame;",
];

const V8_DEDUPE: &str = "DELETE FROM clips WHERE rowid NOT IN (
  SELECT rowid FROM (
    SELECT rowid,
           ROW_NUMBER() OVER (
             PARTITION BY current_path
             ORDER BY updated_at DESC, clip_id ASC
           ) AS rn
    FROM clips
  ) WHERE rn = 1
);";

const V8_REBUILD: &[&str] = &[
    "CREATE TABLE clips_new (
    current_path TEXT PRIMARY KEY,
    clip_id TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    file_mtime REAL NOT NULL,
    rating INTEGER NOT NULL DEFAULT 0,
    bpm REAL,
    mood TEXT,
    genre TEXT,
    energy INTEGER,
    brief TEXT,
    duration_sec REAL,
    dismissed_at REAL,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL,
    is_orphan INTEGER NOT NULL DEFAULT 0
);",
    "INSERT INTO clips_new (
    current_path, clip_id, file_size, file_mtime,
    rating, bpm, mood, genre, energy, brief,
    duration_sec, dismissed_at, created_at, updated_at, is_orphan
)
SELECT
    current_path, clip_id, file_size, file_mtime,
    rating, bpm, mood, genre, energy, brief,
    duration_sec, dismissed_at, created_at, updated_at, is_orphan
FROM clips;",
    "DROP TABLE clips;",
    "ALTER TABLE clips_new RENAME TO clips;",
    "CREATE INDEX idx_clips_clip_id ON clips(clip_id);",
    "CREATE INDEX idx_clips_rating  ON clips(rating);",
    "CREATE INDEX idx_clips_orphan  ON clips(is_orphan);",
    "CREATE TABLE clip_tags_new (
    current_path TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (current_path, tag),
    FOREIGN KEY (current_path)
        REFERENCES clips(current_path)
        ON DELETE CASCADE
        ON UPDATE CASCADE
);",
    "INSERT INTO clip_tags_new (current_path, tag)
SELECT c.current_path, t.tag
FROM clip_tags t
JOIN clips c ON c.clip_id = t.clip_id;",
    "DROP TABLE clip_tags;",
    "ALTER TABLE clip_tags_new RENAME TO clip_tags;",
    "CREATE INDEX idx_clip_tags_tag ON clip_tags(tag);",
    "DROP TABLE clip_search;",
    "CREATE VIRTUAL TABLE clip_search USING fts5(
    current_path UNINDEXED,
    filename,
    tags,
    mood,
    genre,
    brief,
    tokenize = 'unicode61'
);",
];

const V9: &[&str] = &[
    "CREATE TABLE stashes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL
);",
    "CREATE TABLE stash_clips (
    stash_id TEXT NOT NULL,
    clip_id TEXT NOT NULL,
    relative_path_hint TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (stash_id, clip_id),
    FOREIGN KEY (stash_id) REFERENCES stashes(id) ON DELETE CASCADE
);",
    "CREATE INDEX idx_stash_clips_clip_id ON stash_clips(clip_id);",
];

const V10: &[&str] = &["ALTER TABLE clips ADD COLUMN frame_rate REAL;"];

const V11: &[&str] = &["ALTER TABLE clips ADD COLUMN video_width INTEGER;", "ALTER TABLE clips ADD COLUMN video_height INTEGER;"];

const V12: &[&str] = &["CREATE TABLE clip_alpha (
    clip_id TEXT PRIMARY KEY,
    probe TEXT,
    alpha_type TEXT,
    updated_at REAL NOT NULL
);"];

/// What running the migrations did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// The identifiers run now, in order.
    pub applied: Vec<&'static str>,
    /// v8 ran on a library that had rows: the app should re-index every root once (Swift sets the user default
    /// `Crate.v0.6.33.forceFullReindex`; the Rust app keeps its own flag).
    pub needs_full_reindex: bool,
    /// Duplicate-path rows v8 removed before keying `clips` by path.
    pub v8_duplicates_removed: usize,
}

fn run_statements(conn: &Connection, statements: &[&str]) -> Result<()> {
    for sql in statements {
        conn.execute_batch(sql)?;
    }
    Ok(())
}

fn count(conn: &Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |r| r.get(0))?)
}

fn body(conn: &Connection, id: &str, report: &mut MigrationReport) -> Result<()> {
    match id {
        "v1_initial_schema" => run_statements(conn, V1),
        "v2_clip_search_fts5" => run_statements(conn, V2),
        "v3_clip_duration" => run_statements(conn, V3),
        "v4_saved_filters" => run_statements(conn, V4),
        "v5_orphan_dismissed" => run_statements(conn, V5),
        "v6_clip_brief" => run_statements(conn, V6),
        "v7_drop_loop_markers" => run_statements(conn, V7),
        "v8_path_primary_key" => {
            let duplicates = count(conn, "SELECT COUNT(*) - COUNT(DISTINCT current_path) FROM clips")?;
            conn.execute_batch(V8_DEDUPE)?;
            run_statements(conn, V8_REBUILD)?;
            report.v8_duplicates_removed = usize::try_from(duplicates).unwrap_or(0);
            if count(conn, "SELECT COUNT(*) FROM clips")? > 0 {
                report.needs_full_reindex = true;
            }
            Ok(())
        }
        "v9_stashes" => run_statements(conn, V9),
        "v10_clip_frame_rate" => run_statements(conn, V10),
        "v11_clip_video_size" => run_statements(conn, V11),
        "v12_clip_alpha" => run_statements(conn, V12),
        _ => Ok(()),
    }
}

/// GRDB's `Migration.run` with deferred foreign-key checks (foreign keys on): `PRAGMA foreign_keys = OFF`,
/// `BEGIN IMMEDIATE`, the body, the identifier into `grdb_migrations`, `PRAGMA foreign_key_check` (any row fails the
/// migration), `COMMIT`, and `PRAGMA foreign_keys = ON` whatever happened.
fn run_one(conn: &Connection, id: &'static str, report: &mut MigrationReport) -> Result<()> {
    let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
    let transaction = |conn: &Connection, report: &mut MigrationReport| -> Result<()> {
        conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        let inner = (|| -> Result<()> {
            body(conn, id, report)?;
            conn.execute("INSERT INTO grdb_migrations (identifier) VALUES (?)", [id])?;
            if foreign_keys != 0 {
                let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
                let violations = stmt.query_map([], |_| Ok(()))?.count();
                if violations > 0 {
                    return Err(LibraryError::ForeignKeyViolation { migration: id, violations });
                }
            }
            Ok(())
        })();
        match inner {
            Ok(()) => {
                conn.execute_batch("COMMIT TRANSACTION")?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK TRANSACTION");
                Err(e)
            }
        }
    };
    if foreign_keys == 0 {
        return transaction(conn, report);
    }
    conn.execute_batch("PRAGMA foreign_keys = OFF")?;
    let result = transaction(conn, report);
    let restored = conn.execute_batch("PRAGMA foreign_keys = ON");
    result?;
    restored?;
    Ok(())
}

/// `DatabaseMigrator.migrate`: create `grdb_migrations` if needed, then run, in order, every registered migration
/// whose identifier is not recorded. Identifiers the table holds that this build does not know (a newer writer's)
/// are tolerated, as GRDB does.
pub fn migrate(conn: &Connection) -> Result<MigrationReport> {
    conn.execute_batch(GRDB_MIGRATIONS_DDL)?;
    let applied: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT identifier FROM grdb_migrations")?;
        stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?
    };
    let mut report = MigrationReport::default();
    for id in MIGRATION_IDS {
        if applied.contains(id) {
            continue;
        }
        run_one(conn, id, &mut report)?;
        report.applied.push(id);
    }
    Ok(report)
}
