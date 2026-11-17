//! Folder enumeration as Swift MaCrate does it (CrateKit/FileEnumerator.swift, VideoFileFilter.swift): which files
//! are clips, the one-level listing and the recursive walk, the order they come back in, and each clip's size and
//! modification date exactly as Foundation reports them — the size and date are half of the indexer's cache-hit
//! rule ([`crate::roots::cache_hit`]) and the date is part of the thumbnail key ([`crate::thumbkey`]).
//!
//! What Foundation decides and how it is reproduced here (measured against Swift MaCrate's own code with
//! tools/swift-library-oracle, docs/data/m1/files-crosstest.txt):
//! - **Paths.** The walk reports each clip under the folder's real path (symbolic links and `/tmp`-style aliases in
//!   the folder's own path resolved, names in their on-disk case and Unicode form), then the entry's on-disk name:
//!   [`std::fs::canonicalize`] of the folder. A folder that is itself a symbolic link is not followed: the listing
//!   fails and the walk yields nothing. A listed URL's `path` does not go through the decomposition
//!   `URL(fileURLWithPath:)` applies ([`crate::unicode::file_system_representation`]): a name stored composed comes
//!   back composed, byte for byte (measured: `café.mov` written NFC and NFD, Swedish and Vietnamese letters, the
//!   test media's names). So `clips.current_path` holds the on-disk form; paths Swift rebuilds from a string
//!   (sidecar URLs, the FTS file name, [`crate::thumbkey::thumbnail_key_for`]) are decomposed.
//! - **Hidden** entries are skipped (and never entered): a name starting with `.`, or the `UF_HIDDEN` flag.
//! - **Symbolic links** are skipped and never followed.
//! - **Packages** (`.skipsPackageDescendants`): a folder Finder shows as one item is not entered. Foundation asks
//!   Launch Services; here the folder's extension is looked up in [`PACKAGE_EXTENSIONS`], the 177 extensions that
//!   are packages on the home Mac (macOS 27.0.1, measured one by one). Not reproduced: a folder whose Finder
//!   "bundle" bit is set (an extended attribute, unreadable without `unsafe`; Launch Services through
//!   `glance-platform` can answer both later), and package types an app installed after the measurement declares.
//! - **Order**: [`localized_case_insensitive_cmp`], a stable sort, so names that compare equal keep the walk's
//!   order (directory order, depth first, a folder's contents right after the folder).
//! - **Date**: [`foundation_mtime`], Core Foundation's conversion of the `stat` time to a `Date`.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use unicode_normalization::UnicodeNormalization;

/// The clip extensions (`VideoFileFilter.extensions`), compared lower-cased. Nothing else decides: no content
/// sniff, no minimum size.
pub const VIDEO_EXTENSIONS: [&str; 7] = ["mov", "mp4", "m4v", "avi", "mkv", "mxf", "webm"];

