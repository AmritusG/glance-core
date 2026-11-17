//! `library.sqlite` as Swift MaCrate's `MetadataStore` opens, reads and writes it: one connection, foreign keys
//! on, a 5 s busy timeout, the journal mode left alone, `<same folder>/measurements.sqlite` attached as `meas`, the
//! twelve migrations, and every read and write with the same SQL. Synchronous; run it on a worker thread.

use crate::error::{LibraryError, PresetError, Result};
use crate::filter::{ClipFilter, ClipSort, SavedPreset};
use crate::measurements::{self, ClipMeasurement, SCHEMA_NAME};
use crate::query::{self, QueryPlan};
use crate::schema::{self, MigrationReport};
use crate::search::{filename_token, sanitize_search_query};
use crate::stash::{Stash, StashEntry};
use crate::unicode::trim_whitespaces_and_newlines;
use glance_media::AlphaView;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, ToSql, params, params_from_iter};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Where the library lives (`MetadataStore.Location`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    /// A file; `measurements.sqlite` is attached from the same folder.
    File(PathBuf),
    /// A throw-away in-memory library (iCrate's search index); measurements attach `:memory:`.
    InMemory,
}

/// One `clips` row (`ClipRow`): one per file path. Times are seconds since 1970.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipRow {
    pub clip_id: String,
    pub current_path: String,
    pub file_size: i64,
    pub file_mtime: f64,
    pub rating: i64,
    pub bpm: Option<f64>,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub energy: Option<i64>,
    pub brief: Option<String>,
    /// `None` = not extracted yet.
    pub duration_sec: Option<f64>,
    /// `None` = not tried, `> 0` = mean frame rate, `0` = tried, none.
    pub frame_rate: Option<f64>,
    /// Same tri-state as `frame_rate`; written as a pair with `video_height`.
    pub video_width: Option<i64>,
    pub video_height: Option<i64>,
    pub created_at: f64,
    pub updated_at: f64,
    pub is_orphan: bool,
}

impl ClipRow {
    /// A row with the required fields; rating 0, every optional unset, not an orphan.
    pub fn new(clip_id: impl Into<String>, current_path: impl Into<String>, file_size: i64, file_mtime: f64, created_at: f64, updated_at: f64) -> ClipRow {
        ClipRow {
            clip_id: clip_id.into(),
            current_path: current_path.into(),
            file_size,
            file_mtime,
            rating: 0,
            bpm: None,
            mood: None,
            genre: None,
            energy: None,
            brief: None,
            duration_sec: None,
            frame_rate: None,
            video_width: None,
            video_height: None,
            created_at,
            updated_at,
            is_orphan: false,
        }
    }

    fn from_row(r: &Row<'_>) -> rusqlite::Result<ClipRow> {
        Ok(ClipRow {
            clip_id: r.get("clip_id")?,
            current_path: r.get("current_path")?,
            file_size: r.get("file_size")?,
            file_mtime: r.get("file_mtime")?,
            rating: r.get("rating")?,
            bpm: r.get("bpm")?,
            mood: r.get("mood")?,
            genre: r.get("genre")?,
            energy: r.get("energy")?,
            brief: r.get("brief")?,
            duration_sec: r.get("duration_sec")?,
            frame_rate: r.get("frame_rate")?,
            video_width: r.get("video_width")?,
            video_height: r.get("video_height")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
            is_orphan: r.get("is_orphan")?,
        })
    }
}

/// The grid cell's fields for one path (`ClipCellMetadata`); `tags` is `Some` (possibly empty) once fetched.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipCellMetadata {
    pub clip_id: Option<String>,
    pub duration_sec: Option<f64>,
    pub frame_rate: Option<f64>,
    pub video_width: Option<i64>,
    pub video_height: Option<i64>,
    pub rating: Option<i64>,
    pub bpm: Option<f64>,
    pub energy: Option<i64>,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub brief: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// One clip id with at least one row without a brief, and the row to brief it from (`UnbriefedEntry`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnbriefedEntry {
    pub clip_id: String,
    pub sample_path: String,
}

/// A live clip without a measurement row (`MeasurementRun.Item`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeasurementItem {
    pub path: String,
    pub clip_id: String,
}

/// What opening did beyond connecting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenReport {
    pub migrations: MigrationReport,
    /// `library.sqlite.pre-v8.bak` was written now.
    pub wrote_pre_v8_backup: bool,
    /// `library.sqlite.pre-measurements.bak` was written now.
    pub wrote_pre_measurements_backup: bool,
    /// The FTS index was empty with clips present and was rebuilt: the number of rows indexed.
    pub search_backfilled: Option<usize>,
}

type MountedVolumes = Box<dyn Fn() -> BTreeSet<String> + Send>;
type Clock = Box<dyn Fn() -> f64 + Send>;

/// MaCrate's library database (`MetadataStore`).
pub struct MetadataStore {
    conn: Connection,
    mounted_volumes: MountedVolumes,
    clock: Clock,
    report: OpenReport,
}

impl std::fmt::Debug for MetadataStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetadataStore").field("report", &self.report).finish_non_exhaustive()
    }
}

/// The entries of `/Volumes` as `/Volumes/<name>` (`MetadataStore.currentMountedVolumes`): a clip under
/// `/Volumes/<name>` is browsable only while `<name>` is mounted.
pub fn current_mounted_volumes() -> BTreeSet<String> {
    std::fs::read_dir("/Volumes").map(|rd| rd.flatten().filter_map(|e| e.file_name().to_str().map(|n| format!("/Volumes/{n}"))).collect()).unwrap_or_default()
}

/// Seconds since 1970, now (`Date().timeIntervalSince1970`).
pub fn now_seconds() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{name}{suffix}"))
}

/// `backupBeforeV8MigrationIfNeeded`: copy the library aside once, before v8 runs on it.
fn backup_before_v8(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let backup = sibling(path, ".pre-v8.bak");
    if backup.exists() {
        return Ok(false);
    }
    let applied = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .ok()
        .and_then(|probe| {
            probe.query_row("SELECT EXISTS(SELECT 1 FROM grdb_migrations WHERE identifier = 'v8_path_primary_key')", [], |r| r.get::<_, bool>(0)).ok()
        })
        .unwrap_or(false);
    if applied {
        return Ok(false);
    }
    std::fs::copy(path, &backup)?;
    Ok(true)
}

