// SPDX-License-Identifier: MIT
//! The files Arena opens, the sort setting, and the expansion of a selection (files and folders)
//! into the ordered media files a load fills consecutive columns with.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs::{self, Metadata};
use std::path::Path;
use std::time::SystemTime;
use unicode_normalization::UnicodeNormalization;

/// File types Arena loads into a clip slot (the file-dialog filters compiled into Arena 7.28.1).
/// `.avc` is a composition, not a clip, and is deliberately absent.
pub struct ArenaMediaTypes;

impl ArenaMediaTypes {
    pub const VIDEO: [&'static str; 7] = ["mov", "mp4", "avi", "mpg", "mpeg", "gif", "wmv"];
    pub const IMAGE: [&'static str; 7] = ["png", "jpg", "jpeg", "tif", "tiff", "psd", "exr"];
    pub const AUDIO: [&'static str; 5] = ["wav", "aiff", "aif", "flac", "ogg"];

    fn is_known(ext: &str) -> bool {
        let e = ext.to_lowercase();
        Self::VIDEO.contains(&e.as_str()) || Self::IMAGE.contains(&e.as_str()) || Self::AUDIO.contains(&e.as_str())
    }

    /// By the path's extension alone.
    pub fn is_supported_path(path: &str) -> bool {
        Self::is_known(&path_extension(path))
    }

    /// A file URL: not a directory (`is_dir`, or a path ending in `/`) and a known extension.
    pub fn is_supported_file(path: &str, is_dir: bool) -> bool {
        !is_dir && !path.ends_with('/') && Self::is_supported_path(path)
    }
}

/// The last path component (`/x/A/` → `A`, `/` → `/`).
pub fn last_path_component(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return if path.is_empty() { String::new() } else { "/".to_string() };
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
}

/// The extension of the last path component, without the dot (`""` for none).
pub fn path_extension(path: &str) -> String {
    Path::new(&last_path_component(path)).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Sort keys for the order files fill consecutive columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    #[default]
    Name,
    DateModified,
    DateCreated,
    DateAdded,
    Size,
    Kind,
}

impl SortKey {
    pub const ALL: [SortKey; 6] = [SortKey::Name, SortKey::DateModified, SortKey::DateCreated, SortKey::DateAdded, SortKey::Size, SortKey::Kind];

    pub fn title(self) -> &'static str {
        match self {
            SortKey::Name => "Name",
            SortKey::DateModified => "Date Modified",
            SortKey::DateCreated => "Date Created",
            SortKey::DateAdded => "Date Added",
            SortKey::Size => "Size",
            SortKey::Kind => "Kind",
        }
    }
}

/// The attributes a sort needs, read once per item.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MediaItem {
    pub path: String,
    pub is_folder: bool,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub added: Option<SystemTime>,
    pub size: Option<u64>,
}

impl MediaItem {
    pub fn new(path: impl Into<String>) -> Self {
        MediaItem { path: path.into(), ..Default::default() }
    }

    pub fn name(&self) -> String {
        last_path_component(&self.path)
    }

    /// Kind = the file extension (Arena's types are all identified by it).
    pub fn kind(&self) -> String {
        if self.is_folder { String::new() } else { path_extension(&self.path).to_lowercase() }
    }