/// Folder extensions macOS treats as packages (`URLResourceValues.isPackage` true for an empty folder of that name;
/// every extension Launch Services lists on the home Mac, tested one by one). Compared lower-cased.
pub const PACKAGE_EXTENSIONS: [&str; 177] = [
    "abbu",
    "action",
    "aimodel",
    "aimodelc",
    "aipackage",
    "aiperf",
    "aplibrary",
    "aplmodel",
    "app",
    "appdownload",
    "appex",
    "avchd",
    "backupbundle",
    "band",
    "book",
    "bundle",
    "caction",
    "cannedsearch",
    "cmm",
    "cotsettings",
    "cotsyntax",
    "definition",
    "dext",
    "dist",
    "distz",
    "doccarchive",
    "docset",
    "download",
    "dsym",
    "dutmp",
    "dvdmedia",
    "dvtplugin",
    "dwg",
    "dxf",
    "epub",
    "fcpproj",
    "fldownload",
    "fmadapter",
    "folx",
    "fs",
    "gamesavebundle",
    "gbproj",
    "gputrace",
    "gtdbg",
    "hdd",
    "hds",
    "help",
    "hvpl",
    "iba",
    "ibooks",
    "ibplugin",
    "icbu",
    "icon",
    "ideplugin",
    "ilmbplugin",
    "imovielibrary",
    "instrdst",
    "internetconnect",
    "ite",
    "itlp",
    "kext",
    "key",
    "key-tef",
    "kpf",
    "kth",
    "logarchive",
    "lpdf",
    "macvm",
    "magnifier",
    "mdimporter",
    "menu",
    "migratedaplibrary",
    "migratedphotolibrary",
    "mlmodelc",
    "mlpackage",
    "mlperf",
    "mlproj",
    "movpkg",
    "mpkg",
    "mpsgraphpackage",
    "mtlpackage",
    "musiclibrary",
    "mxo",
    "networkconnect",
    "nib",
    "nmbtemplate",
    "notesarchive",
    "numbers",
    "numbers-tef",
    "octest",
    "order",
    "osax",
    "p10",
    "p12",
    "p7",
    "p7b",
    "p7m",
    "p7r",
    "p7s",
    "pages",
    "pages-tef",
    "pbproj",
    "pem",
    "pfx",
    "photolibrary",
    "photosasset",
    "photoslibrary",
    "pkcs12",
    "pkg",
    "pkpass",
    "placeholder",
    "playground",
    "plugin",
    "pluginkit",
    "ppp",
    "prefpane",
    "pvm",
    "pvmz",
    "pvt",
    "qlgenerator",
    "qtpxcomposition",
    "remoteplaceholder",
    "rge",
    "rtfd",
    "saver",
    "scptd",
    "seplugin",
    "service",
    "slidesaver",
    "sparsebundle",
    "spreporter",
    "ssdownload",
    "stickiesappexport",
    "storyboardc",
    "suggestionsassets",
    "swiftpm",
    "swproj",
    "synscene",
    "sysprefex",
    "systemextension",
    "template",
    "theater",
    "trace",
    "tvlibrary",
    "videoslibrary",
    "viewhierarchy",
    "vst",
    "vstinfo",
    "wdgt",
    "webplugin",
    "widgetkitsim",
    "workflow",
    "xcappdata",
    "xcarchive",
    "xccrashpoint",
    "xcdatamodel",
    "xcdatamodeld",
    "xcdiskwritepoint",
    "xcfeedbackpoint",
    "xcfilescontainer",
    "xchangpoint",
    "xclaunchpoint",
    "xcloc",
    "xcmappingmodel",
    "xcode",
    "xcodeproj",
    "xcodesettings",
    "xcplaygroundpage",
    "xcplugin",
    "xcprocessorusagepoint",
    "xcresult",
    "xcsnapshots",
    "xctest",
    "xctestproducts",
    "xctoolchain",
    "xcworkspace",
    "xpc",
];

/// One clip file found by a listing (`ClipFile` as `FileEnumerator.makeClip` fills it).
#[derive(Clone, Debug, PartialEq)]
pub struct ClipFile {
    /// The path as Swift's `clip.url.path` gives it — what `clips.current_path` stores.
    pub path: String,
    /// The file name (`url.lastPathComponent`), on-disk Unicode form.
    pub display_name: String,
    /// Bytes (`URLResourceValues.fileSize`).
    pub size: i64,
    /// Seconds since 1970 ([`foundation_mtime`]); `Date.distantPast` when the system gives none.
    pub mtime: f64,
}

/// `Date.distantPast` as seconds since 1970 (`-63114076800` since 2001).
pub const DISTANT_PAST: f64 = -63_114_076_800.0 + SECONDS_1970_TO_2001;

/// Seconds from 1970-01-01 to 2001-01-01, Foundation's reference date (`kCFAbsoluteTimeIntervalSince1970`).
pub const SECONDS_1970_TO_2001: f64 = 978_307_200.0;

/// The modification date Foundation gives a file, as seconds since 1970 (`contentModificationDate` /
/// `attributesOfItem[.modificationDate]` → `timeIntervalSince1970`): Core Foundation turns `stat`'s
/// `(tv_sec, tv_nsec)` into reference-date seconds `(tv_sec - 978307200) + 1e-9 * tv_nsec` and `Date` adds the
/// 978307200 back, both in `f64` — not `tv_sec + tv_nsec / 1e9`, which differs in the last bit for about a third of
/// all times (measured on 315 dates from 1906 to 9999: 0 differences with this formula, 91 with the naive one).
/// `None` off Unix.
pub fn foundation_mtime(meta: &std::fs::Metadata) -> Option<f64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(foundation_time(meta.mtime(), meta.mtime_nsec()))
    }
    #[cfg(not(unix))]
    {
        let since = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(foundation_time(i64::try_from(since.as_secs()).ok()?, i64::from(since.subsec_nanos())))
    }
}

