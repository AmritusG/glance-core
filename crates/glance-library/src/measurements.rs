//! `measurements.sqlite`, attached to every library connection as `meas` (Swift MaCrate's `MeasurementsStore`):
//! its idempotent schema, the row type, and the native-scanner / lab writes.

use crate::error::Result;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::Path;
use std::time::Duration;

/// The schema name the library attaches the store under.
pub const SCHEMA_NAME: &str = "meas";

/// `MeasurementsStore.Provenance`.
pub const PROVENANCE_LAB: &str = "lab";
pub const PROVENANCE_NATIVE: &str = "native";

/// `MeasurementScanner.measureVersion`, `.source` and `.unsupportedStatus` (Swift MaCrate 0.16.4).
pub const SCANNER_MEASURE_VERSION: &str = "native v2 (wave 98 rules)";
pub const SCANNER_SOURCE: &str = "native scanner";
pub const UNSUPPORTED_STATUS: &str = "unsupported-native";

/// `MeasurementsStore.Threshold`.
pub const ON_BLACK: f64 = 0.5;
pub const CENTRE_HEAVY: f64 = 0.40;
pub const JOIN_STEP_FLAG: f64 = 2.5;

/// `MeasurementsStore.createTableSQL(schema:)`.
pub fn create_table_sql(schema: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {schema}.clip_measurements (
    clip_id         TEXT PRIMARY KEY NOT NULL,
    lab_key         TEXT NOT NULL,
    lab_status      TEXT NOT NULL,
    lab_reason      TEXT,
    pass1_score     REAL,
    is_loop         INTEGER,
    cycle_s         REAL,
    cycle_rule      TEXT,
    approx_cycle    INTEGER,
    join_step       REAL,
    cuts            INTEGER,
    flashes         INTEGER,
    flicker         REAL,
    flow_mean       REAL,
    moves           INTEGER,
    text_checked    INTEGER NOT NULL DEFAULT 0,
    text_found      INTEGER,
    black_share     REAL,
    centre_share    REAL,
    xfade_score     REAL,
    measure_version TEXT,
    source          TEXT,
    measured_at     TEXT,
    provenance      TEXT NOT NULL DEFAULT 'lab'
)"
    )
}

/// `MeasurementsStore.createSkipsTableSQL(schema:)`.
pub fn create_skips_table_sql(schema: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {schema}.scan_skips (
    clip_id         TEXT PRIMARY KEY NOT NULL,
    reason          TEXT NOT NULL,
    scanner_version TEXT NOT NULL,
    skipped_at      TEXT NOT NULL
)"
    )
}

/// `MeasurementsStore.migrate(_:schema:)`: create the table, add `provenance` to a table that predates it (existing
/// rows read `lab`), create `scan_skips`. Idempotent; run on every library connection.
pub fn migrate(conn: &Connection, schema: &str) -> Result<()> {
    conn.execute_batch(&create_table_sql(schema))?;
    let mut stmt = conn.prepare(&format!("PRAGMA {schema}.table_info(clip_measurements)"))?;
    let columns: Vec<String> = stmt.query_map([], |r| r.get::<_, String>("name"))?.collect::<rusqlite::Result<_>>()?;
    if !columns.iter().any(|c| c == "provenance") {
        conn.execute_batch(&format!("ALTER TABLE {schema}.clip_measurements\nADD COLUMN provenance TEXT NOT NULL DEFAULT '{PROVENANCE_LAB}'"))?;
    }
    conn.execute_batch(&create_skips_table_sql(schema))?;
    Ok(())
}

/// `MeasurementsStore.openForWriting(at:)`: the store as a standalone database (the importer / CLI connection):
/// directory created, busy timeout 10 s, schema migrated as `main`.
pub fn open_for_writing(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(10))?;
    conn.execute_batch("PRAGMA foreign_keys = ON")?;
    migrate(&conn, "main")?;
    Ok(conn)
}

/// One `clip_measurements` row (`ClipMeasurement`). `None` is "no reading", never 0.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipMeasurement {
    pub clip_id: String,
    pub lab_key: String,
    pub lab_status: String,
    pub lab_reason: Option<String>,
    pub pass1_score: Option<f64>,
    pub is_loop: Option<i64>,
    pub cycle_s: Option<f64>,
    pub cycle_rule: Option<String>,
    pub approx_cycle: Option<i64>,
    pub join_step: Option<f64>,
    pub cuts: Option<i64>,
    pub flashes: Option<i64>,
    pub flicker: Option<f64>,
    pub flow_mean: Option<f64>,
    pub moves: Option<i64>,
    pub text_checked: i64,
    pub text_found: Option<i64>,
    pub black_share: Option<f64>,
    pub centre_share: Option<f64>,
    pub xfade_score: Option<f64>,
    pub measure_version: Option<String>,
    pub source: Option<String>,
    pub measured_at: Option<String>,
    pub provenance: String,
}

impl ClipMeasurement {
    /// A row with only the required fields; every reading unset, `text_checked` 0, provenance `lab`.
    pub fn new(clip_id: impl Into<String>, lab_key: impl Into<String>, lab_status: impl Into<String>) -> ClipMeasurement {
        ClipMeasurement {
            clip_id: clip_id.into(),
            lab_key: lab_key.into(),
            lab_status: lab_status.into(),
            lab_reason: None,
            pass1_score: None,
            is_loop: None,
            cycle_s: None,
            cycle_rule: None,
            approx_cycle: None,
            join_step: None,
            cuts: None,
            flashes: None,
            flicker: None,
            flow_mean: None,
            moves: None,
            text_checked: 0,
            text_found: None,
            black_share: None,
            centre_share: None,
            xfade_score: None,
            measure_version: None,
            source: None,
            measured_at: None,
            provenance: PROVENANCE_LAB.to_string(),
        }
    }