    fn from_metadata(path: &str, meta: Option<&Metadata>, added: &AddedProbe<'_>) -> Self {
        let is_dir = meta.is_some_and(Metadata::is_dir);
        MediaItem {
            path: path.to_string(),
            is_folder: is_dir && !is_package_dir(path),
            modified: meta.and_then(|m| m.modified().ok()),
            created: meta.and_then(|m| m.created().ok()),
            added: added.and_then(|f| f(Path::new(path))),
            size: meta.filter(|m| !m.is_dir()).map(Metadata::len),
        }
    }
}

/// Reads a file's "date added" (a macOS notion; `None` where unknown).
pub type AddedProbe<'a> = Option<&'a (dyn Fn(&Path) -> Option<SystemTime> + Send + Sync)>;

/// Directory extensions treated as packages (opaque, not expanded): application and plug-in
/// bundles and document packages.
const PACKAGE_EXTENSIONS: [&str; 31] = [
    "app",
    "appex",
    "bundle",
    "framework",
    "plugin",
    "kext",
    "xpc",
    "pkg",
    "mpkg",
    "prefpane",
    "qlgenerator",
    "mdimporter",
    "saver",
    "component",
    "vst",
    "vst3",
    "aaxplugin",
    "rtfd",
    "photoslibrary",
    "fcpbundle",
    "logicx",
    "band",
    "playground",
    "xcodeproj",
    "xcworkspace",
    "xcarchive",
    "scptd",
    "sparsebundle",
    "numbers",
    "pages",
    "key",
];

fn is_package_dir(path: &str) -> bool {
    let ext = path_extension(path).to_lowercase();
    !ext.is_empty() && PACKAGE_EXTENSIONS.contains(&ext.as_str())
}

#[cfg(target_os = "macos")]
fn is_hidden_flag(m: &Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    const UF_HIDDEN: u32 = 0x8000;
    m.st_flags() & UF_HIDDEN != 0
}

#[cfg(windows)]
fn is_hidden_flag(m: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(not(any(target_os = "macos", windows)))]
fn is_hidden_flag(_m: &Metadata) -> bool {
    false
}

/// Finder-style name order (`localizedStandardCompare`): case-insensitive, digit runs compared
/// as numbers ("clip 2" before "clip 10"), spaces before punctuation (in the Unicode collation
/// order: `_ - , ; : ! ? . ' " ( ) …`) before digits before letters; ties broken by leading zeros
/// (more zeros later), then accents (the plain letter first), then case (lower case first).
pub fn natural_compare(a: &str, b: &str) -> Ordering {
    let ta = tokens(a);
    let tb = tokens(b);
    let mut zeros = Ordering::Equal;
    for (x, y) in ta.iter().zip(&tb) {
        let o = match (x, y) {
            (Tok::Num(p), Tok::Num(q)) => {
                let o = cmp_digits(p, q);
                if o == Ordering::Equal && zeros == Ordering::Equal {
                    zeros = p.len().cmp(&q.len());
                }
                o
            }
            (Tok::Num(_), Tok::Ch(c)) => (DIGIT_CLASS, 0).cmp(&rank(*c)),
            (Tok::Ch(c), Tok::Num(_)) => rank(*c).cmp(&(DIGIT_CLASS, 0)),
            (Tok::Ch(c), Tok::Ch(d)) => rank(*c).cmp(&rank(*d)),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    ta.len()
        .cmp(&tb.len())
        .then(zeros)
        .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
        .then_with(|| {
            for (x, y) in a.chars().zip(b.chars()) {
                if x != y && x.to_lowercase().eq(y.to_lowercase()) {
                    return if x.is_lowercase() { Ordering::Less } else { Ordering::Greater };
                }
            }
            Ordering::Equal
        })
        .then_with(|| a.cmp(b))
}

const DIGIT_CLASS: u32 = 2;

/// ASCII punctuation and symbols in the Unicode collation order.
const PUNCTUATION: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

/// Currency symbols, which sort after the other symbols.
const CURRENCY: &str = "$\u{a2}\u{a3}\u{a4}\u{a5}\u{20ac}\u{20b9}\u{20bd}\u{20a9}\u{20ba}\u{20ab}\u{e3f}";

/// (class, weight): whitespace 0, punctuation and symbols 1 (currency last), digits 2, letters
/// 3; letters weigh their lower-case base letter (accents removed).
fn rank(c: char) -> (u32, u32) {
    if c.is_whitespace() {
        (0, u32::from(c))
    } else if c.is_alphanumeric() {
        (3, u32::from(base(c)))
    } else if CURRENCY.contains(c) {
        (1, 0x20_0000 + u32::from(c))
    } else if let Some(i) = PUNCTUATION.find(c) {
        (1, u32::try_from(i).unwrap_or(0))
    } else {
        (1, 0x100 + u32::from(c))
    }
}

/// Letters the collation treats as variants of plain ones although Unicode does not decompose
/// them (ß = ss, ø = o, æ = ae …).
fn expand(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{df}' => "ss",
        '\u{f8}' | '\u{d8}' => "o",
        '\u{e6}' | '\u{c6}' => "ae",
        '\u{153}' | '\u{152}' => "oe",
        '\u{111}' | '\u{110}' => "d",
        '\u{142}' | '\u{141}' => "l",
        '\u{131}' => "i",
        _ => return None,
    })
}

enum Tok {
    Num(String),
    Ch(char),
}

fn tokens(s: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut digits = String::new();
    let expanded: String = s.chars().map(|c| expand(c).map_or_else(|| c.to_string(), str::to_string)).collect();
    for c in expanded.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            if !digits.is_empty() {
                out.push(Tok::Num(std::mem::take(&mut digits)));
            }
            out.push(Tok::Ch(c));
        }
    }
    if !digits.is_empty() {
        out.push(Tok::Num(digits));
    }
    out
}

