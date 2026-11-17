//! Brief import (Swift MaCrate 0.16.4 `BriefImporter`, CrateKit/BriefImport.swift): a file of clip briefs, JSON or
//! CSV, read into records the importer then matches by `clip_id` (first) or path.
//!
//! **JSON**, three shapes:
//! 1. a flat object `{clip_id: brief, …}` (non-string values skipped);
//! 2. an array of objects, each with any of `clip_id` / `clipid`, `current_path` / `path` / `file_path` /
//!    `filepath` and `brief` (keys compared lower-cased);
//! 3. an object whose `briefs` key holds the array of (2) (wins over the flat reading when it is an array).
//!
//! A brief is trimmed (`whitespacesAndNewlines`) and dropped when empty; an array record needs a `clip_id` or a path
//! (a string, even an empty one). The document is read as Foundation's `JSONSerialization` reads it on macOS (an
//! oracle over Swift's own parser checked every rule below):
//! - the encoding: a UTF-8 BOM is skipped; a UTF-16 BOM (`FF FE`, `FE FF`) or the UTF-32 big-endian one
//!   (`00 00 FE FF`) picks that encoding; otherwise, past 4 bytes, the zero bytes in the first four pick UTF-32 /
//!   UTF-16 (BE or LE), else UTF-8; a UTF-16 text's odd last byte is ignored;
//! - the top level must be an object or an array; whitespace is space, tab, CR, LF; one trailing comma is allowed
//!   after the last member of an object or array; containers nest 512 deep (a 513th level may only be empty);
//! - strings: no control characters below U+0020, the standard escapes, surrogates only in pairs, and the first
//!   U+FEFF of a string (written or escaped) is dropped; numbers as JSON writes them, and must fit a `Double`;
//! - a key given twice keeps its first value.
//!
//! Where Swift's order is a hash order (the flat shape's records; which alias wins when an object carries two, say
//! `path` and `current_path`), this port takes the file's order: records in the order written, the first alias
//! written. Swift's own choice there changes from run to run. Keys that differ in bytes but are canonically
//! equivalent (NFC vs NFD) are two records here; Swift's dictionary merges them.
//!
//! **CSV**: a header row is required; columns are found by name (lower-cased, trimmed: `clip_id` / `clipid`,
//! `current_path` / `path` / `file_path` / `filepath`, `brief`; the first of each wins), other columns are ignored.
//! RFC 4180 quoting (`""` inside quotes is a quote; newlines inside quotes are kept); a record ends at LF, CR or
//! CRLF. Rows that are blank, too short to reach the brief column, have an empty brief, or have neither a
//! `clip_id` nor a path are skipped. [`parse`] reads the file as UTF-8 and drops a leading BOM (Swift's
//! `String(contentsOf:encoding: .utf8)`); [`parse_csv`] takes the text as given.

use crate::unicode::trim_whitespaces_and_newlines;
use std::fmt;
use std::path::Path;

/// One parsed brief. At least one of `clip_id` and `path` is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BriefImportRecord {
    pub clip_id: Option<String>,
    pub path: Option<String>,
    pub brief: String,
}

/// Why a brief file could not be read (`BriefImportParseError`; Swift's `malformedCSV` is never thrown and has no
/// counterpart). The texts are Foundation's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BriefImportError {
    /// The file could not be read: Foundation's sentence for it.
    Unreadable(String),
    /// Neither `json` nor `csv` (compared lower-cased): the extension, `""` when there is none.
    UnsupportedExtension(String),
    /// Not JSON, or JSON of another shape.
    MalformedJson(String),
    /// An empty CSV.
    CsvMissingHeader,
    /// A CSV header without a `brief` column.
    CsvNoBriefColumn,
}

impl fmt::Display for BriefImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BriefImportError::Unreadable(why) | BriefImportError::MalformedJson(why) => f.write_str(why),
            BriefImportError::UnsupportedExtension(ext) => write!(f, "unsupported file extension “{ext}”"),
            BriefImportError::CsvMissingHeader => f.write_str("the CSV file has no header row"),
            BriefImportError::CsvNoBriefColumn => f.write_str("the CSV header has no brief column"),
        }
    }
}

impl std::error::Error for BriefImportError {}

/// `JSONSerialization`'s error, as `localizedDescription` words it for every malformed document.
pub const MALFORMED_JSON: &str = "The data couldn’t be read because it isn’t in the correct format.";