/// `MeasurementsStore.backupLibraryBeforeFirstAttachIfNeeded`.
fn backup_before_measurements(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let backup = sibling(path, ".pre-measurements.bak");
    if backup.exists() {
        return Ok(false);
    }
    std::fs::copy(path, &backup)?;
    Ok(true)
}

/// The FTS row of `row` (delete, then insert): `filename` is the last path component without its extension, `tags`
/// the tags joined by spaces, unset mood / genre / brief empty strings.
fn upsert_search_row(conn: &Connection, row: &ClipRow, tags: &[String]) -> Result<()> {
    conn.execute("DELETE FROM clip_search WHERE current_path = ?", [&row.current_path])?;
    conn.execute(
        "INSERT INTO clip_search (current_path, filename, tags, mood, genre, brief)\nVALUES (?, ?, ?, ?, ?, ?)",
        params![
            row.current_path,
            filename_token(&row.current_path),
            tags.join(" "),
            row.mood.as_deref().unwrap_or(""),
            row.genre.as_deref().unwrap_or(""),
            row.brief.as_deref().unwrap_or(""),
        ],
    )?;
    Ok(())
}

fn tags_at(conn: &Connection, path: &str) -> Result<Vec<String>> {
    strings(conn, "SELECT tag FROM clip_tags WHERE current_path = ? ORDER BY tag", &[&path])
}