/// [`foundation_mtime`] from a `stat` time.
pub fn foundation_time(sec: i64, nsec: i64) -> f64 {
    ((sec as f64 - SECONDS_1970_TO_2001) + 1.0e-9 * nsec as f64) + SECONDS_1970_TO_2001
}

/// Whether `name` (a file name or path) has a clip extension: [`path_extension`] lower-cased (Unicode lower case,
/// as Swift's `lowercased()`), in [`VIDEO_EXTENSIONS`].
pub fn is_video(name: &str) -> bool {
    path_extension(name).is_some_and(|ext| {
        let lower = ext.to_lowercase();
        VIDEO_EXTENSIONS.contains(&lower.as_str())
    })
}

/// Foundation's `URL.pathExtension` of the last component of `name` (after its last `.`, which must not start
/// the name); `None` for no extension.
pub fn path_extension(name: &str) -> Option<&str> {
    let last = name.trim_end_matches('/').rsplit('/').next().unwrap_or(name);
    let dot = last.rfind('.')?;
    let ext = last.get(dot + 1..)?;
    (dot > 0 && !ext.is_empty()).then_some(ext)
}

/// Whether a folder named `name` is a package ([`PACKAGE_EXTENSIONS`]).
pub fn is_package_name(name: &str) -> bool {
    path_extension(name).is_some_and(|ext| {
        let lower = ext.to_lowercase();
        PACKAGE_EXTENSIONS.contains(&lower.as_str())
    })
}

/// Hidden as `.skipsHiddenFiles` has it: a name starting with `.`, or the `UF_HIDDEN` flag (`chflags hidden`).
pub fn is_hidden(name: &str, meta: &std::fs::Metadata) -> bool {
    name.starts_with('.') || hidden_flag(meta)
}

#[cfg(target_os = "macos")]
fn hidden_flag(meta: &std::fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    const UF_HIDDEN: u32 = 0x8000;
    meta.st_flags() & UF_HIDDEN != 0
}

#[cfg(not(target_os = "macos"))]
fn hidden_flag(_meta: &std::fs::Metadata) -> bool {
    false
}

/// `FileEnumerator.directVideos(in:)`: the clips directly inside `dir`, sorted. An error when `dir` cannot be
/// listed — missing, not a folder, unreadable, or a symbolic link (Foundation does not follow a link to the folder
/// itself).
pub fn direct_videos(dir: &Path) -> std::io::Result<Vec<ClipFile>> {
    let base = listing_base(dir).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotADirectory, "not a folder that can be listed"))?;
    let mut clips = Vec::new();
    for entry in std::fs::read_dir(&base)? {
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        if is_hidden(&name, &meta) || meta.file_type().is_symlink() || !is_video(&name) {
            continue;
        }
        if let Some(clip) = make_clip(&base, name, &meta) {
            clips.push(clip);
        }
    }
    sort_by_name(&mut clips);
    Ok(clips)
}

/// What a walk reports along the way: called with the running count about every 250 ms or every 500 clips
/// (Swift's throttle), and once at the end.
pub type Progress<'a> = &'a mut dyn FnMut(usize);