/// Read a brief file, by its extension (`json` / `csv`, any case).
pub fn parse(path: &Path) -> Result<Vec<BriefImportRecord>, BriefImportError> {
    // `URL.pathExtension`: none for a name without a dot past its first character (`.json` has none).
    let ext = path.file_name().and_then(|n| n.to_str()).and_then(|n| n.rfind('.').filter(|&i| i > 0).map(|i| n[i + 1..].to_lowercase())).unwrap_or_default();
    match ext.as_str() {
        "json" => parse_json(&read(path)?),
        "csv" => {
            let bytes = read(path)?;
            let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
            let text = std::str::from_utf8(bytes)
                .map_err(|_| BriefImportError::Unreadable("The file couldn’t be opened because it isn’t in the correct format.".into()))?;
            parse_csv(text)
        }
        _ => Err(BriefImportError::UnsupportedExtension(ext)),
    }
}

/// The file's bytes, or Foundation's sentence for why not.
fn read(path: &Path) -> Result<Vec<u8>, BriefImportError> {
    std::fs::read(path).map_err(|e| {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        BriefImportError::Unreadable(match e.kind() {
            std::io::ErrorKind::NotFound => format!("The file “{name}” couldn’t be opened because there is no such file."),
            std::io::ErrorKind::PermissionDenied => format!("The file “{name}” couldn’t be opened because you don’t have permission to view it."),
            _ => format!("The file “{name}” couldn’t be opened."),
        })
    })
}

// ---------------------------------------------------------------------------------------------
// JSON

/// The parts of a JSON value the importer looks at.
enum Value {
    Str(String),
    Arr(Vec<Value>),
    /// Members in file order, each key once (its first value).
    Obj(Vec<(String, Value)>),
    /// A number, `true`, `false` or `null`.
    Other,
}

/// Parse a JSON brief file (see the module notes for the shapes and the reading rules).
pub fn parse_json(data: &[u8]) -> Result<Vec<BriefImportRecord>, BriefImportError> {
    let malformed = || BriefImportError::MalformedJson(MALFORMED_JSON.into());
    let text = decode_json_text(data).ok_or_else(malformed)?;
    let root = Parser { s: text.as_bytes(), at: 0 }.document().ok_or_else(malformed)?;
    Ok(match root {
        Value::Arr(items) => records_from_objects(&items),
        Value::Obj(members) => {
            if let Some((_, Value::Arr(items))) = members.iter().find(|(k, _)| k == "briefs") {
                records_from_objects(items)
            } else {
                members
                    .iter()
                    .filter_map(|(key, value)| {
                        let Value::Str(brief) = value else { return None };
                        let brief = trim_whitespaces_and_newlines(brief);
                        (!brief.is_empty()).then(|| BriefImportRecord { clip_id: Some(key.clone()), path: None, brief: brief.to_string() })
                    })
                    .collect()
            }
        }
        // The parser only returns a container at the top level.
        Value::Str(_) | Value::Other => return Err(malformed()),
    })
}

fn records_from_objects(items: &[Value]) -> Vec<BriefImportRecord> {
    items
        .iter()
        .filter_map(|item| {
            let Value::Obj(members) = item else { return None };
            let (mut clip_id, mut path, mut brief): (Option<&str>, Option<&str>, Option<&str>) = (None, None, None);
            for (key, value) in members {
                let lower = key.to_lowercase();
                let text = match value {
                    Value::Str(s) => Some(s.as_str()),
                    _ => None,
                };
                if clip_id.is_none() && (lower == "clip_id" || lower == "clipid") {
                    clip_id = text;
                }
                if path.is_none() && matches!(lower.as_str(), "current_path" | "path" | "file_path" | "filepath") {
                    path = text;
                }
                if brief.is_none() && lower == "brief" {
                    brief = text;
                }
            }
            let brief = trim_whitespaces_and_newlines(brief?);
            if brief.is_empty() || (clip_id.is_none() && path.is_none()) {
                return None;
            }
            Some(BriefImportRecord { clip_id: clip_id.map(str::to_string), path: path.map(str::to_string), brief: brief.to_string() })
        })
        .collect()
}