fn strings(conn: &Connection, sql: &str, args: &[&dyn ToSql]) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(args, |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn clip_rows(conn: &Connection, sql: &str, args: &[&dyn ToSql]) -> Result<Vec<ClipRow>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(args, ClipRow::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn clip_row(conn: &Connection, sql: &str, args: &[&dyn ToSql]) -> Result<Option<ClipRow>> {
    Ok(conn.query_row(sql, args, ClipRow::from_row).optional()?)
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

fn prefix_range(prefix: &str) -> (String, String) {
    (format!("{prefix}/"), format!("{prefix}0"))
}

impl MetadataStore {
    /// Open as Swift's `MetadataStore.init` does, with `/Volumes` read for the mount check.
    pub fn open(location: Location) -> Result<MetadataStore> {
        MetadataStore::open_with(location, Box::new(current_mounted_volumes))
    }

    /// Open with a stand-in for the mounted volumes (tests, an injected set): create the folder, back the file up
    /// before v8 and before the first measurements attach (once each), connect, attach `meas` and make its tables,
    /// run the migrations, check foreign keys are on, and rebuild the FTS index if it is empty while clips exist.
    pub fn open_with(location: Location, mounted_volumes: MountedVolumes) -> Result<MetadataStore> {
        let mut report = OpenReport::default();
        let (conn, measurements_path) = match &location {
            Location::File(path) => {
                if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                    std::fs::create_dir_all(dir)?;
                }
                report.wrote_pre_v8_backup = backup_before_v8(path)?;
                report.wrote_pre_measurements_backup = backup_before_measurements(path)?;
                let measurements = path.with_file_name("measurements.sqlite").to_string_lossy().into_owned();
                (Connection::open(path)?, measurements)
            }
            Location::InMemory => (Connection::open_in_memory()?, ":memory:".to_string()),
        };
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
        conn.execute(&format!("ATTACH DATABASE ? AS {SCHEMA_NAME}"), [&measurements_path])?;
        measurements::migrate(&conn, SCHEMA_NAME)?;
        report.migrations = schema::migrate(&conn)?;
        let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
        if foreign_keys == 0 {
            return Err(LibraryError::ForeignKeysOff);
        }
        let mut store = MetadataStore { conn, mounted_volumes, clock: Box::new(now_seconds), report };
        store.report.search_backfilled = store.backfill_search_index_if_needed()?;
        Ok(store)
    }

    /// What opening did (migrations run, the v8 re-index flag, backups, FTS backfill).
    pub fn open_report(&self) -> &OpenReport {
        &self.report
    }

    /// Replace the clock used for `updated_at` / `dismissed_at` / `created_at` stamps (seconds since 1970).
    pub fn set_clock(&mut self, clock: Clock) {
        self.clock = clock;
    }

    /// The connection, for reads the store does not wrap (diagnostics).
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    fn now(&self) -> f64 {
        (self.clock)()
    }

    fn backfill_search_index_if_needed(&mut self) -> Result<Option<usize>> {
        let tx = self.conn.transaction()?;
        let search: i64 = tx.query_row("SELECT COUNT(*) FROM clip_search", [], |r| r.get(0))?;
        let clips: i64 = tx.query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))?;
        if search != 0 || clips <= 0 {
            tx.commit()?;
            return Ok(None);
        }
        let rows = clip_rows(&tx, "SELECT * FROM \"clips\"", &[])?;
        for row in &rows {
            let tags = tags_at(&tx, &row.current_path)?;
            upsert_search_row(&tx, row, &tags)?;
        }
        tx.commit()?;
        Ok(Some(rows.len()))
    }

    // MARK: clips

    /// `upsert(_:tags:)`: insert the row, or update every mapped column of the row at its path (never a REPLACE, so
    /// `dismissed_at` and the rowid stay), replace its tags, and rewrite its FTS row.
    pub fn upsert(&mut self, row: &ClipRow, tags: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO \"clips\" (\"clip_id\", \"current_path\", \"file_size\", \"file_mtime\", \"rating\", \"bpm\", \"mood\", \
             \"genre\", \"energy\", \"brief\", \"duration_sec\", \"frame_rate\", \"video_width\", \"video_height\", \
             \"created_at\", \"updated_at\", \"is_orphan\") VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) \
             ON CONFLICT(\"current_path\") DO UPDATE SET \"clip_id\" = excluded.\"clip_id\", \"file_size\" = excluded.\"file_size\", \
             \"file_mtime\" = excluded.\"file_mtime\", \"rating\" = excluded.\"rating\", \"bpm\" = excluded.\"bpm\", \
             \"mood\" = excluded.\"mood\", \"genre\" = excluded.\"genre\", \"energy\" = excluded.\"energy\", \
             \"brief\" = excluded.\"brief\", \"duration_sec\" = excluded.\"duration_sec\", \"frame_rate\" = excluded.\"frame_rate\", \
             \"video_width\" = excluded.\"video_width\", \"video_height\" = excluded.\"video_height\", \
             \"created_at\" = excluded.\"created_at\", \"updated_at\" = excluded.\"updated_at\", \"is_orphan\" = excluded.\"is_orphan\"",
            params![
                row.clip_id,
                row.current_path,
                row.file_size,
                row.file_mtime,
                row.rating,
                row.bpm,
                row.mood,
                row.genre,
                row.energy,
                row.brief,
                row.duration_sec,
                row.frame_rate,
                row.video_width,
                row.video_height,
                row.created_at,
                row.updated_at,
                row.is_orphan,
            ],
        )?;
        tx.execute("DELETE FROM clip_tags WHERE current_path = ?", [&row.current_path])?;
        for tag in tags {
            tx.execute("INSERT INTO clip_tags (current_path, tag) VALUES (?, ?)", params![row.current_path, tag])?;
        }
        upsert_search_row(&tx, row, tags)?;
        tx.commit()?;
        Ok(())
    }

    /// `fetch(clipID:)`: any one row with this content id.
    pub fn fetch_by_clip_id(&self, clip_id: &str) -> Result<Option<ClipRow>> {
        clip_row(&self.conn, "SELECT * FROM clips WHERE clip_id = ? LIMIT 1", &[&clip_id])
    }

    /// `fetch(at:)`: the row at a path.
    pub fn fetch_at(&self, path: &str) -> Result<Option<ClipRow>> {
        clip_row(&self.conn, "SELECT * FROM clips WHERE current_path = ?", &[&path])
    }

    /// `fetchByPath(_:)`.
    pub fn fetch_by_path(&self, path: &str) -> Result<Option<ClipRow>> {
        clip_row(&self.conn, "SELECT * FROM clips WHERE current_path = ? LIMIT 1", &[&path])
    }

    /// `fetchAll()`: every row, in table order.
    pub fn fetch_all(&self) -> Result<Vec<ClipRow>> {
        clip_rows(&self.conn, "SELECT * FROM \"clips\"", &[])
    }

    /// `fetchAll(underPathPrefix:)`: rows under `prefix/` (`[prefix + "/", prefix + "0")`, index-backed, no LIKE).
    pub fn fetch_all_under(&self, prefix: &str) -> Result<Vec<ClipRow>> {
        let (lo, hi) = prefix_range(prefix);
        clip_rows(&self.conn, "SELECT * FROM clips WHERE current_path >= ? AND current_path < ?", &[&lo, &hi])
    }

    /// `totalFileSize(underPathPrefix:)`: bytes of the non-orphan rows under `prefix/`.
    pub fn total_file_size(&self, prefix: &str) -> Result<i64> {
        let (lo, hi) = prefix_range(prefix);
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(file_size), 0) FROM clips\nWHERE current_path >= ? AND current_path < ? AND is_orphan = 0",
            params![lo, hi],
            |r| r.get(0),
        )?)
    }

    /// `hasAnyRow(underPathPrefix:)`.
    pub fn has_any_row(&self, prefix: &str) -> Result<bool> {
        let (lo, hi) = prefix_range(prefix);
        Ok(self.conn.query_row("SELECT EXISTS(SELECT 1 FROM clips WHERE current_path >= ? AND current_path < ?)", params![lo, hi], |r| r.get(0))?)
    }

    /// `delete(clipID:)`: every row with this content id, their tags (cascade) and FTS rows.
    pub fn delete_clip_id(&mut self, clip_id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let paths = strings(&tx, "SELECT current_path FROM clips WHERE clip_id = ?", &[&clip_id])?;
        tx.execute("DELETE FROM clips WHERE clip_id = ?", [clip_id])?;
        for p in &paths {
            tx.execute("DELETE FROM clip_search WHERE current_path = ?", [p])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// `deleteRow(at:)`: one row by path, its tags and FTS row.
    pub fn delete_row(&mut self, path: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM clips WHERE current_path = ?", [path])?;
        tx.execute("DELETE FROM clip_search WHERE current_path = ?", [path])?;
        tx.commit()?;
        Ok(())
    }

    /// `deleteRows(underPathPrefix:)`: every row under `prefix/`; returns their paths.
    pub fn delete_rows_under(&mut self, prefix: &str) -> Result<Vec<String>> {
        let (lo, hi) = prefix_range(prefix);
        let tx = self.conn.transaction()?;
        let paths = strings(&tx, "SELECT current_path FROM clips WHERE current_path >= ? AND current_path < ?", &[&lo, &hi])?;
        tx.execute("DELETE FROM clips WHERE current_path >= ? AND current_path < ?", params![lo, hi])?;
        for p in &paths {
            tx.execute("DELETE FROM clip_search WHERE current_path = ?", [p])?;
        }
        tx.commit()?;
        Ok(paths)
    }

    /// `retireOrphansWithLiveTwin()`: delete each orphan row whose content is live at another path; returns their
    /// paths.
    pub fn retire_orphans_with_live_twin(&mut self) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        let paths = strings(&tx, "SELECT current_path FROM clips\nWHERE is_orphan = 1\n  AND clip_id IN (SELECT clip_id FROM clips WHERE is_orphan = 0)", &[])?;
        for p in &paths {
            tx.execute("DELETE FROM clips WHERE current_path = ?", [p])?;
            tx.execute("DELETE FROM clip_search WHERE current_path = ?", [p])?;
        }
        tx.commit()?;
        Ok(paths)
    }

    fn set_orphan_where(&mut self, column: &str, key: &str, orphan: bool) -> Result<()> {
        let sql = if orphan {
            format!("UPDATE clips SET is_orphan = 1 WHERE {column} = ?")
        } else {
            format!("UPDATE clips SET is_orphan = 0, dismissed_at = NULL WHERE {column} = ?")
        };
        let tx = self.conn.transaction()?;
        tx.execute(&sql, [key])?;
        tx.commit()?;
        Ok(())
    }

    /// `setOrphan(clipID:_:)`: clearing also clears `dismissed_at`; marking keeps it.
    pub fn set_orphan_by_clip_id(&mut self, clip_id: &str, orphan: bool) -> Result<()> {
        self.set_orphan_where("clip_id", clip_id, orphan)
    }

    /// `setOrphan(at:_:)`.
    pub fn set_orphan_at(&mut self, path: &str, orphan: bool) -> Result<()> {
        self.set_orphan_where("current_path", path, orphan)
    }

    /// `clearDismissal(clipID:)`: dismissed orphan → active orphan.
    pub fn clear_dismissal_by_clip_id(&mut self, clip_id: &str) -> Result<()> {
        self.execute_one("UPDATE clips SET dismissed_at = NULL WHERE clip_id = ?", &[&clip_id])
    }

    /// `clearDismissal(at:)`.
    pub fn clear_dismissal_at(&mut self, path: &str) -> Result<()> {
        self.execute_one("UPDATE clips SET dismissed_at = NULL WHERE current_path = ?", &[&path])
    }

    fn execute_one(&mut self, sql: &str, args: &[&dyn ToSql]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(sql, args)?;
        tx.commit()?;
        Ok(())
    }

    /// `setPath(clipID:to:)`: rename any one row with this content id (a no-op when there is none).
    pub fn set_path_by_clip_id(&mut self, clip_id: &str, new_path: &str) -> Result<()> {
        let old: Option<String> = self.conn.query_row("SELECT current_path FROM clips WHERE clip_id = ? LIMIT 1", [clip_id], |r| r.get(0)).optional()?;
        match old {
            Some(old) => self.set_path(&old, new_path),
            None => Ok(()),
        }
    }

    /// `setPath(at:to:)`: move a row to a new path, `updated_at` = now; tags follow by `ON UPDATE CASCADE`; the FTS
    /// row at the old path is dropped and the new one written. Fails with a constraint error when `new_path` is taken.
    pub fn set_path(&mut self, old_path: &str, new_path: &str) -> Result<()> {
        let now = self.now();
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE clips SET current_path = ?, updated_at = ? WHERE current_path = ?", params![new_path, now, old_path])?;
        if let Some(updated) = clip_row(&tx, "SELECT * FROM clips WHERE current_path = ?", &[&new_path])? {
            let tags = tags_at(&tx, new_path)?;
            tx.execute("DELETE FROM clip_search WHERE current_path = ?", [old_path])?;
            upsert_search_row(&tx, &updated, &tags)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// `dismissAllOrphans()`: stamp `dismissed_at` = now on every active orphan; returns how many.
    pub fn dismiss_all_orphans(&mut self) -> Result<usize> {
        let now = self.now();
        let tx = self.conn.transaction()?;
        let n = tx.execute("UPDATE clips SET dismissed_at = ? WHERE is_orphan = 1 AND dismissed_at IS NULL", [now])?;
        tx.commit()?;
        Ok(n)
    }

    /// `cellMetadataByPath(_:)`: the grid fields of each path that has a row, tags fetched (sorted).
    pub fn cell_metadata_by_path(&self, paths: &[String]) -> Result<BTreeMap<String, ClipCellMetadata>> {
        let mut map = BTreeMap::new();
        if paths.is_empty() {
            return Ok(map);
        }
        let sql = format!(
            "SELECT current_path, clip_id, duration_sec, frame_rate, video_width, video_height,\n       rating, bpm, energy, mood, genre, brief\nFROM clips\nWHERE current_path IN ({})",
            placeholders(paths.len())
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(paths.iter()), |r| {
            Ok((
                r.get::<_, String>("current_path")?,
                ClipCellMetadata {
                    clip_id: r.get("clip_id")?,
                    duration_sec: r.get("duration_sec")?,
                    frame_rate: r.get("frame_rate")?,
                    video_width: r.get("video_width")?,
                    video_height: r.get("video_height")?,
                    rating: r.get("rating")?,
                    bpm: r.get("bpm")?,
                    energy: r.get("energy")?,
                    mood: r.get("mood")?,
                    genre: r.get("genre")?,
                    brief: r.get("brief")?,
                    tags: Some(Vec::new()),
                },
            ))
        })?;
        for row in rows {
            let (path, meta) = row?;
            map.insert(path, meta);
        }
        if !map.is_empty() {
            let found: Vec<&String> = map.keys().collect();
            let sql = format!("SELECT current_path, tag FROM clip_tags\nWHERE current_path IN ({})\nORDER BY tag", placeholders(found.len()));
            let mut stmt = self.conn.prepare(&sql)?;
            let tags = stmt
                .query_map(params_from_iter(found.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (path, tag) in tags {
                if let Some(list) = map.get_mut(&path).and_then(|m| m.tags.as_mut()) {
                    list.push(tag);
                }
            }
        }
        Ok(map)
    }

    fn pairs_map(&self, sql: &str, keys: &[String]) -> Result<BTreeMap<String, Vec<String>>> {
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params_from_iter(keys.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (k, v) = row?;
            map.entry(k).or_default().push(v);
        }
        Ok(map)
    }

    /// `tagsByClipID(_:)`: the union of the tags of every row with each content id.
    pub fn tags_by_clip_id(&self, clip_ids: &[String]) -> Result<BTreeMap<String, Vec<String>>> {
        if clip_ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        let sql = format!(
            "SELECT DISTINCT c.clip_id, t.tag\nFROM clip_tags t\nJOIN clips c ON c.current_path = t.current_path\nWHERE c.clip_id IN ({})\nORDER BY t.tag",
            placeholders(clip_ids.len())
        );
        self.pairs_map(&sql, clip_ids)
    }

    /// `tagsByPath(_:)`.
    pub fn tags_by_path(&self, paths: &[String]) -> Result<BTreeMap<String, Vec<String>>> {
        if paths.is_empty() {
            return Ok(BTreeMap::new());
        }
        let sql = format!("SELECT current_path, tag FROM clip_tags\nWHERE current_path IN ({})\nORDER BY tag", placeholders(paths.len()));
        self.pairs_map(&sql, paths)
    }

    /// `setDuration(clipID:_:)`: derived data, no `updated_at` bump, no FTS.
    pub fn set_duration(&mut self, clip_id: &str, duration_sec: Option<f64>) -> Result<()> {
        self.execute_one("UPDATE clips SET duration_sec = ? WHERE clip_id = ?", &[&duration_sec, &clip_id])
    }

    /// `setFrameRate(clipID:_:)` (0 = tried, none).
    pub fn set_frame_rate(&mut self, clip_id: &str, frame_rate: f64) -> Result<()> {
        self.execute_one("UPDATE clips SET frame_rate = ? WHERE clip_id = ?", &[&frame_rate, &clip_id])
    }

    /// `setVideoSize(clipID:width:height:)` (0, 0 = tried, none).
    pub fn set_video_size(&mut self, clip_id: &str, width: i64, height: i64) -> Result<()> {
        self.execute_one("UPDATE clips SET video_width = ?, video_height = ? WHERE clip_id = ?", &[&width, &height, &clip_id])
    }

    // MARK: Alpha Type

    /// `alphaInfo(clipID:)`: the chosen Alpha Type (`None` = default) and the cached probe string.
    pub fn alpha_info(&self, clip_id: &str) -> Result<(Option<AlphaView>, Option<String>)> {
        let row: Option<(Option<String>, Option<String>)> =
            self.conn.query_row("SELECT alpha_type, probe FROM clip_alpha WHERE clip_id = ?", [clip_id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        Ok(match row {
            Some((alpha, probe)) => (alpha.as_deref().and_then(AlphaView::from_stored_name), probe),
            None => (None, None),
        })
    }

    /// `alphaType(forPath:)`: the choice for the content at a path.
    pub fn alpha_type_for_path(&self, path: &str) -> Result<Option<AlphaView>> {
        let raw: Option<Option<String>> = self
            .conn
            .query_row("SELECT a.alpha_type FROM clips c JOIN clip_alpha a ON a.clip_id = c.clip_id\nWHERE c.current_path = ?", [path], |r| r.get(0))
            .optional()?;
        Ok(raw.flatten().as_deref().and_then(AlphaView::from_stored_name))
    }

    /// `currentPaths(clipID:)`: every path holding this content, sorted.
    pub fn current_paths(&self, clip_id: &str) -> Result<Vec<String>> {
        strings(&self.conn, "SELECT current_path FROM clips WHERE clip_id = ? ORDER BY current_path", &[&clip_id])
    }

    /// `setAlphaType(clipID:_:)`.
    pub fn set_alpha_type(&mut self, clip_id: &str, alpha: AlphaView) -> Result<()> {
        let now = self.now();
        self.execute_one(
            "INSERT INTO clip_alpha (clip_id, alpha_type, updated_at) VALUES (?, ?, ?)\nON CONFLICT(clip_id) DO UPDATE SET alpha_type = excluded.alpha_type, updated_at = excluded.updated_at",
            &[&clip_id, &alpha.stored_name(), &now],
        )
    }

    /// `setAlphaProbe(clipID:_:)` (an `AlphaProbe::encode` string); never changes the choice.
    pub fn set_alpha_probe(&mut self, clip_id: &str, probe: &str) -> Result<()> {
        let now = self.now();
        self.execute_one(
            "INSERT INTO clip_alpha (clip_id, probe, updated_at) VALUES (?, ?, ?)\nON CONFLICT(clip_id) DO UPDATE SET probe = excluded.probe, updated_at = excluded.updated_at",
            &[&clip_id, &probe, &now],
        )
    }

    // MARK: presets

    /// `savePreset(name:filter:sort:)`: under the trimmed name, replacing the filter and sort of an existing preset
    /// of that name (its `created_at` stays).
    pub fn save_preset(&mut self, name: &str, filter: &ClipFilter, sort: Option<ClipSort>) -> Result<()> {
        let trimmed = trim_whitespaces_and_newlines(name);
        if trimmed.is_empty() {
            return Err(PresetError::EmptyName.into());
        }
        let filter_json = filter.to_json()?;
        let sort_raw = sort.map_or("", ClipSort::raw_value);
        let now = self.now();
        self.execute_one(
            "INSERT INTO saved_filters (name, filter_json, sort_value, created_at)\nVALUES (?, ?, ?, ?)\nON CONFLICT(name) DO UPDATE SET\n    filter_json = excluded.filter_json,\n    sort_value  = excluded.sort_value",
            &[&trimmed, &filter_json, &sort_raw, &now],
        )
    }

    /// `listPresets()`: oldest first; an undecodable filter reads as the empty filter, an unknown sort as none.
    pub fn list_presets(&self) -> Result<Vec<SavedPreset>> {
        let mut stmt = self.conn.prepare("SELECT name, filter_json, sort_value, created_at\nFROM saved_filters\nORDER BY created_at ASC")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, f64>(3)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (name, filter_json, sort_value, created_at) = row?;
            out.push(SavedPreset {
                name,
                filter: ClipFilter::from_json(filter_json.as_bytes()).unwrap_or_default(),
                sort: if sort_value.is_empty() { None } else { ClipSort::from_raw_value(&sort_value) },
                created_at,
            });
        }
        Ok(out)
    }

    /// `deletePreset(name:)`.
    pub fn delete_preset(&mut self, name: &str) -> Result<()> {
        self.execute_one("DELETE FROM saved_filters WHERE name = ?", &[&name])
    }

    /// `renamePreset(from:to:)`: trimmed new name; the same name is a no-op; a missing preset or a taken name is a
    /// typed error.
    pub fn rename_preset(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        let trimmed = trim_whitespaces_and_newlines(new_name);
        if trimmed.is_empty() {
            return Err(PresetError::EmptyName.into());
        }
        if trimmed == old_name {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        let exists = |name: &str| -> Result<bool> { Ok(tx.query_row("SELECT EXISTS(SELECT 1 FROM saved_filters WHERE name = ?)", [name], |r| r.get(0))?) };
        if !exists(old_name)? {
            return Err(PresetError::NotFound(old_name.to_string()).into());
        }
        if exists(trimmed)? {
            return Err(PresetError::DuplicateName(trimmed.to_string()).into());
        }
        tx.execute("UPDATE saved_filters SET name = ? WHERE name = ?", params![trimmed, old_name])?;
        tx.commit()?;
        Ok(())
    }

    // MARK: stashes (the mirror of `.crate-stashes.json`)

    /// `upsertStash(_:)`: the stash record (name and `updated_at` replaced, `created_at` kept) and all its entries,
    /// `position` = order in the file.
    pub fn upsert_stash(&mut self, stash: &Stash) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO stashes (id, name, created_at, updated_at)\nVALUES (?, ?, ?, ?)\nON CONFLICT(id) DO UPDATE SET\n    name = excluded.name,\n    updated_at = excluded.updated_at",
            params![stash.id, stash.name, stash.created_at, stash.updated_at],
        )?;
        tx.execute("DELETE FROM stash_clips WHERE stash_id = ?", [&stash.id])?;
        for (i, entry) in stash.clips.iter().enumerate() {
            tx.execute(
                "INSERT INTO stash_clips (stash_id, clip_id, relative_path_hint, position)\nVALUES (?, ?, ?, ?)",
                params![stash.id, entry.clip_id, entry.relative_path_hint, i as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// `listStashes()`: by name, entries in position order.
    pub fn list_stashes(&self) -> Result<Vec<Stash>> {
        let mut by_stash: HashMap<String, Vec<StashEntry>> = HashMap::new();
        let mut stmt = self.conn.prepare("SELECT stash_id, clip_id, relative_path_hint\nFROM stash_clips\nORDER BY stash_id, position ASC")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (sid, clip_id, relative_path_hint) = row?;
            by_stash.entry(sid).or_default().push(StashEntry { clip_id, relative_path_hint });
        }
        let mut stmt = self.conn.prepare("SELECT id, name, created_at, updated_at FROM stashes ORDER BY name ASC")?;
        let stashes = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?, r.get::<_, f64>(3)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(stashes
            .into_iter()
            .map(|(id, name, created_at, updated_at)| {
                let clips = by_stash.remove(&id).unwrap_or_default();
                Stash { id, name, created_at, updated_at, clips }
            })
            .collect())
    }

    /// `renameStash(id:newName:updatedAt:)`.
    pub fn rename_stash(&mut self, id: &str, new_name: &str, updated_at: f64) -> Result<()> {
        self.execute_one("UPDATE stashes SET name = ?, updated_at = ? WHERE id = ?", &[&new_name, &updated_at, &id])
    }

    /// `deleteStash(id:)`: the record; its entries go by `ON DELETE CASCADE`.
    pub fn delete_stash(&mut self, id: &str) -> Result<()> {
        self.execute_one("DELETE FROM stashes WHERE id = ?", &[&id])
    }

    // MARK: orphans

    /// `countOrphans()`: active (not dismissed) orphans.
    pub fn count_orphans(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM clips WHERE is_orphan = 1 AND dismissed_at IS NULL", [], |r| r.get(0))?)
    }

    /// `fetchOrphans()`: active orphans by path.
    pub fn fetch_orphans(&self) -> Result<Vec<ClipRow>> {
        clip_rows(&self.conn, "SELECT * FROM clips WHERE is_orphan = 1 AND dismissed_at IS NULL ORDER BY current_path", &[])
    }

    /// `countDismissedOrphans()`.
    pub fn count_dismissed_orphans(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM clips WHERE is_orphan = 1 AND dismissed_at IS NOT NULL", [], |r| r.get(0))?)
    }

    /// `fetchDismissedOrphans()`: most recently dismissed first, then by path.
    pub fn fetch_dismissed_orphans(&self) -> Result<Vec<ClipRow>> {
        clip_rows(&self.conn, "SELECT * FROM clips\nWHERE is_orphan = 1 AND dismissed_at IS NOT NULL\nORDER BY dismissed_at DESC, current_path ASC", &[])
    }

    // MARK: tags and autocomplete

    /// `tags(for:)`: the union of the tags of every row with this content id.
    pub fn tags_for_clip_id(&self, clip_id: &str) -> Result<Vec<String>> {
        strings(
            &self.conn,
            "SELECT DISTINCT t.tag FROM clip_tags t\nJOIN clips c ON c.current_path = t.current_path\nWHERE c.clip_id = ?\nORDER BY t.tag",
            &[&clip_id],
        )
    }

    /// `tags(forPath:)`.
    pub fn tags_for_path(&self, path: &str) -> Result<Vec<String>> {
        tags_at(&self.conn, path)
    }

    /// `clipIDs(withTag:)`.
    pub fn clip_ids_with_tag(&self, tag: &str) -> Result<Vec<String>> {
        strings(
            &self.conn,
            "SELECT DISTINCT c.clip_id\nFROM clip_tags t\nJOIN clips c ON c.current_path = t.current_path\nWHERE t.tag = ?\nORDER BY c.clip_id",
            &[&tag],
        )
    }

    /// `allTags()`.
    pub fn all_tags(&self) -> Result<Vec<String>> {
        strings(&self.conn, "SELECT DISTINCT tag FROM clip_tags ORDER BY tag", &[])
    }

    /// `allMoods()`.
    pub fn all_moods(&self) -> Result<Vec<String>> {
        strings(&self.conn, "SELECT DISTINCT mood FROM clips WHERE mood IS NOT NULL AND mood != '' ORDER BY mood", &[])
    }

    /// `allGenres()`.
    pub fn all_genres(&self) -> Result<Vec<String>> {
        strings(&self.conn, "SELECT DISTINCT genre FROM clips WHERE genre IS NOT NULL AND genre != '' ORDER BY genre", &[])
    }

    // MARK: search

    /// `searchPaths(query:)`: paths matching the sanitised query, best match first (FTS5 rank).
    pub fn search_paths(&self, query: &str) -> Result<Vec<String>> {
        let expression = sanitize_search_query(query);
        if expression.is_empty() {
            return Ok(Vec::new());
        }
        strings(&self.conn, "SELECT current_path FROM clip_search\nWHERE clip_search MATCH ?\nORDER BY rank", &[&expression])
    }

    /// `searchClipIDs(filenameQuery:)`: the content ids of `search_paths`, in rank order. (Swift traps when two
    /// matching paths differ only by Unicode normalization; here each path is its own key.)
    pub fn search_clip_ids(&self, query: &str) -> Result<Vec<String>> {
        let paths = self.search_paths(query)?;
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!("SELECT current_path, clip_id FROM clips WHERE current_path IN ({})", placeholders(paths.len()));
        let mut stmt = self.conn.prepare(&sql)?;
        let by_path: HashMap<String, String> =
            stmt.query_map(params_from_iter(paths.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(paths.iter().filter_map(|p| by_path.get(p).cloned()).collect())
    }

    /// `fetchRows(clipIDs:)`: one (any) row per content id.
    pub fn fetch_rows_by_clip_ids(&self, clip_ids: &[String]) -> Result<Vec<ClipRow>> {
        if clip_ids.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!("SELECT * FROM clips WHERE clip_id IN ({})", placeholders(clip_ids.len()));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(clip_ids.iter()), ClipRow::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut seen = HashSet::new();
        Ok(rows.into_iter().filter(|r| seen.insert(r.clip_id.clone())).collect())
    }

    /// `fetchRows(paths:)`.
    pub fn fetch_rows_by_paths(&self, paths: &[String]) -> Result<Vec<ClipRow>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!("SELECT * FROM clips WHERE current_path IN ({})", placeholders(paths.len()));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(paths.iter()), ClipRow::from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // MARK: duplicates

    /// `clipIDsWithDuplicates()`: content ids at more than one path.
    pub fn clip_ids_with_duplicates(&self) -> Result<Vec<String>> {
        strings(&self.conn, "SELECT clip_id FROM clips\nGROUP BY clip_id HAVING COUNT(*) > 1", &[])
    }

    /// `fetchDuplicates(of:excluding:)`.
    pub fn fetch_duplicates(&self, clip_id: &str, excluding: &str) -> Result<Vec<ClipRow>> {
        clip_rows(&self.conn, "SELECT * FROM clips WHERE clip_id = ? AND current_path != ? ORDER BY current_path", &[&clip_id, &excluding])
    }

    /// `fetchRows(forClipID:)`: every row with this content id, by path.
    pub fn fetch_rows_for_clip_id(&self, clip_id: &str) -> Result<Vec<ClipRow>> {
        clip_rows(&self.conn, "SELECT * FROM clips WHERE clip_id = ? ORDER BY current_path", &[&clip_id])
    }

    // MARK: briefs

    /// `unbriefedDistinctCount(underPathPrefix:)`.
    pub fn unbriefed_distinct_count_under(&self, prefix: &str) -> Result<i64> {
        let (lo, hi) = prefix_range(prefix);
        Ok(self.conn.query_row(
            "SELECT COUNT(DISTINCT clip_id) FROM clips\nWHERE is_orphan = 0\n  AND (brief IS NULL OR TRIM(brief) = '')\n  AND current_path >= ? AND current_path < ?",
            params![lo, hi],
            |r| r.get(0),
        )?)
    }

    /// `unbriefedDistinctCount(in:)`.
    pub fn unbriefed_distinct_count_in(&self, paths: &[String]) -> Result<i64> {
        if paths.is_empty() {
            return Ok(0);
        }
        let sql = format!(
            "SELECT COUNT(DISTINCT clip_id) FROM clips\nWHERE current_path IN ({})\n  AND (brief IS NULL OR TRIM(brief) = '')",
            placeholders(paths.len())
        );
        Ok(self.conn.query_row(&sql, params_from_iter(paths.iter()), |r| r.get(0))?)
    }

    fn unbriefed_entries(&self, sql: &str, args: Vec<String>) -> Result<Vec<UnbriefedEntry>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt
            .query_map(params_from_iter(args.iter()), |r| Ok(UnbriefedEntry { clip_id: r.get("clip_id")?, sample_path: r.get("sample_path")? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// `unbriefedDistinctEntries(underPathPrefix:)`: one entry per content id, the lowest path as the sample, the
    /// most recently updated groups first.
    pub fn unbriefed_distinct_entries_under(&self, prefix: &str) -> Result<Vec<UnbriefedEntry>> {
        let (lo, hi) = prefix_range(prefix);
        self.unbriefed_entries(
            "SELECT clip_id, MIN(current_path) AS sample_path\nFROM clips\nWHERE is_orphan = 0\n  AND (brief IS NULL OR TRIM(brief) = '')\n  AND current_path >= ? AND current_path < ?\nGROUP BY clip_id\nORDER BY MAX(updated_at) DESC",
            vec![lo, hi],
        )
    }

    /// `unbriefedDistinctEntries(in:)`.
    pub fn unbriefed_distinct_entries_in(&self, paths: &[String]) -> Result<Vec<UnbriefedEntry>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT clip_id, MIN(current_path) AS sample_path\nFROM clips\nWHERE current_path IN ({})\n  AND (brief IS NULL OR TRIM(brief) = '')\nGROUP BY clip_id\nORDER BY MAX(updated_at) DESC",
            placeholders(paths.len())
        );
        self.unbriefed_entries(&sql, paths.to_vec())
    }

    fn apply_brief_to_all_rows_sharing_impl(&mut self, clip_id: &str, brief: &str, overwrite: bool) -> Result<Vec<String>> {
        let now = self.now();
        let (select, update) = if overwrite {
            ("SELECT current_path FROM clips WHERE clip_id = ?", "UPDATE clips SET brief = ?, updated_at = ?\nWHERE clip_id = ?")
        } else {
            (
                "SELECT current_path FROM clips\nWHERE clip_id = ?\n  AND (brief IS NULL OR TRIM(brief) = '')",
                "UPDATE clips SET brief = ?, updated_at = ?\nWHERE clip_id = ?\n  AND (brief IS NULL OR TRIM(brief) = '')",
            )
        };
        let tx = self.conn.transaction()?;
        let paths = strings(&tx, select, &[&clip_id])?;
        if paths.is_empty() {
            tx.commit()?;
            return Ok(paths);
        }
        tx.execute(update, params![brief, now, clip_id])?;
        for path in &paths {
            let Some(row) = clip_row(&tx, "SELECT * FROM clips WHERE current_path = ?", &[path])? else { continue };
            let tags = tags_at(&tx, path)?;
            upsert_search_row(&tx, &row, &tags)?;
        }
        tx.commit()?;
        Ok(paths)
    }

    /// `applyBriefToAllRowsSharing(clipID:brief:)`: fill every blank brief (NULL or spaces) of this content, stamp
    /// `updated_at`, rewrite their FTS rows; returns the paths changed.
    pub fn apply_brief_to_all_rows_sharing(&mut self, clip_id: &str, brief: &str) -> Result<Vec<String>> {
        self.apply_brief_to_all_rows_sharing_impl(clip_id, brief, false)
    }

    /// `overwriteBriefForAllRowsSharing(clipID:brief:)`: the same, replacing existing briefs too.
    pub fn overwrite_brief_for_all_rows_sharing(&mut self, clip_id: &str, brief: &str) -> Result<Vec<String>> {
        self.apply_brief_to_all_rows_sharing_impl(clip_id, brief, true)
    }

    /// `overwriteBriefAt(path:brief:overwriteExisting:)`: one row; without `overwrite`, a brief that is not blank
    /// (after trimming whitespace and newlines) is kept. True when written.
    pub fn overwrite_brief_at(&mut self, path: &str, brief: &str, overwrite: bool) -> Result<bool> {
        let now = self.now();
        let tx = self.conn.transaction()?;
        let Some(mut row) = clip_row(&tx, "SELECT * FROM clips WHERE current_path = ?", &[&path])? else {
            tx.commit()?;
            return Ok(false);
        };
        if !overwrite && row.brief.as_deref().is_some_and(|b| !trim_whitespaces_and_newlines(b).is_empty()) {
            tx.commit()?;
            return Ok(false);
        }
        tx.execute("UPDATE clips SET brief = ?, updated_at = ? WHERE current_path = ?", params![brief, now, path])?;
        row.brief = Some(brief.to_string());
        row.updated_at = now;
        let tags = tags_at(&tx, path)?;
        upsert_search_row(&tx, &row, &tags)?;
        tx.commit()?;
        Ok(true)
    }

    // MARK: measurements

    /// `measurement(clipID:)`: `None` = not measured.
    pub fn measurement(&self, clip_id: &str) -> Result<Option<ClipMeasurement>> {
        measurements::measurement(&self.conn, SCHEMA_NAME, clip_id)
    }

    /// `unmeasuredClips()` (`MeasurementRun.unmeasuredSQL`): live rows with no measurement and no skip by the
    /// current scanner version, by path.
    pub fn unmeasured_clips(&self) -> Result<Vec<MeasurementItem>> {
        let sql = format!(
            "SELECT c.current_path, c.clip_id FROM clips c\nLEFT JOIN {SCHEMA_NAME}.clip_measurements m ON m.clip_id = c.clip_id\n\
             WHERE m.clip_id IS NULL AND c.is_orphan = 0\n  AND NOT EXISTS (SELECT 1 FROM {SCHEMA_NAME}.scan_skips s\n\
             WHERE s.clip_id = c.clip_id AND s.scanner_version = ?)\nORDER BY c.current_path"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map([measurements::SCANNER_MEASURE_VERSION], |r| Ok(MeasurementItem { path: r.get(0)?, clip_id: r.get(1)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// `unmeasuredClips(paths:)`: the live rows at `paths` with no measurement row (500 paths per query), by path.
    pub fn unmeasured_clips_at(&self, paths: &[String]) -> Result<Vec<MeasurementItem>> {
        let mut out = Vec::new();
        for chunk in paths.chunks(500) {
            let sql = format!(
                "SELECT c.current_path, c.clip_id FROM clips c\nLEFT JOIN {SCHEMA_NAME}.clip_measurements m ON m.clip_id = c.clip_id\nWHERE m.clip_id IS NULL AND c.is_orphan = 0 AND c.current_path IN ({})",
                placeholders(chunk.len())
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params_from_iter(chunk.iter()), |r| Ok(MeasurementItem { path: r.get(0)?, clip_id: r.get(1)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            out.extend(rows);
        }
        // Swift's `<` on String: canonically equivalent paths (an NFC and an NFD spelling) compare equal and keep
        // their order.
        out.sort_by_cached_key(|i| crate::unicode::file_system_representation(&i.path));
        Ok(out)
    }

    /// `insertNativeMeasurement(_:)`: one native-scanner row, insert only (an `unsupported-native` row becomes a
    /// `scan_skips` row). True when a measurement row was written.
    pub fn insert_native_measurement(&mut self, row: &ClipMeasurement) -> Result<bool> {
        let now = self.now();
        let tx = self.conn.transaction()?;
        let n = measurements::insert_native(&tx, std::slice::from_ref(row), &|| measurements::iso8601(now))?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// The lab importer's write through the library connection (`INSERT OR REPLACE`).
    pub fn insert_lab_measurements(&mut self, rows: &[ClipMeasurement]) -> Result<()> {
        let tx = self.conn.transaction()?;
        measurements::insert_lab(&tx, rows)?;
        tx.commit()?;
        Ok(())
    }

    // MARK: composite query

    /// `query(search:filter:sort:)`: the content ids of the plan's rows, first occurrence kept.
    pub fn query(&self, search: &str, filter: &ClipFilter, sort: ClipSort) -> Result<Vec<String>> {
        let volumes = (self.mounted_volumes)();
        let plan = query::build_query_plan(search, filter, sort, Some(&volumes));
        let ids = self.run_plan(&plan)?;
        let mut seen = HashSet::new();
        Ok(ids.into_iter().filter(|id| seen.insert(id.clone())).collect())
    }

    /// `queryPaths(search:filter:sort:)`: one path per matching row (duplicates are their own rows).
    pub fn query_paths(&self, search: &str, filter: &ClipFilter, sort: ClipSort) -> Result<Vec<String>> {
        let volumes = (self.mounted_volumes)();
        let plan = query::build_query_plan_for_paths(search, filter, sort, Some(&volumes));
        self.run_plan(&plan)
    }

    fn run_plan(&self, plan: &QueryPlan) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(&plan.sql)?;
        let rows = stmt.query_map(params_from_iter(plan.args.iter()), |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