/// `FileEnumerator.recursiveVideos(in:excludedPaths:progress:)`: every clip under `root`, sorted. Hidden entries,
/// packages and symbolic links are skipped and never entered; a folder whose [`crate::roots::standardized_path`] is
/// in `excluded` (the exclusion set's own form) is not entered. Unreadable folders are passed over. A `root` that
/// is missing, not a folder or a symbolic link yields nothing (Foundation's enumerator does the same). `cancelled`
/// is polled once per entry; a cancelled walk returns what it found so far, sorted.
pub fn recursive_videos(root: &Path, excluded: &HashSet<String>, mut progress: Option<Progress<'_>>, cancelled: &dyn Fn() -> bool) -> Vec<ClipFile> {
    let mut clips = Vec::new();
    let Some(base) = listing_base(root) else {
        if let Some(p) = progress.as_mut() {
            p(0);
        }
        return clips;
    };
    let mut last_report = Instant::now();
    let mut stack: Vec<Frame> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&base) {
        stack.push(Frame { dir: base, entries, skip_next_folder: false });
    }
    while let Some(frame) = stack.last_mut() {
        let Some(entry) = frame.entries.next() else {
            stack.pop();
            continue;
        };
        if cancelled() {
            break;
        }
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        let dir = frame.dir.clone();
        if is_hidden(&name, &meta) {
            if meta.is_dir() {
                frame.skip_next_folder = false;
            }
            continue;
        }
        if meta.file_type().is_symlink() {
            // Swift calls `skipDescendants()` on the link; Foundation applies it to the next folder of this
            // listing instead (see `Frame`).
            frame.skip_next_folder = true;
            continue;
        }
        if meta.is_dir() {
            if std::mem::take(&mut frame.skip_next_folder) {
                continue;
            }
            let path = dir.join(&name);
            let excluded_here = !excluded.is_empty() && path.to_str().is_some_and(|p| excluded.contains(&crate::roots::standardized_path(p)));
            if !excluded_here
                && !is_package_name(&name)
                && let Ok(entries) = std::fs::read_dir(&path)
            {
                stack.push(Frame { dir: path, entries, skip_next_folder: false });
            }
            // A folder named like a clip ("x.mov/") is not a regular file: nothing to add.
            continue;
        }
        if !is_video(&name) {
            continue;
        }
        if let Some(clip) = make_clip(&dir, name, &meta) {
            clips.push(clip);
            if let Some(p) = progress.as_mut()
                && (clips.len().is_multiple_of(500) || last_report.elapsed() > Duration::from_millis(250))
            {
                p(clips.len());
                last_report = Instant::now();
            }
        }
    }
    if let Some(p) = progress.as_mut() {
        p(clips.len());
    }
    sort_by_name(&mut clips);
    clips
}

/// One folder of the walk, read in directory order.
///
/// `skip_next_folder` reproduces a Foundation quirk Swift MaCrate's walk runs into: it calls `skipDescendants()`
/// on every symbolic link to stay out of it, and for an entry that is not a folder Foundation keeps that request
/// and applies it to the next folder entry of the same listing — hidden folders and packages included — which is
/// then not entered; the request lapses at the end of the listing. So a folder that comes right after a link in
/// directory order (with no other folder between them) is not walked by Swift MaCrate, and is not walked here.
/// Measured on 500 generated trees (300 flat, 200 nested; tools/swift-library-oracle, `enum` and `walk`): this
/// rule matches every one; neither "the next folder anywhere" nor "only folders that would be entered" does.
struct Frame {
    dir: PathBuf,
    entries: std::fs::ReadDir,
    skip_next_folder: bool,
}

/// The real path a listing of `dir` reports its entries under; `None` when `dir` is missing, a symbolic link or
/// not a folder.
fn listing_base(dir: &Path) -> Option<PathBuf> {
    let own = std::fs::symlink_metadata(dir).ok()?;
    if !own.is_dir() {
        return None;
    }
    std::fs::canonicalize(dir).ok()
}

/// `FileEnumerator.makeClip`: regular files only.
fn make_clip(dir: &Path, name: String, meta: &std::fs::Metadata) -> Option<ClipFile> {
    if !meta.is_file() {
        return None;
    }
    let path = dir.join(&name).to_str()?.to_string();
    Some(ClipFile { path, display_name: name, size: i64::try_from(meta.len()).unwrap_or(i64::MAX), mtime: foundation_mtime(meta).unwrap_or(DISTANT_PAST) })
}

/// `FileEnumerator.sortByName`: [`localized_case_insensitive_cmp`] on the display name, stable.
pub fn sort_by_name(clips: &mut [ClipFile]) {
    // Stable, like Swift's sort; each name's key is computed once.
    clips.sort_by_cached_key(|c| collation_key(&c.display_name));
}

