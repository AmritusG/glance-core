//! A structural dump of a SQLite file — `sqlite_master`, the file pragmas, `PRAGMA table_xinfo` / `index_list` /
//! `foreign_key_list` per table, `index_xinfo` per index, and every row by primary key with its storage class — the
//! same shape the Swift oracle (`tools/swift-library-oracle`) writes, so two libraries compare value for value and
//! type for type. A REAL is written as its IEEE-754 bit pattern (hex), never re-printed.

use crate::error::Result;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, Row};
use serde_json::{Map, Value, json};
use std::path::Path;

fn value(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(["integer", i.to_string()]),
        ValueRef::Real(r) => json!(["real", format!("{:016x}", r.to_bits())]),
        ValueRef::Text(t) => json!(["text", String::from_utf8_lossy(t)]),
        ValueRef::Blob(b) => json!(["blob", b.iter().map(|x| format!("{x:02x}")).collect::<String>()]),
    }
}

fn row_object(row: &Row<'_>, names: &[String]) -> rusqlite::Result<Value> {
    let mut m = Map::new();
    for (i, name) in names.iter().enumerate() {
        m.insert(name.clone(), value(row.get_ref(i)?));
    }
    Ok(Value::Object(m))
}

fn all_rows(conn: &Connection, sql: &str) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(sql)?;
    let names: Vec<String> = stmt.column_names().map(|s| s.to_string()).collect();
    let rows = stmt.query_map([], |r| row_object(r, &names))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Dump the database at `path`, opened read-only.
pub fn dump_file(path: &Path) -> Result<Value> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    dump(&conn)
}

/// Dump the `main` database of `conn`.
pub fn dump(conn: &Connection) -> Result<Value> {
    let mut out = Map::new();
    let master = all_rows(conn, "SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY rowid")?;
    for p in ["journal_mode", "user_version", "application_id", "page_size", "encoding", "auto_vacuum", "page_count", "freelist_count"] {
        let v = conn.query_row(&format!("PRAGMA {p}"), [], |r| Ok(value(r.get_ref(0)?)))?;
        out.insert(format!("pragma_{p}"), v);
    }
    let mut tables = Map::new();
    let mut indexes = Map::new();
    for entry in &master {
        let text = |k: &str| entry.get(k).and_then(|v| v.get(1)).and_then(Value::as_str).unwrap_or("").to_string();
        let (kind, name, sql) = (text("type"), text("name"), text("sql"));
        let q = quoted(&name);
        if kind == "index" {
            indexes.insert(name.clone(), Value::Array(all_rows(conn, &format!("PRAGMA index_xinfo({q})"))?));
            continue;
        }
        if kind != "table" {
            continue;
        }
        let mut t = Map::new();
        let info = all_rows(conn, &format!("PRAGMA table_xinfo({q})"))?;
        t.insert("index_list".into(), Value::Array(all_rows(conn, &format!("PRAGMA index_list({q})"))?));
        t.insert("foreign_key_list".into(), Value::Array(all_rows(conn, &format!("PRAGMA foreign_key_list({q})"))?));
        if !sql.starts_with("CREATE VIRTUAL") {
            let int = |v: &Value, k: &str| v.get(k).and_then(|x| x.get(1)).and_then(Value::as_str).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            let mut pk: Vec<(i64, String)> = info
                .iter()
                .filter(|c| int(c, "pk") > 0)
                .map(|c| (int(c, "pk"), quoted(c.get("name").and_then(|x| x.get(1)).and_then(Value::as_str).unwrap_or(""))))
                .collect();
            pk.sort();
            let without_rowid = sql.contains("WITHOUT ROWID");
            let select = if without_rowid { "SELECT *" } else { "SELECT rowid AS \"__rowid\", *" };
            let order = if pk.is_empty() {
                if without_rowid { String::new() } else { " ORDER BY rowid".to_string() }
            } else {
                format!(" ORDER BY {}", pk.into_iter().map(|p| p.1).collect::<Vec<_>>().join(", "))
            };
            t.insert("rows".into(), Value::Array(all_rows(conn, &format!("{select} FROM {q}{order}"))?));
        }
        t.insert("table_xinfo".into(), Value::Array(info));
        tables.insert(name, Value::Object(t));
    }
    out.insert("sqlite_master".into(), Value::Array(master));
    out.insert("tables".into(), Value::Object(tables));
    out.insert("indexes".into(), Value::Object(indexes));
    Ok(Value::Object(out))
}

/// `PRAGMA integrity_check` rows, `PRAGMA foreign_key_check` row count, the GRDB migration identifiers, and the
/// `clips` / `clip_search` row counts.
pub fn checks(conn: &Connection) -> Result<Value> {
    let strings = |sql: &str| -> Result<Vec<String>> {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    };
    let count = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
    let fk = {
        let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
        stmt.query_map([], |_| Ok(()))?.count()
    };
    Ok(json!({
        "integrity_check": strings("PRAGMA integrity_check")?,
        "foreign_key_check": fk,
        "migrations": strings("SELECT identifier FROM grdb_migrations ORDER BY rowid")?,
        "clips": count("SELECT COUNT(*) FROM clips")?,
        "clip_search": count("SELECT COUNT(*) FROM clip_search")?,
    }))
}