fn cmp_digits(p: &str, q: &str) -> Ordering {
    let p = p.trim_start_matches('0');
    let q = q.trim_start_matches('0');
    p.len().cmp(&q.len()).then_with(|| p.cmp(q))
}

/// The base letter, lower case, accents removed.
fn base(c: char) -> char {
    let first = c.to_string().nfd().next().unwrap_or(c);
    first.to_lowercase().next().unwrap_or(first)
}

pub struct MediaSorter;

impl MediaSorter {
    /// Ascending/descending by `key`; ties (and missing values, which sort as the smallest) fall
    /// back to the natural name order, then the full path.
    pub fn sort(items: &[MediaItem], key: SortKey, ascending: bool) -> Vec<MediaItem> {
        let mut out = items.to_vec();
        out.sort_by(|a, b| {
            let primary = Self::compare(a, b, key);
            if primary != Ordering::Equal {
                return if ascending { primary } else { primary.reverse() };
            }
            natural_compare(&a.name(), &b.name()).then_with(|| a.path.cmp(&b.path))
        });
        out
    }

    fn compare(a: &MediaItem, b: &MediaItem, key: SortKey) -> Ordering {
        match key {
            SortKey::Name => natural_compare(&a.name(), &b.name()),
            SortKey::DateModified => a.modified.cmp(&b.modified),
            SortKey::DateCreated => a.created.cmp(&b.created),
            SortKey::DateAdded => a.added.cmp(&b.added),
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Kind => natural_compare(&a.kind(), &b.kind()),
        }
    }
}

/// The ordered media files a selection (files and/or folders) expands to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MediaCollection {
    pub files: Vec<String>,
    /// Selected folders (top level), in sort order.
    pub folders: Vec<String>,
    /// Each selected folder with the files it expanded to (subfolders included in their
    /// parent), in sort order.
    pub groups: Vec<FolderGroup>,
    /// Files that came out of folders (the large-folder ask counts these).
    pub from_folders: usize,
    pub skipped_hidden: usize,
    pub skipped_non_media: usize,
    /// Subfolders left out because Include Subfolders is off.
    pub subfolders_not_included: usize,
}

impl MediaCollection {
    /// Selected files outside any selected folder.
    pub fn loose_files(&self) -> usize {
        self.files.len().saturating_sub(self.from_folders)
    }

    /// Ask once before loading: folders expanded to more files than `threshold`.
    /// `folders_only` (Folders as New Layers) counts the folders' files only.
    pub fn needs_confirmation(&self, threshold: i64, folders_only: bool) -> bool {
        let n = if folders_only { self.from_folders } else { self.files.len() };
        !self.folders.is_empty() && i64::try_from(n).unwrap_or(i64::MAX) > threshold
    }

    pub fn summary(&self) -> String {
        let n = self.files.len();
        let mut s = format!("{n} file{}", if n == 1 { "" } else { "s" });
        if !self.folders.is_empty() {
            let f = self.folders.len();
            s += &format!(" ({} from {f} folder{})", self.from_folders, if f == 1 { "" } else { "s" });
        }
        let mut skipped: Vec<String> = Vec::new();
        if self.skipped_hidden > 0 {
            skipped.push(format!("{} hidden", self.skipped_hidden));
        }
        if self.skipped_non_media > 0 {
            skipped.push(format!("{} not a type Arena opens", self.skipped_non_media));
        }
        if self.subfolders_not_included > 0 {
            let k = self.subfolders_not_included;
            skipped.push(format!("{k} subfolder{} (Include Subfolders is off)", if k == 1 { "" } else { "s" }));
        }
        if !skipped.is_empty() {
            s += &format!(", skipped {}", skipped.join(", "));
        }
        s
    }
}

/// A selected folder and the media files it expanded to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderGroup {
    pub folder: String,
    pub files: Vec<String>,
}

impl FolderGroup {
    pub fn new(folder: impl Into<String>, files: Vec<String>) -> Self {
        FolderGroup { folder: folder.into(), files }
    }

    /// The layer name: the folder's name.
    pub fn name(&self) -> String {
        last_path_component(&self.folder)
    }
}

/// Expands a selection into the media files Arena accepts, in sort order. The selected items are
/// ordered by the sort setting; each folder expands in place: its own files (sorted), then (with
/// Include Subfolders) each subfolder (sorted) the same way. Hidden items inside folders and
/// anything that is not a type Arena opens are skipped and counted; packages (.app, bundles) count
/// as non-media; symlinked folders are not followed.
pub struct MediaCollector;

