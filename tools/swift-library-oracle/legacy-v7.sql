CREATE TABLE IF NOT EXISTS grdb_migrations (identifier TEXT NOT NULL PRIMARY KEY);
CREATE TABLE clips (
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
);
CREATE TABLE clip_tags (
    clip_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (clip_id, tag),
    FOREIGN KEY (clip_id) REFERENCES clips(clip_id) ON DELETE CASCADE
);
CREATE INDEX idx_clip_tags_tag ON clip_tags(tag);
CREATE INDEX idx_clips_rating ON clips(rating);
CREATE INDEX idx_clips_path ON clips(current_path);
CREATE INDEX idx_clips_orphan ON clips(is_orphan);
INSERT INTO grdb_migrations (identifier) VALUES ('v1_initial_schema');
CREATE VIRTUAL TABLE clip_search USING fts5(
    clip_id UNINDEXED,
    filename,
    tags,
    mood,
    genre,
    tokenize = 'unicode61'
);
INSERT INTO grdb_migrations (identifier) VALUES ('v2_clip_search_fts5');
ALTER TABLE clips ADD COLUMN duration_sec REAL;
INSERT INTO grdb_migrations (identifier) VALUES ('v3_clip_duration');
CREATE TABLE saved_filters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    filter_json TEXT NOT NULL,
    sort_value TEXT NOT NULL,
    created_at REAL NOT NULL
);
INSERT INTO grdb_migrations (identifier) VALUES ('v4_saved_filters');
ALTER TABLE clips ADD COLUMN dismissed_at REAL;
INSERT INTO grdb_migrations (identifier) VALUES ('v5_orphan_dismissed');
ALTER TABLE clips ADD COLUMN brief TEXT;
DROP TABLE clip_search;
CREATE VIRTUAL TABLE clip_search USING fts5(
    clip_id UNINDEXED,
    filename,
    tags,
    mood,
    genre,
    brief,
    tokenize = 'unicode61'
);
INSERT INTO grdb_migrations (identifier) VALUES ('v6_clip_brief');
ALTER TABLE clips DROP COLUMN loop_start_sec;
ALTER TABLE clips DROP COLUMN loop_end_sec;
ALTER TABLE clips DROP COLUMN loop_start_frame;
ALTER TABLE clips DROP COLUMN loop_end_frame;
INSERT INTO grdb_migrations (identifier) VALUES ('v7_drop_loop_markers');
-- Synthetic v7-era rows: clip_id-keyed, with two duplicate-path groups for v8's dedupe.
INSERT INTO clips (clip_id, current_path, file_size, file_mtime, rating, bpm, mood, genre, energy, created_at, updated_at, is_orphan, duration_sec, dismissed_at, brief)
VALUES ('legacy-1:100', '/Volumes/Fixture/Old/a.mov', 100, 1700000000.5, 3, 120.0, 'dark', 'techno', 5, 1700000000.0, 1700000100.0, 0, 12.5, NULL, 'brief a'),
       ('legacy-2a:200', '/Volumes/Fixture/Old/dup.mov', 200, 1700000000.0, 1, NULL, NULL, NULL, NULL, 1700000000.0, 1700000200.0, 0, NULL, NULL, NULL),
       ('legacy-2b:200', '/Volumes/Fixture/Old/dup.mov', 200, 1700000000.0, 2, NULL, 'kept', NULL, NULL, 1700000000.0, 1700000300.0, 0, NULL, NULL, NULL),
       ('legacy-3b:300', '/Volumes/Fixture/Old/tie.mov', 300, 1700000000.0, 0, NULL, NULL, NULL, NULL, 1700000000.0, 1700000400.0, 0, NULL, NULL, 'tie b'),
       ('legacy-3a:300', '/Volumes/Fixture/Old/tie.mov', 300, 1700000000.0, 0, NULL, NULL, NULL, NULL, 1700000000.0, 1700000400.0, 0, NULL, NULL, 'tie a'),
       ('legacy-4:400', '/Volumes/Fixture/Old/gone.mov', 400, 0.0, 0, NULL, NULL, NULL, NULL, 1700000000.0, 1700000000.0, 1, NULL, 1700000999.25, NULL),
       ('legacy-5:500', '/Volumes/Fixture/Old/Café #1.mov', 500, 1700000000.0, 4, 90.5, NULL, 'house', 9, 1700000000.0, 1700000050.0, 0, 3.0, NULL, NULL);
INSERT INTO clip_tags (clip_id, tag) VALUES
       ('legacy-1:100', 'x'), ('legacy-1:100', 'y'),
       ('legacy-2a:200', 'dropped'), ('legacy-2b:200', 'kept'),
       ('legacy-3a:300', 'tie-a'), ('legacy-3b:300', 'tie-b'),
       ('legacy-5:500', 'caf'), ('legacy-5:500', 'z');
INSERT INTO clip_search (clip_id, filename, tags, mood, genre, brief) VALUES
       ('legacy-1:100', 'a', 'x y', 'dark', 'techno', 'brief a'),
       ('legacy-2b:200', 'dup', 'kept', 'kept', '', '');
INSERT INTO saved_filters (name, filter_json, sort_value, created_at)
VALUES ('Legacy', '{"tags":["x"],"tagsLogic":"and","ratingMin":3}', 'ratingDesc', 1700000500.0);
