//! Library roots, exclusions and the path forms Swift MaCrate compares folders in (CrateCore/LibraryRootStore.swift,
//! LibraryRoot.swift, RecursionPreference.swift, ExpansionStore.swift, LibraryIndexer.swift, CrateKit/
//! ExclusionStore.swift), and the shapes of the small JSON files MaCrate keeps in Application Support. The
//! bookmarks inside those files are Apple's opaque format: carried here as bytes, resolved later by the platform.
//!
//! Two path forms, both measured against Swift on macOS 27 (docs/data/m1/files-crosstest.txt):
//! - [`standardized_path`] — `url.standardizedFileURL.path`: the exclusion set, the index denylist, the walk's
//!   prune test. Symbolic links are kept, except that a path with `..` is resolved (the real parent), a leading
//!   `/private` is dropped when the path without it exists, and the text comes back decomposed (Apple's file-system
//!   form of NFD).
//! - [`canonical_path`] — `LibraryRootStore.canonicalPath`: standardized, then every symbolic link resolved (when
//!   the path exists; otherwise unchanged), then NFC. Root identity, containment and de-duplication.
//!
//! The database's `current_path` is neither: it is the walk's own path ([`crate::enumerate::ClipFile::path`]).

use crate::enumerate::localized_case_insensitive_cmp;
use crate::unicode::file_system_representation;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

// MARK: - Path forms

/// Swift's `URL(fileURLWithPath: path).standardizedFileURL.path`.
///
/// A relative `path` is taken from the current directory and a leading `~/` from `HOME`, as `URL(fileURLWithPath:)`
/// does. Then: empty and `.` components dropped, the trailing `/` dropped; a path with a `..` component is the real
/// path when it exists (`/tmp/..` is `/private`) and is otherwise reduced by text; `/private/var/automount`,
/// `/var/automount` or `/private` is removed from the front when what remains exists; the result is decomposed
/// ([`crate::unicode::file_system_representation`], the form `URL(fileURLWithPath:)` gives).
pub fn standardized_path(path: &str) -> String {
    let absolute = absolute(path);
    let mut parts: Vec<&str> = Vec::new();
    let mut has_parent = false;
    for c in absolute.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                has_parent = true;
                parts.push(c);
            }
            _ => parts.push(c),
        }
    }
    let joined = format!("/{}", parts.join("/"));
    let mut result = joined.clone();
    if has_parent {
        result = match std::fs::canonicalize(&joined).ok().and_then(|p| p.to_str().map(str::to_string)) {
            Some(real) => real,
            None => {
                let mut kept: Vec<&str> = Vec::new();
                for c in parts {
                    if c == ".." {
                        kept.pop();
                    } else {
                        kept.push(c);
                    }
                }
                format!("/{}", kept.join("/"))
            }
        };
    }
    file_system_representation(&strip_private(result))
}

/// Swift's `LibraryRootStore.canonicalPath(URL(fileURLWithPath: path))`: [`standardized_path`], then
/// `resolvingSymlinksInPath()` (the real path with a leading `/private` removed when the rest exists; the
/// standardized path unchanged when it cannot be resolved), then NFC (`precomposedStringWithCanonicalMapping`).
pub fn canonical_path(path: &str) -> String {
    let standardized = standardized_path(path);
    let resolved = match std::fs::canonicalize(&standardized).ok().and_then(|p| p.to_str().map(str::to_string)) {
        Some(real) => strip_private(real),
        None => standardized,
    };
    resolved.nfc().collect()
}

/// `URL(fileURLWithPath: path).lastPathComponent` — a new root's `displayName`: the last component of the absolute
/// path as written (`.` and `..` included; `/` for the root), decomposed.
pub fn last_path_component(path: &str) -> String {
    let absolute = absolute(path);
    let trimmed = absolute.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/".to_string();
    }
    file_system_representation(trimmed.rsplit('/').next().unwrap_or(trimmed))
}