/// The document's text in the encoding `JSONSerialization` detects; `None` when it does not decode.
fn decode_json_text(b: &[u8]) -> Option<String> {
    #[derive(Clone, Copy)]
    enum Enc {
        Utf8,
        Utf16(bool),
        Utf32(bool),
    }
    let (enc, skip) = match b {
        [0xEF, 0xBB, 0xBF, ..] => (Enc::Utf8, 3),
        [0x00, 0x00, 0xFE, 0xFF, ..] => (Enc::Utf32(true), 4),
        [0xFF, 0xFE, ..] => (Enc::Utf16(false), 2),
        [0xFE, 0xFF, ..] => (Enc::Utf16(true), 2),
        [b0, b1, b2, b3, _, ..] => match (*b0, *b1, *b2, *b3) {
            (0, 0, 0, _) => (Enc::Utf32(true), 0),
            (_, 0, 0, 0) => (Enc::Utf32(false), 0),
            (0, _, 0, _) => (Enc::Utf16(true), 0),
            (_, 0, _, 0) => (Enc::Utf16(false), 0),
            _ => (Enc::Utf8, 0),
        },
        _ => (Enc::Utf8, 0),
    };
    let body = &b[skip..];
    match enc {
        Enc::Utf8 => std::str::from_utf8(body).ok().map(str::to_string),
        Enc::Utf16(be) => {
            let units = body.as_chunks::<2>().0.iter().map(|&p| if be { u16::from_be_bytes(p) } else { u16::from_le_bytes(p) });
            char::decode_utf16(units).collect::<Result<String, _>>().ok()
        }
        Enc::Utf32(be) => body.as_chunks::<4>().0.iter().map(|&p| char::from_u32(if be { u32::from_be_bytes(p) } else { u32::from_le_bytes(p) })).collect(),
    }
}

/// `JSONSerialization`'s nesting limit: a container below this many levels may only be empty.
const MAX_DEPTH: usize = 512;

struct Parser<'a> {
    s: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn document(mut self) -> Option<Value> {
        self.space();
        let root = match self.peek()? {
            b'{' | b'[' => self.value(0)?,
            _ => return None,
        };
        self.space();
        (self.at == self.s.len()).then_some(root)
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        let hit = self.peek() == Some(c);
        if hit {
            self.at += 1;
        }
        hit
    }

    /// One value at `depth` containers deep (whitespace before it already skipped).
    fn value(&mut self, depth: usize) -> Option<Value> {
        match self.peek()? {
            b'{' => self.object(depth + 1),
            b'[' => self.array(depth + 1),
            b'"' => self.string().map(Value::Str),
            b't' => self.literal(b"true"),
            b'f' => self.literal(b"false"),
            b'n' => self.literal(b"null"),
            _ => self.number(),
        }
    }

    fn literal(&mut self, word: &[u8]) -> Option<Value> {
        self.s.get(self.at..)?.starts_with(word).then(|| {
            self.at += word.len();
            Value::Other
        })
    }

    fn array(&mut self, depth: usize) -> Option<Value> {
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.eat(b']') {
            return Some(Value::Arr(items));
        }
        if depth > MAX_DEPTH {
            return None;
        }
        loop {
            items.push(self.value(depth)?);
            self.space();
            if self.eat(b']') {
                return Some(Value::Arr(items));
            }
            if !self.eat(b',') {
                return None;
            }
            self.space();
            // One trailing comma before the end.
            if self.eat(b']') {
                return Some(Value::Arr(items));
            }
        }
    }

    fn object(&mut self, depth: usize) -> Option<Value> {
        self.at += 1;
        let mut members: Vec<(String, Value)> = Vec::new();
        self.space();
        if self.eat(b'}') {
            return Some(Value::Obj(members));
        }
        if depth > MAX_DEPTH {
            return None;
        }
        loop {
            if self.peek()? != b'"' {
                return None;
            }
            let key = self.string()?;
            self.space();
            if !self.eat(b':') {
                return None;
            }
            self.space();
            let value = self.value(depth)?;
            if !members.iter().any(|(k, _)| *k == key) {
                members.push((key, value));
            }
            self.space();
            if self.eat(b'}') {
                return Some(Value::Obj(members));
            }
            if !self.eat(b',') {
                return None;
            }
            self.space();
            if self.eat(b'}') {
                return Some(Value::Obj(members));
            }
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        let digits = std::str::from_utf8(self.s.get(self.at..self.at + 4)?).ok()?;
        if !digits.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        self.at += 4;
        u32::from_str_radix(digits, 16).ok()
    }

    fn string(&mut self) -> Option<String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            // Copy the run up to the next quote, backslash or control character as it is (the text is valid UTF-8).
            let run = self.s.get(self.at..)?.iter().position(|&c| c == b'"' || c == b'\\' || c < 0x20)?;
            out.push_str(std::str::from_utf8(&self.s[self.at..self.at + run]).ok()?);
            self.at += run;
            match self.peek()? {
                b'"' => {
                    self.at += 1;
                    // Foundation drops a string's leading byte-order mark.
                    return Some(out.strip_prefix('\u{FEFF}').map(str::to_string).unwrap_or(out));
                }
                b'\\' => {
                    self.at += 1;
                    let e = self.peek()?;
                    self.at += 1;
                    out.push(match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            if (0xD800..0xDC00).contains(&hi) {
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return None;
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return None;
                                }
                                char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00))?
                            } else {
                                char::from_u32(hi)?
                            }
                        }
                        _ => return None,
                    });
                }
                // A raw control character.
                _ => return None,
            }
        }
    }

    fn number(&mut self) -> Option<Value> {
        let start = self.at;
        self.eat(b'-');
        let digits = |p: &mut Self| {
            let from = p.at;
            while matches!(p.peek(), Some(b'0'..=b'9')) {
                p.at += 1;
            }
            p.at - from
        };
        let first = self.peek()?;
        let int = digits(self);
        if int == 0 || (first == b'0' && int > 1) {
            return None;
        }
        if self.eat(b'.') && digits(self) == 0 {
            return None;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if digits(self) == 0 {
                return None;
            }
        }
        let text = std::str::from_utf8(&self.s[start..self.at]).ok()?;
        text.parse::<f64>().ok().filter(|v| v.is_finite()).map(|_| Value::Other)
    }
}

