//! The per-clip sidecar `.<clip file name>.crate-meta.json`, canonical metadata shared with iCrate, byte for byte as
//! Swift MaCrate writes it (`ClipMetadata`, `SidecarStore`, `SelfWriteTracker`).

use crate::error::{LibraryError, Result};
use crate::json::{self, Json};
use crate::unicode::{file_system_representation, is_prepend, joins_previous};
use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// `ClipMetadata.currentSchemaVersion`.
pub const CURRENT_SCHEMA_VERSION: i64 = 1;

const SUFFIX: &str = ".crate-meta.json";

/// One clip's canonical metadata (`ClipMetadata`). Times are seconds since 1970.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipMetadata {
    pub version: i64,
    pub clip_id: String,
    pub rating: i64,
    pub tags: Vec<String>,
    pub bpm: Option<f64>,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub energy: Option<i64>,
    pub brief: Option<String>,
    pub created_at: f64,
    pub updated_at: f64,
}

impl ClipMetadata {
    /// `ClipMetadata.makeDefault(clipID:now:)`: version 1, rating 0, no tags, every optional unset.
    pub fn new(clip_id: impl Into<String>, now: f64) -> ClipMetadata {
        ClipMetadata {
            version: CURRENT_SCHEMA_VERSION,
            clip_id: clip_id.into(),
            rating: 0,
            tags: Vec::new(),
            bpm: None,
            mood: None,
            genre: None,
            energy: None,
            brief: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// The bytes `ClipMetadata.sidecarEncoder` writes: exactly the eleven keys, `null` for an unset optional,
    /// pretty-printed with sorted keys.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let opt_s = |v: &Option<String>| Json::or_null(v.clone(), Json::String);
        let value = Json::object([
            ("version", Json::Int(self.version)),
            ("clip_id", Json::string(self.clip_id.clone())),
            ("rating", Json::Int(self.rating)),
            ("tags", Json::Array(self.tags.iter().cloned().map(Json::String).collect())),
            ("bpm", Json::or_null(self.bpm, Json::Double)),
            ("mood", opt_s(&self.mood)),
            ("genre", opt_s(&self.genre)),
            ("energy", Json::or_null(self.energy, Json::Int)),
            ("brief", opt_s(&self.brief)),
            ("created_at", Json::Double(self.created_at)),
            ("updated_at", Json::Double(self.updated_at)),
        ]);
        Ok(json::to_pretty_sorted(&value)?.into_bytes())
    }

    /// `ClipMetadata.sidecarDecoder`: `clip_id`, `created_at` and `updated_at` required; the rest optional (absent
    /// or `null` → version 1, rating 0, no tags, unset); unknown keys ignored; any wrong type refuses the whole file.
    pub fn decode(bytes: &[u8]) -> Result<ClipMetadata> {
        let doc = json::parse(bytes).ok_or_else(|| LibraryError::Decode("not JSON".into()))?;
        if !matches!(doc, Json::Object(_)) {
            return Err(LibraryError::Decode("not an object".into()));
        }
        let bad = |key: &str| LibraryError::Decode(format!("\"{key}\" is missing or has the wrong type"));
        let required_f64 = |key: &str| doc.present(key).and_then(Json::as_f64).ok_or_else(|| bad(key));
        let optional = |key: &str| -> Option<&Json> { doc.present(key) };
        let opt_i64 = |key: &str| -> Result<Option<i64>> { optional(key).map(|v| v.as_i64().ok_or_else(|| bad(key))).transpose() };
        let opt_f64 = |key: &str| -> Result<Option<f64>> { optional(key).map(|v| v.as_f64().ok_or_else(|| bad(key))).transpose() };
        let opt_str = |key: &str| -> Result<Option<String>> { optional(key).map(|v| v.as_str().map(str::to_string).ok_or_else(|| bad(key))).transpose() };
        let tags = match optional("tags") {
            None => Vec::new(),
            Some(v) => {
                v.as_array().and_then(|items| items.iter().map(|t| t.as_str().map(str::to_string)).collect::<Option<Vec<_>>>()).ok_or_else(|| bad("tags"))?
            }
        };
        Ok(ClipMetadata {
            version: opt_i64("version")?.unwrap_or(CURRENT_SCHEMA_VERSION),
            clip_id: doc.present("clip_id").and_then(Json::as_str).map(str::to_string).ok_or_else(|| bad("clip_id"))?,
            rating: opt_i64("rating")?.unwrap_or(0),
            tags,
            bpm: opt_f64("bpm")?,
            mood: opt_str("mood")?,
            genre: opt_str("genre")?,
            energy: opt_i64("energy")?,
            brief: opt_str("brief")?,
            created_at: required_f64("created_at")?,
            updated_at: required_f64("updated_at")?,
        })
    }

    /// `fileSizeFromClipID`: the `<size>` after the last `:` of the clip id, if it is a whole number.
    pub fn file_size_from_clip_id(&self) -> Option<i64> {
        let raw = match self.clip_id.split_once(':') {
            Some((_, rest)) => rest,
            None => self.clip_id.as_str(),
        };
        // Swift: `split(separator: ":", maxSplits: 1).last` then `Int64(_)` (an optional sign, digits).
        raw.parse::<i64>().ok()
    }
}

fn path_string(path: &Path) -> String {
    file_system_representation(&path.to_string_lossy())
}

fn split_last(path: &str) -> (&str, &str) {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(i) => (trimmed.get(..i).unwrap_or(""), trimmed.get(i + 1..).unwrap_or("")),
        None => ("", trimmed),
    }
}