fn absolute(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME").and_then(|h| h.into_string().ok())
    {
        return format!("{home}/{rest}");
    }
    if path.starts_with('/') {
        return path.to_string();
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    format!("{}/{path}", cwd.to_str().unwrap_or(""))
}

/// Drop a leading `/private/var/automount`, `/var/automount` or `/private` when the path without it exists.
fn strip_private(path: String) -> String {
    for prefix in ["/private/var/automount", "/var/automount", "/private"] {
        if let Some(rest) = path.strip_prefix(prefix)
            && rest.starts_with('/')
            && Path::new(rest).exists()
        {
            return rest.to_string();
        }
    }
    path
}

// MARK: - Roots

/// Walk roots refused outright, and their descendants (`LibraryIndexer.denyParentOrDescendant`).
pub const DENY_WITH_DESCENDANTS: [&str; 10] = ["/System", "/Library", "/Applications", "/private", "/usr", "/bin", "/sbin", "/etc", "/var", "/tmp"];
/// Walk roots refused only when exactly equal (`LibraryIndexer.denyExactMatch`): their children are real roots.
pub const DENY_EXACT: [&str; 3] = ["/", "/Users", "/Volumes"];

/// `LibraryIndexer.isDangerousRoot`: whether a root whose [`standardized_path`] is `path` must not be walked.
pub fn is_dangerous_root(path: &str) -> bool {
    DENY_EXACT.contains(&path) || DENY_WITH_DESCENDANTS.iter().any(|d| path == *d || path.strip_prefix(d).is_some_and(|r| r.starts_with('/')))
}

/// `LibraryRootStore.isContained`: `candidate` is `root` or lies under it, both compared in [`canonical_path`] form
/// with a component boundary (`/a/Foo` contains `/a/Foo/bar`, not `/a/Foobar`).
pub fn is_contained(candidate: &str, root: &str) -> bool {
    is_contained_canonical(&canonical_path(candidate), &canonical_path(root))
}

/// [`is_contained`] on two paths already in canonical form.
pub fn is_contained_canonical(candidate: &str, root: &str) -> bool {
    candidate == root || candidate.strip_prefix(root).is_some_and(|r| r.starts_with('/'))
}

/// What adding a folder as a root does (`LibraryRootStore.AddRootResult`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddRootOutcome {
    /// Added; `overlaps_existing` when it is a strict parent or child of an attached root (warn and allow).
    Added { overlaps_existing: bool },
    /// The same folder ([`canonical_path`]) is already attached; nothing changes.
    AlreadyAttached,
}

/// `LibraryRootStore.addRoot`'s decision for `candidate`, given the resolved paths of the attached roots (offline
/// roots have none and are not compared — Swift's known limitation, kept).
pub fn add_root_outcome<'a>(candidate: &str, resolved_roots: impl IntoIterator<Item = &'a str>) -> AddRootOutcome {
    let c = canonical_path(candidate);
    let roots: Vec<String> = resolved_roots.into_iter().map(canonical_path).collect();
    if roots.contains(&c) {
        return AddRootOutcome::AlreadyAttached;
    }
    let overlaps_existing = roots.iter().any(|r| is_contained_canonical(&c, r) || is_contained_canonical(r, &c));
    AddRootOutcome::Added { overlaps_existing }
}

/// A library root as `library-roots.json` stores it (`LibraryRoot`).
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryRoot {
    pub id: Uuid,
    pub display_name: String,
    /// A plain (not security-scoped) bookmark, opaque here.
    pub bookmark_data: Vec<u8>,
    /// Seconds since 2001-01-01 (Foundation's `Date`).
    pub date_added: f64,
}

/// The sidebar's root order (`RootSortMode`, stored in `root-sort.json` as its raw string).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RootSortMode {
    #[default]
    Manual,
    NameAsc,
    NameDesc,
    DateAddedAsc,
    DateAddedDesc,
    SizeAsc,
    SizeDesc,
}

impl RootSortMode {
    pub const ALL: [RootSortMode; 7] = [Self::Manual, Self::NameAsc, Self::NameDesc, Self::DateAddedAsc, Self::DateAddedDesc, Self::SizeAsc, Self::SizeDesc];