/// Swift's `a.localizedCaseInsensitiveCompare(b)` for the user's locale (Core Foundation over ICU's root collation:
/// the home Mac runs `en_SE`, which collates as English), reproduced as a Unicode Collation Algorithm comparison at
/// the secondary level:
/// - case and width are ignored, as are compatibility forms (`ﬁ` = `fi`, `１` = `1`, `Ⓐ` = `a`, `½` = `1⁄2`), and
///   hiragana equals katakana;
/// - punctuation and symbols are not ignored and sort before digits, digits before letters, in the default table's
///   order (` ` `_` `-` `,` `;` `:` `!` `?` `.` `'` `"` `(` `)` `[` `]` `{` `}` `@` `*` `/` `\` `&` `#` `%` `` ` ``
///   `^`, other symbols, `+` `<` `=` `>` `|` `~`, emoji, currency); digits compare one by one (`clip10` before
///   `clip2`); Latin before Greek, Cyrillic, Hebrew, Arabic, Thai, Hangul, kana, Han;
/// - accents count after the letters (`ete` < `été` < `etf`), in the default table's order (acute, grave, breve,
///   circumflex, caron, ring, diaeresis, double acute, tilde, dot); `ß` = `ss`, `æ` / `œ` = `ae` / `oe` plus an
///   accent-level difference, `ø` `đ` `ł` `ħ` = `o` `d` `l` `h` plus one, `ı` after `i`, `þ` after `z`.
///
/// Residual differences (docs/data/m1/files-crosstest.txt): Core Foundation compares the case-folded strings first
/// and hands only the rest from the first difference to ICU, which changes a few comparisons around `ß` / `ss`
/// (`ßa` sorts after `ssb` in Swift); scripts outside the ones above sort by code point.
pub fn localized_case_insensitive_cmp(a: &str, b: &str) -> Ordering {
    collation_key(a).cmp(&collation_key(b))
}

/// Compared field by field: every primary weight first, then every secondary one.
#[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    primary: Vec<u32>,
    secondary: Vec<u16>,
}

/// Secondary weight of a base character (no accent).
const BASE: u16 = 0x20;

/// Punctuation, symbols and currency in the order Swift sorts them (measured: each character followed by `a`,
/// sorted by `localizedCaseInsensitiveCompare` on macOS 27, tools/swift-library-oracle `sort`). A character here
/// weighs by its place, before any compatibility decomposition (`℃` stays a symbol, `‼` is not `!!`).
const SYMBOL_ORDER: &str = "\u{2028}\u{2029}‾_‗-‐‑‒–—―⁓,;;⁏:‼⁉!¡⁈⁇?¿‽…‥.․··⁕⁖⁘⁙⁚⁛⁜⁝⁞'‘’‚‛‹›\"“”„‟«»()[]{}⁅⁆‖§¶⁋@*⁎⁑/\\&⁊#%‰‱†‡•‣‧⁃⁌⁍⁗‴″′‷‶‵‸※‿⁔⁀⁐⁁⁂`´΄^¯¨΅¸ʹ͵°℃℉©®℄℈℔℗℘℞℟℣℥℧℩℮℺⅁⅂⅃⅄⅊⅌⅏←↚→↛↑↓↔↕↖↗↘↙↜↝↞↟∀∁∂∃∄∅∆∇∈∉∊∋∌∍∎∏⅀+±÷×<=>¬|¦~⁒⁄⅋─━│┃┄┅┆┇┈┉┊┋┌┍┎┏■□▢▣▤▥▦▧▨▩▪▫▬▭▮▯☀☁☂☃☄★☆☇☈☉☊☋☌☍☎☏😀😁😂😃😄😅😆😇😈😉😊😋😌😍😎😏¤¢$£¥₠₡₢₣₤₥₦₩₪₫€₭₮₯₰₱₲₳₴₵₶₷₸₹₺₻₼₽₾₿";

fn symbol_rank(c: char) -> Option<u32> {
    static RANKS: std::sync::OnceLock<HashMap<char, u32>> = std::sync::OnceLock::new();
    RANKS.get_or_init(|| SYMBOL_ORDER.chars().enumerate().map(|(i, c)| (c, i as u32)).collect()).get(&c).copied()
}