// ---------------------------------------------------------------------------------------------
// CSV

/// Parse a CSV brief file (see the module notes).
pub fn parse_csv(text: &str) -> Result<Vec<BriefImportRecord>, BriefImportError> {
    let rows = parse_csv_rows(text);
    let header = rows.first().ok_or(BriefImportError::CsvMissingHeader)?;
    let (mut clip_col, mut path_col, mut brief_col) = (None, None, None);
    for (i, name) in header.iter().enumerate() {
        let lower = name.to_lowercase();
        match trim_whitespaces_and_newlines(&lower) {
            "clip_id" | "clipid" => {
                clip_col.get_or_insert(i);
            }
            "current_path" | "path" | "file_path" | "filepath" => {
                path_col.get_or_insert(i);
            }
            "brief" => {
                brief_col.get_or_insert(i);
            }
            _ => {}
        }
    }
    let brief_col = brief_col.ok_or(BriefImportError::CsvNoBriefColumn)?;
    let cell = |row: &[String], col: Option<usize>| {
        col.and_then(|c| row.get(c)).map(|v| trim_whitespaces_and_newlines(v)).filter(|v| !v.is_empty()).map(str::to_string)
    };
    let mut records = Vec::new();
    for row in &rows[1..] {
        if row.len() == 1 && row[0].is_empty() {
            continue;
        }
        let Some(brief) = row.get(brief_col) else { continue };
        let brief = trim_whitespaces_and_newlines(brief);
        if brief.is_empty() {
            continue;
        }
        let (clip_id, path) = (cell(row, clip_col), cell(row, path_col));
        if clip_id.is_none() && path.is_none() {
            continue;
        }
        records.push(BriefImportRecord { clip_id, path, brief: brief.to_string() });
    }
    Ok(records)
}

/// The CSV's rows and fields (Swift's `parseCSVRows`): `"…"` quoting from the start of a field, `""` inside quotes
/// for a quote, newlines kept inside quotes, rows ended by LF, CR or CRLF; an unterminated quote runs to the end.
pub fn parse_csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut current = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.next_if_eq(&'"').is_some() {
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            ',' => current.push(std::mem::take(&mut field)),
            '\n' | '\r' => {
                if c == '\r' {
                    chars.next_if_eq(&'\n');
                }
                current.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut current));
            }
            '"' if field.is_empty() => in_quotes = true,
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !current.is_empty() {
        current.push(field);
        rows.push(current);
    }
    rows
}