fn join(dir: &str, name: &str) -> PathBuf {
    if dir.is_empty() { PathBuf::from(format!("/{name}")) } else { PathBuf::from(format!("{dir}/{name}")) }
}

/// `SidecarStore.sidecarURL(for:).path`: `.<file name>.crate-meta.json` beside the clip, the whole path in the
/// file-system representation `URL` gives it (`café.mov` → `.cafe\u{301}.mov.crate-meta.json`).
pub fn sidecar_path(clip: &Path) -> PathBuf {
    let full = path_string(clip);
    let (dir, name) = split_last(&full);
    join(dir, &format!(".{name}{SUFFIX}"))
}

/// `SidecarStore.clipURLFromSidecar(_:)`: the clip a sidecar names, or `None` when the name is not
/// `.<name>.crate-meta.json` with a non-empty `<name>` (compared by `Character`, as Swift's `hasPrefix` /
/// `hasSuffix` do).
pub fn clip_path_from_sidecar(sidecar: &Path) -> Option<PathBuf> {
    let full = path_string(sidecar);
    let (dir, name) = split_last(&full);
    if !is_sidecar_name(name) {
        return None;
    }
    let inner = name.get(1..name.len() - SUFFIX.len()).filter(|s| !s.is_empty())?;
    Some(join(dir, inner))
}

/// `name.hasPrefix(".") && name.hasSuffix(".crate-meta.json")`, by `Character`.
pub fn is_sidecar_name(name: &str) -> bool {
    let mut chars = name.chars();
    if chars.next() != Some('.') || chars.next().is_some_and(joins_previous) {
        return false;
    }
    if !name.ends_with(SUFFIX) || name.len() < SUFFIX.len() {
        return false;
    }
    // The suffix's first `.` would join a preceding Prepend scalar into one `Character`.
    let before = name.get(..name.len() - SUFFIX.len()).and_then(|s| s.chars().next_back());
    !before.is_some_and(is_prepend)
}

/// `SidecarStore.read(for:)`: `Ok(None)` when no sidecar exists, an error when it exists but cannot be read or
/// decoded.
pub fn read_sidecar(clip: &Path) -> Result<Option<ClipMetadata>> {
    let path = sidecar_path(clip);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    ClipMetadata::decode(&bytes).map(Some)
}

/// `SidecarStore.write(_:for:)`: encode, tell the tracker (before the write, as Swift does), then write atomically
/// through `.<UUID>.crate-meta.tmp` in the same directory and a rename over the sidecar.
pub fn write_sidecar(meta: &ClipMetadata, clip: &Path, tracker: Option<&SelfWriteTracker>) -> Result<PathBuf> {
    let path = sidecar_path(clip);
    let bytes = meta.encode()?;
    if let Some(t) = tracker {
        t.record(&path);
    }
    atomic_write(&bytes, &path)?;
    Ok(path)
}

/// `SidecarStore.delete(for:)`: remove the sidecar if there is one (idempotent).
pub fn delete_sidecar(clip: &Path, tracker: Option<&SelfWriteTracker>) -> Result<()> {
    let path = sidecar_path(clip);
    if !path.exists() {
        return Ok(());
    }
    if let Some(t) = tracker {
        t.record(&path);
    }
    std::fs::remove_file(&path)?;
    Ok(())
}

/// `SidecarStore.move(from:to:)`: move the sidecar along with a renamed clip; nothing to do when the old one does not
/// exist; refuses (as `FileManager.moveItem`) when the destination exists. Both paths are recorded first.
pub fn move_sidecar(old_clip: &Path, new_clip: &Path, tracker: Option<&SelfWriteTracker>) -> Result<()> {
    let old = sidecar_path(old_clip);
    let new = sidecar_path(new_clip);
    if let Some(t) = tracker {
        t.record(&old);
        t.record(&new);
    }
    if !old.exists() {
        return Ok(());
    }
    if std::fs::symlink_metadata(&new).is_ok() {
        return Err(LibraryError::DestinationExists(new));
    }
    std::fs::rename(&old, &new)?;
    Ok(())
}