    /// The raw value Swift stores.
    pub fn raw(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::NameAsc => "nameAsc",
            Self::NameDesc => "nameDesc",
            Self::DateAddedAsc => "dateAddedAsc",
            Self::DateAddedDesc => "dateAddedDesc",
            Self::SizeAsc => "sizeAsc",
            Self::SizeDesc => "sizeDesc",
        }
    }

    pub fn from_raw(raw: &str) -> Option<RootSortMode> {
        Self::ALL.into_iter().find(|m| m.raw() == raw)
    }
}

/// `RootSorting.apply`: the roots in `mode`'s order, the stored order untouched. Names compare as
/// [`localized_case_insensitive_cmp`]; equal dates and equal sizes fall back to the name A→Z; a root without a size
/// (offline, not computed) goes last in both size orders. Stable, like Swift's sort.
pub fn sort_roots(roots: &[LibraryRoot], mode: RootSortMode, sizes: &HashMap<Uuid, i64>) -> Vec<LibraryRoot> {
    let mut out = roots.to_vec();
    let name = |a: &LibraryRoot, b: &LibraryRoot| localized_case_insensitive_cmp(&a.display_name, &b.display_name);
    let date = |a: &LibraryRoot, b: &LibraryRoot, asc: bool| {
        if a.date_added != b.date_added {
            let o = a.date_added.partial_cmp(&b.date_added).unwrap_or(Ordering::Equal);
            if asc { o } else { o.reverse() }
        } else {
            name(a, b)
        }
    };
    let size = |a: &LibraryRoot, b: &LibraryRoot, asc: bool| match (sizes.get(&a.id), sizes.get(&b.id)) {
        (Some(x), Some(y)) if x != y => {
            if asc {
                x.cmp(y)
            } else {
                y.cmp(x)
            }
        }
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        _ => name(a, b),
    };
    match mode {
        RootSortMode::Manual => {}
        RootSortMode::NameAsc => out.sort_by(name),
        RootSortMode::NameDesc => out.sort_by(|a, b| name(b, a)),
        RootSortMode::DateAddedAsc => out.sort_by(|a, b| date(a, b, true)),
        RootSortMode::DateAddedDesc => out.sort_by(|a, b| date(a, b, false)),
        RootSortMode::SizeAsc => out.sort_by(|a, b| size(a, b, true)),
        RootSortMode::SizeDesc => out.sort_by(|a, b| size(a, b, false)),
    }
    out
}

/// `LibraryRootStore.moveRoots(fromOffsets:toOffset:)` (SwiftUI's `Array.move` semantics): the roots at `source`
/// move, in their order, to before the element that was at `destination`.
pub fn move_roots<T: Clone>(items: &mut Vec<T>, source: &[usize], destination: usize) {
    let mut from: Vec<usize> = source.iter().copied().filter(|&i| i < items.len()).collect();
    from.sort_unstable();
    from.dedup();
    let moving: Vec<T> = from.iter().filter_map(|&i| items.get(i).cloned()).collect();
    for &i in from.iter().rev() {
        items.remove(i);
    }
    let before = from.iter().filter(|&&i| i < destination).count();
    let at = destination.saturating_sub(before).min(items.len());
    items.splice(at..at, moving);
}

/// `LibraryRootStore.mergeDedup`: the lists concatenated, keeping the first root per `key`.
pub fn merge_dedup(lists: &[Vec<LibraryRoot>], key: impl Fn(&LibraryRoot) -> String) -> Vec<LibraryRoot> {
    let mut seen = HashSet::new();
    lists.iter().flatten().filter(|r| seen.insert(key(r))).cloned().collect()
}

/// `LibraryRootStore.collapseResolvedDuplicates`: the ids of roots to drop because an earlier root resolves to the
/// same [`canonical_path`]. `resolved` gives each root's path, `None` when offline (never collapsed).
pub fn duplicate_root_ids(roots: &[LibraryRoot], resolved: impl Fn(&LibraryRoot) -> Option<String>) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    roots.iter().filter(|r| resolved(r).is_some_and(|p| !seen.insert(canonical_path(&p)))).map(|r| r.id).collect()
}

// MARK: - Exclusions

/// The `UserDefaults` key (MaCrate's domain) holding the excluded folders, an array of [`standardized_path`]s.
pub const EXCLUDED_FOLDERS_KEY: &str = "com.amritus.crate.excludedFolders";