impl MediaCollector {
    pub fn collect(paths: &[String], include_subfolders: bool, sort_key: SortKey, ascending: bool) -> MediaCollection {
        Self::collect_with(paths, include_subfolders, sort_key, ascending, None)
    }

    /// As [`MediaCollector::collect`], with a probe for each item's "date added".
    pub fn collect_with(paths: &[String], include_subfolders: bool, sort_key: SortKey, ascending: bool, added: AddedProbe<'_>) -> MediaCollection {
        let mut out = MediaCollection::default();
        let top: Vec<MediaItem> = paths
            .iter()
            .map(|p| {
                let p = standardized(p);
                MediaItem::from_metadata(&p, fs::symlink_metadata(&p).ok().as_ref(), &added)
            })
            .collect();
        for item in MediaSorter::sort(&top, sort_key, ascending) {
            if item.is_folder {
                out.folders.push(item.path.clone());
                let before = out.files.len();
                Self::expand(&item.path, include_subfolders, sort_key, ascending, &added, &mut out);
                out.from_folders += out.files.len() - before;
                out.groups.push(FolderGroup { folder: item.path.clone(), files: out.files[before..].to_vec() });
            } else if ArenaMediaTypes::is_supported_path(&item.path) {
                out.files.push(item.path);
            } else {
                out.skipped_non_media += 1;
            }
        }
        out
    }

    fn expand(folder: &str, recursive: bool, sort_key: SortKey, ascending: bool, added: &AddedProbe<'_>, out: &mut MediaCollection) {
        let Ok(entries) = fs::read_dir(folder) else { return };
        let mut files: Vec<MediaItem> = Vec::new();
        let mut subfolders: Vec<MediaItem> = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = if folder.ends_with('/') { format!("{folder}{name}") } else { format!("{folder}/{name}") };
            let v = fs::symlink_metadata(&child).ok();
            if v.as_ref().is_some_and(is_hidden_flag) || name.starts_with('.') {
                out.skipped_hidden += 1;
                continue;
            }
            if v.as_ref().is_some_and(|m| m.file_type().is_symlink()) {
                // Follow links to files only (a linked folder could loop).
                let tv = fs::metadata(&child).ok();
                if tv.as_ref().is_some_and(Metadata::is_dir) {
                    continue;
                }
                if ArenaMediaTypes::is_supported_path(&child) {
                    files.push(MediaItem::from_metadata(&child, tv.as_ref(), added));
                } else {
                    out.skipped_non_media += 1;
                }
                continue;
            }
            let item = MediaItem::from_metadata(&child, v.as_ref(), added);
            if item.is_folder {
                if recursive {
                    subfolders.push(item);
                } else {
                    out.subfolders_not_included += 1;
                }
            } else if ArenaMediaTypes::is_supported_path(&item.path) {
                files.push(item);
            } else {
                out.skipped_non_media += 1;
            }
        }
        out.files.extend(MediaSorter::sort(&files, sort_key, ascending).into_iter().map(|i| i.path));
        for sub in MediaSorter::sort(&subfolders, sort_key, ascending) {
            Self::expand(&sub.path, true, sort_key, ascending, added, out);
        }
    }
}

/// A path without its trailing slashes (`URL(fileURLWithPath:).path`).
fn standardized(p: &str) -> String {
    let t = p.trim_end_matches('/');
    if t.is_empty() && p.starts_with('/') { "/".to_string() } else { t.to_string() }
}

/// A menu selection split into media files, folders and the rest. Folders are told apart by
/// their metadata (a directory, not a package), or a trailing `/` when it cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MediaSelection {
    pub files: Vec<String>,
    pub folders: Vec<String>,
    pub unsupported: usize,
}

impl MediaSelection {
    pub fn new<S: AsRef<str>>(paths: &[S]) -> Self {
        let mut sel = MediaSelection::default();
        for p in paths {
            let p = p.as_ref();
            let is_dir = match fs::symlink_metadata(p) {
                Ok(m) => m.is_dir(),
                Err(_) => p.ends_with('/'),
            };
            if is_dir && !is_package_dir(p) {
                sel.folders.push(standardized(p));
            } else if ArenaMediaTypes::is_supported_file(p, is_dir) {
                sel.files.push(p.to_string());
            } else {
                sel.unsupported += 1;
            }
        }
        sel
    }
}