/// Latin letters without a decomposition, each sorting after the letter it follows in Swift's order (measured as
/// [`SYMBOL_ORDER`]): (letter, the ASCII letter before it, place after that letter).
const LATIN_EXTRA: &[(char, char, u32)] = &[
    ('ⱥ', 'a', 1),
    ('ƀ', 'b', 1),
    ('ɓ', 'b', 2),
    ('ƃ', 'b', 3),
    ('ȼ', 'c', 1),
    ('ƈ', 'c', 2),
    ('ȸ', 'd', 1),
    ('ɖ', 'd', 2),
    ('ɗ', 'd', 3),
    ('ƌ', 'd', 4),
    ('ȡ', 'd', 5),
    ('ɇ', 'e', 1),
    ('ǝ', 'e', 2),
    ('ə', 'e', 3),
    ('ɛ', 'e', 4),
    ('ƒ', 'f', 1),
    ('ⅎ', 'f', 2),
    ('ǥ', 'g', 1),
    ('ɠ', 'g', 2),
    ('ɣ', 'g', 3),
    ('ƣ', 'g', 4),
    ('ƕ', 'h', 1),
    ('ı', 'i', 1),
    ('ɨ', 'i', 2),
    ('ɩ', 'i', 3),
    ('ȷ', 'j', 1),
    ('ɉ', 'j', 2),
    ('ƙ', 'k', 2),
    ('ƚ', 'l', 1),
    ('ȴ', 'l', 2),
    ('ƛ', 'l', 3),
    ('ɲ', 'n', 1),
    ('ƞ', 'n', 2),
    ('ȵ', 'n', 3),
    ('ŋ', 'n', 4),
    ('ɔ', 'o', 1),
    ('ɵ', 'o', 2),
    ('ȣ', 'o', 3),
    ('ƥ', 'p', 1),
    ('ȹ', 'q', 1),
    ('ɋ', 'q', 2),
    ('ĸ', 'q', 3),
    ('ʀ', 'r', 1),
    ('ɍ', 'r', 2),
    ('ȿ', 's', 1),
    ('ʃ', 's', 2),
    ('ƪ', 's', 3),
    ('ƾ', 't', 1),
    ('ŧ', 't', 2),
    ('ⱦ', 't', 3),
    ('ƫ', 't', 4),
    ('ƭ', 't', 5),
    ('ʈ', 't', 6),
    ('ȶ', 't', 7),
    ('ʉ', 'u', 1),
    ('ɯ', 'u', 2),
    ('ʊ', 'u', 3),
    ('ʋ', 'v', 1),
    ('ʌ', 'v', 2),
    ('ɏ', 'y', 1),
    ('ƴ', 'y', 2),
    ('ȝ', 'y', 3),
    ('ƍ', 'z', 1),
    ('ƶ', 'z', 2),
    ('ȥ', 'z', 3),
    ('ɀ', 'z', 4),
    ('ʒ', 'z', 5),
    ('ƹ', 'z', 6),
    ('ƺ', 'z', 7),
    ('þ', 'z', 8),
    ('ƿ', 'z', 9),
    ('ƻ', 'z', 10),
    ('ƨ', 'z', 11),
    ('ƽ', 'z', 12),
    ('ƅ', 'z', 13),
    ('ɂ', 'z', 14),
    ('ǀ', 'z', 15),
    ('ǁ', 'z', 16),
    ('ǂ', 'z', 17),
    ('ǃ', 'z', 18),
];

/// Cyrillic letters without a decomposition in Swift's order (`й` is a letter of its own, not `и` + breve;
/// `ё` `ї` `ѓ` `ќ` `ў` `ѐ` `ѝ` decompose and sort as their base letter plus an accent).
const CYRILLIC_ORDER: &str = "абвгґдђеєжзѕиійјклљмнњопрстћуфхцчџшщъыьэюя";

/// The primary weight of a letter that takes no decomposition (`None`: decompose it).
fn letter_rank(c: char) -> Option<u32> {
    if c.is_ascii() {
        return None;
    }
    static RANKS: std::sync::OnceLock<HashMap<char, u32>> = std::sync::OnceLock::new();
    RANKS
        .get_or_init(|| {
            let latin_extra = LATIN_EXTRA.iter().map(|&(l, base, place)| (l, latin(base) + place));
            let cyrillic = CYRILLIC_ORDER.chars().enumerate().map(|(i, l)| (l, G_SCRIPT | (2 << 20) | i as u32));
            latin_extra.chain(cyrillic).collect()
        })
        .get(&c)
        .copied()
}