/// Write `bytes` to `target` so that another reader sees the old file or the new one, never part of one: a
/// `.<UUID>.crate-meta.tmp` beside it, then a rename (the temp file is removed if the rename fails).
pub fn atomic_write(bytes: &[u8], target: &Path) -> Result<()> {
    let dir = target.parent().unwrap_or_else(|| Path::new("/"));
    let temp = dir.join(format!(".{}.crate-meta.tmp", uuid::Uuid::new_v4().to_string().to_uppercase()));
    std::fs::write(&temp, bytes)?;
    if let Err(e) = std::fs::rename(&temp, target) {
        let _ = std::fs::remove_file(&temp);
        return Err(e.into());
    }
    Ok(())
}

fn decode_file(path: &Path) -> Option<ClipMetadata> {
    ClipMetadata::decode(&std::fs::read(path).ok()?).ok()
}

/// Directory extensions macOS treats as packages whatever is installed; `scan_sidecars` does not descend into them.
/// (Swift's enumerator asks LaunchServices, whose answer also depends on the installed apps' declarations: this list
/// covers the system's own package types.)
const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "appex",
    "bundle",
    "component",
    "dSYM",
    "framework",
    "kext",
    "mdimporter",
    "mpkg",
    "musiclibrary",
    "photoslibrary",
    "pkg",
    "playground",
    "plugin",
    "prefPane",
    "qlgenerator",
    "rtfd",
    "saver",
    "scptd",
    "tvlibrary",
    "wdgt",
    "xcarchive",
    "xcodeproj",
    "xcworkspace",
    "xpc",
];

fn is_package(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else { return false };
    PACKAGE_EXTENSIONS.iter().any(|p| p.eq_ignore_ascii_case(ext))
}

/// `SidecarStore.scanSidecars(under:)`: every sidecar below `root` that decodes, keyed by its path. As Swift's
/// `FileManager` enumeration does it: pre-order in directory (`readdir`) order; hidden files and folders included;
/// symbolic links neither followed nor read; package folders not descended into; unreadable folders and undecodable
/// sidecars left out silently. Swift calls `skipDescendants()` on each symbolic link, which Foundation applies to
/// the next folder the walk reaches (even after files in between), so that folder's contents are not scanned either:
/// reproduced here, since it decides which sidecars Swift MaCrate sees.
pub fn scan_sidecars(root: &Path) -> BTreeMap<PathBuf, ClipMetadata> {
    let mut out = BTreeMap::new();
    let mut pending_skip = false;
    walk(root, &mut pending_skip, &mut out);
    out
}

fn walk(dir: &Path, pending_skip: &mut bool, out: &mut BTreeMap<PathBuf, ClipMetadata>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if meta.file_type().is_symlink() {
            *pending_skip = true;
            continue;
        }
        if meta.is_dir() {
            let skip = std::mem::take(pending_skip) || is_package(&path);
            if !skip {
                walk(&path, pending_skip, out);
            }
            continue;
        }
        visit_candidate(&path, out);
    }
}

/// `SidecarStore.scanSidecarsDirect(in:)`: the same, for the immediate contents of `dir` only.
pub fn scan_sidecars_direct(dir: &Path) -> BTreeMap<PathBuf, ClipMetadata> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            continue;
        }
        visit_candidate(&path, &mut out);
    }
    out
}

fn visit_candidate(path: &Path, out: &mut BTreeMap<PathBuf, ClipMetadata>) {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return };
    if !is_sidecar_name(&file_system_representation(name)) {
        return;
    }
    if let Some(meta) = decode_file(path) {
        out.insert(path.to_path_buf(), meta);
    }
}

/// `SelfWriteTracker`: the sidecars this process just wrote, so a file-system watcher can tell its own writes from
/// another process's. An entry counts for 2 seconds and is not consumed by a match; past 64 entries the expired
/// ones are dropped. Keys are compared after `standardizedFileURL` (`.` and `..` resolved lexically).
#[derive(Debug, Default)]
pub struct SelfWriteTracker {
    entries: Mutex<HashMap<PathBuf, Instant>>,
}

const TOLERANCE: Duration = Duration::from_secs(2);
const MAX_ENTRIES: usize = 64;

fn standardized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in Path::new(&path_string(path)).components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

impl SelfWriteTracker {
    pub fn new() -> SelfWriteTracker {
        SelfWriteTracker::default()
    }

    /// Note a write to `path`, now.
    pub fn record(&self, path: &Path) {
        let now = Instant::now();
        let mut entries = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        entries.insert(standardized(path), now);
        if entries.len() > MAX_ENTRIES {
            entries.retain(|_, t| now.duration_since(*t) < TOLERANCE);
        }
    }

    /// Whether `path` was recorded less than 2 seconds ago.
    pub fn is_self_write(&self, path: &Path) -> bool {
        let entries = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        entries.get(&standardized(path)).is_some_and(|t| t.elapsed() < TOLERANCE)
    }
}