/// `ExclusionStore.isEffectivelyExcluded(_:among:)`: `path` is in `excluded` or lies under one of its entries
/// (component boundary). Both in [`standardized_path`] form.
pub fn is_effectively_excluded(path: &str, excluded: &HashSet<String>) -> bool {
    excluded.contains(path) || excluded.iter().any(|p| path.strip_prefix(p.as_str()).is_some_and(|r| r.starts_with('/')))
}

/// `ExclusionStore.exclude`: add `folder` unless an excluded ancestor already covers it, dropping entries under it
/// (the set stays flat). Returns whether the set changed (Swift saves only then).
pub fn exclude(excluded: &mut HashSet<String>, folder: &str) -> bool {
    let p = standardized_path(folder);
    if is_effectively_excluded(&p, excluded) {
        return false;
    }
    let under = format!("{p}/");
    excluded.retain(|e| !e.starts_with(&under));
    excluded.insert(p);
    true
}

/// `ExclusionStore.include`: remove a direct exclusion of `folder`; a folder covered only by an excluded ancestor
/// stays excluded. Returns whether the set changed.
pub fn include(excluded: &mut HashSet<String>, folder: &str) -> bool {
    excluded.remove(&standardized_path(folder))
}

/// `ExclusionStore.excludedAncestor(of:)`: the excluded entry covering `folder` (itself or an ancestor).
pub fn excluded_ancestor(excluded: &HashSet<String>, folder: &str) -> Option<String> {
    let p = standardized_path(folder);
    if excluded.contains(&p) {
        return Some(p);
    }
    excluded.iter().find(|e| p.strip_prefix(e.as_str()).is_some_and(|r| r.starts_with('/'))).cloned()
}

// MARK: - The indexer's cache hit

/// `LibraryIndexer.rowMatches`: a stored row still describes the file when the size is equal and the stored mtime
/// is within 1 ms of the file's ([`crate::enumerate::foundation_mtime`]).
pub fn cache_hit(row_size: i64, row_mtime: f64, file_size: i64, file_mtime: f64) -> bool {
    row_size == file_size && (row_mtime - file_mtime).abs() < 0.001
}

// MARK: - Application Support files

/// File names in `~/Library/Application Support/com.amritus.crate/`.
pub const LIBRARY_ROOTS_FILE: &str = "library-roots.json";
pub const ROOT_SORT_FILE: &str = "root-sort.json";
pub const ROOT_SIZE_DISPLAY_FILE: &str = "root-size-display.json";
pub const RECURSION_PREFS_FILE: &str = "recursion-prefs.json";
pub const EXPANDED_FOLDERS_FILE: &str = "expanded-folders.json";

/// `library-roots.json` as a plain `JSONEncoder` writes `[LibraryRoot]`: `id` the upper-case UUID, `bookmarkData`
/// base64, `dateAdded` the number of seconds since 2001 ([`json_number`]), strings escaped as Foundation does
/// (`/` as `\/`). Swift's own key order is not fixed (it changes from object to object within one run, measured),
/// so readers never depend on it; this writes the declaration order. `None` when a date is not finite (Swift's
/// encoder throws and the file is left as it was).
pub fn encode_library_roots(roots: &[LibraryRoot]) -> Option<String> {
    let mut out = String::from("[");
    for (i, r) in roots.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let id = r.id.hyphenated().encode_upper(&mut Uuid::encode_buffer()).to_string();
        out.push_str(&format!(
            "{{\"id\":{},\"displayName\":{},\"bookmarkData\":{},\"dateAdded\":{}}}",
            json_string(&id),
            json_string(&r.display_name),
            json_string(&base64_encode(&r.bookmark_data)),
            json_number(r.date_added)?
        ));
    }
    out.push(']');
    Some(out)
}