fn latin(l: char) -> u32 {
    G_LATIN | (u32::from(l).saturating_sub(u32::from('a')) << 8)
}

fn collation_key(s: &str) -> Key {
    let mut key = Key::default();
    for c in s.chars() {
        if let Some(rank) = symbol_rank(c) {
            key.primary.push(G_PUNCT | rank);
            key.secondary.push(BASE);
            continue;
        }
        let mut lower = c.to_lowercase();
        if let (Some(l), None) = (lower.next(), lower.next())
            && let Some(p) = letter_rank(l)
        {
            key.primary.push(p);
            key.secondary.push(BASE);
            continue;
        }
        for d in std::iter::once(c).nfkd() {
            if let Some(w) = accent_weight(d) {
                key.secondary.push(w);
                continue;
            }
            for l in d.to_lowercase() {
                push_letter(&mut key, l);
            }
        }
    }
    key
}

/// Stands for the stroke of `ø` `đ` `ł` `ħ` in the accent order (between cedilla and ogonek, measured).
const STROKE: char = '\u{0338}';

/// The secondary weight of a combining mark (DUCET order for the common ones), `None` for any other character.
fn accent_weight(c: char) -> Option<u16> {
    let order = [
        '\u{0301}', '\u{0300}', '\u{0306}', '\u{0302}', '\u{030C}', '\u{030A}', '\u{0308}', '\u{030B}', '\u{0303}', '\u{0307}', '\u{0327}', STROKE, '\u{0328}',
        '\u{0304}',
    ];
    if let Some(i) = order.iter().position(|&m| m == c) {
        return Some(BASE + 1 + i as u16);
    }
    let combining = unicode_normalization::char::is_combining_mark(c);
    combining.then(|| 0x100u16.saturating_add((u32::from(c) & 0x7FFF) as u16))
}

/// Primary groups, in the default table's order.
const G_SPACE: u32 = 1 << 24;
const G_PUNCT: u32 = 2 << 24;
const G_SYMBOL: u32 = 3 << 24;
const G_CURRENCY: u32 = 4 << 24;
const G_DIGIT: u32 = 5 << 24;
const G_LATIN: u32 = 6 << 24;
const G_SCRIPT: u32 = 7 << 24;
const G_OTHER: u32 = 8 << 24;

fn push_letter(key: &mut Key, c: char) {
    let mut one = |p: u32, s: u16| {
        key.primary.push(p);
        key.secondary.push(s);
    };
    match c {
        'a'..='z' => one(latin(c), BASE),
        'ß' => {
            one(latin('s'), BASE);
            one(latin('s'), BASE);
        }
        'æ' => {
            one(latin('a'), BASE + 0x40);
            one(latin('e'), BASE);
        }
        'œ' => {
            one(latin('o'), BASE + 0x40);
            one(latin('e'), BASE);
        }
        'ø' => one(latin('o'), accent_weight(STROKE).unwrap_or(BASE)),
        'đ' => one(latin('d'), accent_weight(STROKE).unwrap_or(BASE)),
        'ł' => one(latin('l'), accent_weight(STROKE).unwrap_or(BASE)),
        'ħ' => one(latin('h'), accent_weight(STROKE).unwrap_or(BASE)),
        'ð' => one(latin('d'), BASE + 0x42),
        '0'..='9' => one(G_DIGIT | (u32::from(c) - u32::from('0')), BASE),
        _ => one(letter_rank(c).unwrap_or_else(|| primary_of(c)), BASE),
    }
}

/// The primary weight of a character that is not an ASCII letter or digit.
fn primary_of(c: char) -> u32 {
    let cp = u32::from(c);
    if c.is_whitespace() || c.is_control() {
        return G_SPACE | cp;
    }
    if let Some(rank) = symbol_rank(c) {
        return G_PUNCT | rank;
    }
    if (0x20A0..=0x20CF).contains(&cp) {
        return G_CURRENCY | 0x100 | cp;
    }
    if c.is_alphabetic() {
        return script_primary(c);
    }
    if c.is_numeric() {
        return G_DIGIT | 0x1000 | cp;
    }
    if cp < 0x2000 && !c.is_alphanumeric() && is_punctuation_like(c) {
        return G_PUNCT | 0x1000 | cp;
    }
    // Symbols: Latin-1 and other modifier / other symbols before the math group, emoji and the rest after it.
    if cp < 0x2000 { G_SYMBOL | cp } else { G_SYMBOL | 0x20_0000 | cp }
}