    pub(crate) fn from_row(r: &Row<'_>) -> rusqlite::Result<ClipMeasurement> {
        Ok(ClipMeasurement {
            clip_id: r.get("clip_id")?,
            lab_key: r.get("lab_key")?,
            lab_status: r.get("lab_status")?,
            lab_reason: r.get("lab_reason")?,
            pass1_score: r.get("pass1_score")?,
            is_loop: r.get("is_loop")?,
            cycle_s: r.get("cycle_s")?,
            cycle_rule: r.get("cycle_rule")?,
            approx_cycle: r.get("approx_cycle")?,
            join_step: r.get("join_step")?,
            cuts: r.get("cuts")?,
            flashes: r.get("flashes")?,
            flicker: r.get("flicker")?,
            flow_mean: r.get("flow_mean")?,
            moves: r.get("moves")?,
            text_checked: r.get("text_checked")?,
            text_found: r.get("text_found")?,
            black_share: r.get("black_share")?,
            centre_share: r.get("centre_share")?,
            xfade_score: r.get("xfade_score")?,
            measure_version: r.get("measure_version")?,
            source: r.get("source")?,
            measured_at: r.get("measured_at")?,
            provenance: r.get("provenance")?,
        })
    }

    /// `INSERT [OR …] INTO clip_measurements (<every column>) VALUES (…)`, as GRDB's `insert(db, onConflict:)`.
    fn insert(&self, conn: &Connection, conflict: &str) -> Result<usize> {
        let sql = format!(
            "INSERT OR {conflict} INTO \"clip_measurements\" (\"clip_id\", \"lab_key\", \"lab_status\", \"lab_reason\", \
             \"pass1_score\", \"is_loop\", \"cycle_s\", \"cycle_rule\", \"approx_cycle\", \"join_step\", \"cuts\", \"flashes\", \
             \"flicker\", \"flow_mean\", \"moves\", \"text_checked\", \"text_found\", \"black_share\", \"centre_share\", \
             \"xfade_score\", \"measure_version\", \"source\", \"measured_at\", \"provenance\") \
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        );
        Ok(conn.execute(
            &sql,
            params![
                self.clip_id,
                self.lab_key,
                self.lab_status,
                self.lab_reason,
                self.pass1_score,
                self.is_loop,
                self.cycle_s,
                self.cycle_rule,
                self.approx_cycle,
                self.join_step,
                self.cuts,
                self.flashes,
                self.flicker,
                self.flow_mean,
                self.moves,
                self.text_checked,
                self.text_found,
                self.black_share,
                self.centre_share,
                self.xfade_score,
                self.measure_version,
                self.source,
                self.measured_at,
                self.provenance,
            ],
        )?)
    }
}

/// `MeasurementsStore.insertNative(_:db:)`: native-scanner rows, insert only (an existing lab or native row is left
/// as it is), provenance `native`; an `unsupported-native` result goes to `scan_skips` instead. Returns the rows
/// inserted. Works wherever `clip_measurements` resolves to the store (`main`, or `meas` through the library).
pub fn insert_native(conn: &Connection, rows: &[ClipMeasurement], now_iso8601: &dyn Fn() -> String) -> Result<usize> {
    let mut n = 0;
    for row in rows {
        if row.lab_status == UNSUPPORTED_STATUS {
            record_skip(conn, row, now_iso8601)?;
            continue;
        }
        let mut native = row.clone();
        native.provenance = PROVENANCE_NATIVE.to_string();
        n += native.insert(conn, "IGNORE")?;
    }
    Ok(n)
}

/// The lab importer's write: `INSERT OR REPLACE` of whole rows (as `row.insert(db, onConflict: .replace)`).
pub fn insert_lab(conn: &Connection, rows: &[ClipMeasurement]) -> Result<()> {
    for row in rows {
        row.insert(conn, "REPLACE")?;
    }
    Ok(())
}

/// `MeasurementsStore.recordSkip(_:db:)`: note an `unsupported-native` result (replacing an older skip):
/// reason = the row's `lab_reason` or `unsupported-native`, scanner version = its `measure_version` or the
/// current scanner's, time = its `measured_at` or now.
pub fn record_skip(conn: &Connection, row: &ClipMeasurement, now_iso8601: &dyn Fn() -> String) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO scan_skips (clip_id, reason, scanner_version, skipped_at)\nVALUES (?, ?, ?, ?)",
        params![
            row.clip_id,
            row.lab_reason.clone().unwrap_or_else(|| UNSUPPORTED_STATUS.to_string()),
            row.measure_version.clone().unwrap_or_else(|| SCANNER_MEASURE_VERSION.to_string()),
            row.measured_at.clone().unwrap_or_else(now_iso8601),
        ],
    )?;
    Ok(())
}

/// The row for `clip_id` in `<schema>.clip_measurements`, or `None` ("not measured").
pub fn measurement(conn: &Connection, schema: &str, clip_id: &str) -> Result<Option<ClipMeasurement>> {
    Ok(conn.query_row(&format!("SELECT * FROM {schema}.clip_measurements WHERE clip_id = ?"), [clip_id], ClipMeasurement::from_row).optional()?)
}

/// `ISO8601DateFormatter().string(from:)` with its default options: `yyyy-MM-dd'T'HH:mm:ss'Z'` in UTC, whole
/// seconds.
pub fn iso8601(seconds_since_1970: f64) -> String {
    let secs = seconds_since_1970.floor() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso() {
        assert_eq!(iso8601(0.0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(1_794_674_229.9), "2026-11-14T16:37:09Z");
    }
}