/// Read `library-roots.json`. `None` for a file that does not decode (Swift then starts with no roots and does not
/// migrate over it). Keys in any order; unknown keys ignored; every field required.
pub fn decode_library_roots(bytes: &[u8]) -> Option<Vec<LibraryRoot>> {
    let Json::Array(items) = Json::parse(bytes)? else { return None };
    items
        .into_iter()
        .map(|item| {
            let Json::Object(fields) = item else { return None };
            let get = |k: &str| fields.iter().find(|(name, _)| name == k).map(|(_, v)| v);
            let id = match get("id")? {
                Json::String(s) if s.len() == 36 => Uuid::try_parse(s).ok()?,
                _ => return None,
            };
            let display_name = match get("displayName")? {
                Json::String(s) => s.clone(),
                _ => return None,
            };
            let bookmark_data = match get("bookmarkData")? {
                Json::String(s) => base64_decode(s)?,
                _ => return None,
            };
            let date_added = match get("dateAdded")? {
                Json::Number(n) => *n,
                _ => return None,
            };
            Some(LibraryRoot { id, display_name, bookmark_data, date_added })
        })
        .collect()
}

/// `recursion-prefs.json` / `expanded-folders.json`: an array of base64 bookmark blobs (`[Data]`).
pub fn encode_bookmark_blobs(blobs: &[Vec<u8>]) -> String {
    let items: Vec<String> = blobs.iter().map(|b| json_string(&base64_encode(b))).collect();
    format!("[{}]", items.join(","))
}

/// Read `recursion-prefs.json` / `expanded-folders.json`; `None` when it does not decode (Swift: empty, no
/// migration over it).
pub fn decode_bookmark_blobs(bytes: &[u8]) -> Option<Vec<Vec<u8>>> {
    let Json::Array(items) = Json::parse(bytes)? else { return None };
    items
        .into_iter()
        .map(|i| match i {
            Json::String(s) => base64_decode(&s),
            _ => None,
        })
        .collect()
}

/// `root-sort.json`: the mode's raw value as one JSON string.
pub fn encode_root_sort(mode: RootSortMode) -> String {
    json_string(mode.raw())
}

/// Read `root-sort.json`: [`RootSortMode::Manual`] when absent, unreadable or unknown.
pub fn decode_root_sort(bytes: Option<&[u8]>) -> RootSortMode {
    match bytes.and_then(Json::parse) {
        Some(Json::String(s)) => RootSortMode::from_raw(&s).unwrap_or_default(),
        _ => RootSortMode::Manual,
    }
}

/// `root-size-display.json`: one JSON Bool.
pub fn encode_show_sizes(show: bool) -> String {
    show.to_string()
}

/// Read `root-size-display.json`: `true` when absent or unreadable.
pub fn decode_show_sizes(bytes: Option<&[u8]>) -> bool {
    match bytes.and_then(Json::parse) {
        Some(Json::Bool(b)) => b,
        _ => true,
    }
}

/// A JSON number as Foundation's `JSONEncoder` writes a `Double`: Swift's description
/// ([`crate::thumbkey::swift_double`]) without a trailing `.0` (`784111777`, `784111777.123456`, `1e+16`,
/// `1e-05`, `-0`). `None` for NaN and infinities, which the encoder refuses.
pub fn json_number(x: f64) -> Option<String> {
    if !x.is_finite() {
        return None;
    }
    let s = crate::thumbkey::swift_double(x);
    Some(s.strip_suffix(".0").map(str::to_string).unwrap_or(s))
}

/// A JSON string as Foundation's `JSONEncoder` writes it: `"` `\` `/` escaped, `\b \f \n \r \t`, other controls
/// below U+0020 as `\u00xx` (lower-case hex), everything else (DEL, U+2028, emoji) as UTF-8.
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding, no line breaks (`Data.base64EncodedString()`, what `JSONEncoder` writes for `Data`).
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(B64[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Strict standard base64 (`Data(base64Encoded:)` without options): length a multiple of 4, padding only at the
/// end, no whitespace.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| B64.iter().position(|&b| b == c).map(|v| v as u32);
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let chunks = bytes.len() / 4;
    for (i, chunk) in bytes.chunks(4).enumerate() {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && i + 1 != chunks) {
            return None;
        }
        let mut n = 0u32;
        for (j, &c) in chunk.iter().enumerate() {
            let v = if j >= 4 - pad { 0 } else { value(c)? };
            n = (n << 6) | v;
        }
        let decoded = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(decoded.get(..3 - pad)?);
    }
    Some(out)
}