/// General punctuation outside ASCII (`¡ ¿ « » ‐ – — ‘ ’ “ ” …`) — what Unicode calls P*.
fn is_punctuation_like(c: char) -> bool {
    matches!(c, '¡' | '¿' | '«' | '»' | '§' | '¶' | '·' | '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205E}')
}

/// Letters outside ASCII Latin: script by script in the default table's order, code point order inside a script
/// (katakana weighs as hiragana).
fn script_primary(c: char) -> u32 {
    let cp = u32::from(c);
    let (rank, at) = match cp {
        0x00C0..=0x024F | 0x1E00..=0x1EFF => (0, cp), // Latin letters NFKD left whole (rare)
        0x0370..=0x03FF | 0x1F00..=0x1FFF => (1, cp),
        0x0400..=0x052F => (2, cp),
        0x0530..=0x058F => (3, cp),
        0x0590..=0x05FF => (4, cp),
        0x0600..=0x06FF | 0x0750..=0x077F => (5, cp),
        0x0900..=0x0DFF => (6, cp),
        0x0E00..=0x0E7F => (7, cp),
        0x0E80..=0x10FF => (8, cp),
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => (9, cp),
        0x3040..=0x309F => (10, cp),
        0x30A0..=0x30FF => (10, cp - 0x60),
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0x20000..=0x3FFFF => (12, cp),
        _ => (11, cp),
    };
    if rank == 0 {
        return G_LATIN | 0x1F00 | cp;
    }
    if rank == 12 {
        return G_OTHER | cp;
    }
    G_SCRIPT | (rank << 20) | (at & 0xF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_extensions_are_case_insensitive_and_foundation_shaped() {
        for yes in ["a.mov", "clip.MKV", "x.MoV", "a..mov", " .mov", "x.txt.mov", "dir/x.webm", "x.MXF"] {
            assert!(is_video(yes), "{yes}");
        }
        for no in ["notes.txt", "mov", ".mov", "x.mov.", "x. mov", "x.mov ", "x.mov.txt", "cover.heic", "x.movie"] {
            assert!(!is_video(no), "{no}");
        }
    }

    #[test]
    fn foundation_dates_round_like_core_foundation() {
        // Swift: 1 ns after 1970 is lost (the reference-date subtraction happens first), 1 ns after 2001 is kept.
        assert_eq!(foundation_time(0, 1), 0.0);
        assert_eq!(foundation_time(1, 0), 1.0);
        assert_eq!(foundation_time(1_700_000_000, 500_000_000), 1_700_000_000.5);
    }

    #[test]
    fn names_sort_like_swift() {
        // Swift's own order for these (tools/swift-library-oracle `sort`).
        let want = ["_under.mov", "-dash.mov", "(paren).mov", "0.mov", "1.mov", "10.mov", "9.mov", "a b.mov", "a-b.mov", "alpha.mov", "Beta.MP4"];
        let mut got: Vec<&str> = want.iter().rev().copied().collect();
        got.sort_by(|a, b| localized_case_insensitive_cmp(a, b));
        assert_eq!(got, want);
        assert_eq!(localized_case_insensitive_cmp("ete", "été"), Ordering::Less);
        assert_eq!(localized_case_insensitive_cmp("X", "x"), Ordering::Equal);
        assert_eq!(localized_case_insensitive_cmp("ﬁle.mov", "file.mov"), Ordering::Equal);
        assert_eq!(localized_case_insensitive_cmp("clip10.mov", "clip2.mov"), Ordering::Less);
    }

    #[test]
    fn packages_by_extension() {
        assert!(is_package_name("Thing.app"));
        assert!(is_package_name("Lib.PhotosLibrary"));
        assert!(!is_package_name("Clips.framework"));
        assert!(!is_package_name("Folder.mov"));
        assert!(!is_package_name("app"));
    }
}