/// Just enough JSON to read these files exactly: numbers are parsed from their text with Rust's correctly rounded
/// `f64` parser, so a `dateAdded` reads back to the very `Double` Swift wrote.
#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn parse(bytes: &[u8]) -> Option<Json> {
        let text = std::str::from_utf8(bytes).ok()?;
        let mut p = Parser { s: text.as_bytes(), at: 0 };
        let v = p.value(0)?;
        p.ws();
        (p.at == p.s.len()).then_some(v)
    }
}

struct Parser<'a> {
    s: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while matches!(self.s.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, lit: &[u8]) -> bool {
        let ok = self.s.get(self.at..self.at + lit.len()) == Some(lit);
        if ok {
            self.at += lit.len();
        }
        ok
    }

    fn value(&mut self, depth: usize) -> Option<Json> {
        if depth > 64 {
            return None;
        }
        self.ws();
        match *self.s.get(self.at)? {
            b'n' => self.eat(b"null").then_some(Json::Null),
            b't' => self.eat(b"true").then_some(Json::Bool(true)),
            b'f' => self.eat(b"false").then_some(Json::Bool(false)),
            b'"' => self.string().map(Json::String),
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                self.ws();
                if self.eat(b"]") {
                    return Some(Json::Array(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.ws();
                    if self.eat(b"]") {
                        return Some(Json::Array(items));
                    }
                    if !self.eat(b",") {
                        return None;
                    }
                }
            }
            b'{' => {
                self.at += 1;
                let mut fields = Vec::new();
                self.ws();
                if self.eat(b"}") {
                    return Some(Json::Object(fields));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.ws();
                    if !self.eat(b":") {
                        return None;
                    }
                    fields.push((k, self.value(depth + 1)?));
                    self.ws();
                    if self.eat(b"}") {
                        return Some(Json::Object(fields));
                    }
                    if !self.eat(b",") {
                        return None;
                    }
                }
            }
            _ => self.number().map(Json::Number),
        }
    }

    fn number(&mut self) -> Option<f64> {
        let start = self.at;
        while matches!(self.s.get(self.at), Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
            self.at += 1;
        }
        let text = std::str::from_utf8(self.s.get(start..self.at)?).ok()?;
        // JSON grammar: optional '-', digits, optional fraction, optional exponent.
        let body = text.strip_prefix('-').unwrap_or(text);
        if !body.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        text.parse().ok().filter(|v: &f64| v.is_finite())
    }

    fn string(&mut self) -> Option<String> {
        if !self.eat(b"\"") {
            return None;
        }
        let mut out = String::new();
        loop {
            let c = *self.s.get(self.at)?;
            self.at += 1;
            match c {
                b'"' => return Some(out),
                b'\\' => {
                    let e = *self.s.get(self.at)?;
                    self.at += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                if !self.eat(b"\\u") {
                                    return None;
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return None;
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            out.push(char::from_u32(cp)?);
                        }
                        _ => return None,
                    }
                }
                c if c < 0x20 => return None,
                _ => {
                    // Copy one UTF-8 sequence whole.
                    let start = self.at - 1;
                    let len = match c {
                        0x00..=0x7F => 1,
                        0xC0..=0xDF => 2,
                        0xE0..=0xEF => 3,
                        _ => 4,
                    };
                    let chunk = std::str::from_utf8(self.s.get(start..start + len)?).ok()?;
                    out.push_str(chunk);
                    self.at = start + len;
                }
            }
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        let t = std::str::from_utf8(self.s.get(self.at..self.at + 4)?).ok()?;
        self.at += 4;
        u32::from_str_radix(t, 16).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_denylist() {
        for yes in ["/", "/Users", "/Volumes", "/System", "/System/Library", "/Library/Caches", "/tmp", "/private", "/var/folders", "/usr/local"] {
            assert!(is_dangerous_root(yes), "{yes}");
        }
        for no in ["/Users/x", "/Volumes/X", "/usrx", "/privatefoo", "/Users/me/Movies"] {
            assert!(!is_dangerous_root(no), "{no}");
        }
    }

    #[test]
    fn containment_has_a_component_boundary() {
        assert!(is_contained_canonical("/a/Foo", "/a/Foo"));
        assert!(is_contained_canonical("/a/Foo/bar", "/a/Foo"));
        assert!(!is_contained_canonical("/a/Foobar", "/a/Foo"));
        assert!(!is_contained_canonical("/a", "/a/Foo"));
    }

    #[test]
    fn exclusions_stay_flat() {
        let mut set = HashSet::new();
        assert!(exclude(&mut set, "/Volumes/D/a/b"));
        assert!(exclude(&mut set, "/Volumes/D/a"));
        assert_eq!(set, HashSet::from(["/Volumes/D/a".to_string()]));
        assert!(!exclude(&mut set, "/Volumes/D/a/c"), "covered by an ancestor");
        assert!(is_effectively_excluded("/Volumes/D/a/c/d", &set));
        assert!(!is_effectively_excluded("/Volumes/D/ab", &set));
        assert_eq!(excluded_ancestor(&set, "/Volumes/D/a/c"), Some("/Volumes/D/a".to_string()));
        assert!(!include(&mut set, "/Volumes/D/a/c"), "only a direct exclusion is removed");
        assert!(include(&mut set, "/Volumes/D/a"));
        assert!(set.is_empty());
    }

    #[test]
    fn cache_hit_is_size_and_a_millisecond() {
        assert!(cache_hit(10, 1_700_000_000.0, 10, 1_700_000_000.000_9));
        assert!(!cache_hit(10, 1_700_000_000.0, 10, 1_700_000_000.002));
        assert!(!cache_hit(10, 1_700_000_000.0, 11, 1_700_000_000.0));
    }

    #[test]
    fn json_numbers_and_strings_as_foundation_writes_them() {
        let cases = [(0.0, "0"), (1.0, "1"), (784_111_777.123456, "784111777.123456"), (-1.0, "-1"), (1e16, "1e+16"), (1e-5, "1e-05"), (-0.0, "-0")];
        for (x, want) in cases {
            assert_eq!(json_number(x).as_deref(), Some(want));
        }
        assert_eq!(json_number(f64::NAN), None);
        assert_eq!(json_string("a/b\"c\\\u{1}\u{1b}\u{7f}é"), "\"a\\/b\\\"c\\\\\\u0001\\u001b\u{7f}é\"");
    }

    #[test]
    fn base64_round_trip() {
        for len in 0..40usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&data)), Some(data));
        }
        assert_eq!(base64_encode(&[0xff, 0xfe]), "//4=");
        assert_eq!(base64_decode("AA=A"), None);
        assert_eq!(base64_decode("AAA"), None);
    }

    #[test]
    fn library_roots_round_trip() {
        let roots = vec![
            LibraryRoot {
                id: Uuid::from_u128(0x6F1C2A4E_9B1D_4C55_8E3A_0123456789AB),
                display_name: "Clips".into(),
                bookmark_data: vec![1, 2, 255],
                date_added: 784_111_777.123456,
            },
            LibraryRoot { id: Uuid::from_u128(7), display_name: "Café/Été".into(), bookmark_data: vec![], date_added: 0.1 },
        ];
        let json = encode_library_roots(&roots).unwrap_or_default();
        assert!(json.starts_with(
            "[{\"id\":\"6F1C2A4E-9B1D-4C55-8E3A-0123456789AB\",\"displayName\":\"Clips\",\"bookmarkData\":\"AQL\\/\",\"dateAdded\":784111777.123456}"
        ));
        assert_eq!(decode_library_roots(json.as_bytes()), Some(roots));
        // Swift's key order varies; any order reads.
        let swift = r#"[{"dateAdded":1e+16,"bookmarkData":"AQL\/","id":"6F1C2A4E-9B1D-4C55-8E3A-0123456789AB","displayName":"a\/b"}]"#;
        let read = decode_library_roots(swift.as_bytes()).unwrap_or_default();
        assert_eq!(read.first().map(|r| (r.display_name.as_str(), r.date_added)), Some(("a/b", 1e16)));
        assert_eq!(decode_library_roots(b"[{\"id\":\"x\"}]"), None);
        assert_eq!(decode_library_roots(b"not json"), None);
    }

    #[test]
    fn small_files() {
        for mode in RootSortMode::ALL {
            assert_eq!(decode_root_sort(Some(encode_root_sort(mode).as_bytes())), mode);
        }
        assert_eq!(encode_root_sort(RootSortMode::DateAddedDesc), "\"dateAddedDesc\"");
        assert_eq!(decode_root_sort(None), RootSortMode::Manual);
        assert_eq!(decode_root_sort(Some(b"\"bogus\"")), RootSortMode::Manual);
        assert!(decode_show_sizes(None));
        assert!(!decode_show_sizes(Some(encode_show_sizes(false).as_bytes())));
        let blobs = vec![vec![], vec![0], vec![0xff, 0xfe]];
        assert_eq!(encode_bookmark_blobs(&blobs), r#"["","AA==","\/\/4="]"#);
        assert_eq!(decode_bookmark_blobs(encode_bookmark_blobs(&blobs).as_bytes()), Some(blobs));
        assert_eq!(encode_bookmark_blobs(&[]), "[]");
    }

    #[test]
    fn sorting_roots() {
        let root = |n: u128, name: &str, date: f64| LibraryRoot { id: Uuid::from_u128(n), display_name: name.into(), bookmark_data: vec![], date_added: date };
        let roots = vec![root(1, "beta", 2.0), root(2, "Alpha", 2.0), root(3, "gamma", 1.0)];
        let names = |v: Vec<LibraryRoot>| v.into_iter().map(|r| r.display_name).collect::<Vec<_>>();
        let none = HashMap::new();
        assert_eq!(names(sort_roots(&roots, RootSortMode::Manual, &none)), ["beta", "Alpha", "gamma"]);
        assert_eq!(names(sort_roots(&roots, RootSortMode::NameAsc, &none)), ["Alpha", "beta", "gamma"]);
        assert_eq!(names(sort_roots(&roots, RootSortMode::NameDesc, &none)), ["gamma", "beta", "Alpha"]);
        assert_eq!(names(sort_roots(&roots, RootSortMode::DateAddedAsc, &none)), ["gamma", "Alpha", "beta"]);
        assert_eq!(names(sort_roots(&roots, RootSortMode::DateAddedDesc, &none)), ["Alpha", "beta", "gamma"]);
        let sizes = HashMap::from([(Uuid::from_u128(1), 5), (Uuid::from_u128(3), 9)]);
        assert_eq!(names(sort_roots(&roots, RootSortMode::SizeAsc, &sizes)), ["beta", "gamma", "Alpha"]);
        assert_eq!(names(sort_roots(&roots, RootSortMode::SizeDesc, &sizes)), ["gamma", "beta", "Alpha"]);
    }

    #[test]
    fn moving_roots_like_swiftui() {
        let mut v = vec!['a', 'b', 'c', 'd', 'e'];
        move_roots(&mut v, &[1, 3], 0);
        assert_eq!(v, ['b', 'd', 'a', 'c', 'e']);
        let mut v = vec!['a', 'b', 'c', 'd', 'e'];
        move_roots(&mut v, &[0], 5);
        assert_eq!(v, ['b', 'c', 'd', 'e', 'a']);
        let mut v = vec!['a', 'b', 'c'];
        move_roots(&mut v, &[0], 2);
        assert_eq!(v, ['b', 'a', 'c']);
    }

    #[test]
    fn standardized_and_canonical_without_the_file_system() {
        assert_eq!(standardized_path("/a/./b//c/"), "/a/b/c");
        assert_eq!(standardized_path("/a/b/../../.."), "/");
        assert_eq!(standardized_path("/Users/nobody-here/Café"), "/Users/nobody-here/Cafe\u{301}");
        assert_eq!(canonical_path("/Users/nobody-here/Cafe\u{301}"), "/Users/nobody-here/Café");
        assert_eq!(last_path_component("/a/b/."), ".");
        assert_eq!(last_path_component("/"), "/");
        assert_eq!(last_path_component("/a/b/"), "b");
    }
}
