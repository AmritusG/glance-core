//! The synthetic folder the files cross-test walks (ours, generated; nothing from anyone's library): clip names
//! in mixed case, digits, accents in both Unicode forms, punctuation and other scripts; hidden files (dot and
//! `UF_HIDDEN`), packages, a Finder-bundle folder, symbolic links (to a file, to a folder, dangling), an unreadable
//! folder, a folder named like a clip, an excluded subtree, files of 0 / 1 / 1 MiB - 1 / 1 MiB / 1 MiB + 1 and
//! several MiB, and modification dates with nanoseconds (before 1970, before 2001, after 2038).
//!
//! Built by the Rust tests (crates/glance-library/tests/files_swift_parity.rs) and by
//! `files_twin tree <dir>` for Swift's oracle; the same names, bytes and dates every time.
#![allow(dead_code)]

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// (relative path, size in bytes). A trailing `/` is a folder.
pub const FILES: &[(&str, u64)] = &[
    ("Zeta.m4v", 0),
    ("alpha.mov", 1),
    ("Beta.MP4", (1 << 20) - 1),
    ("clip1.mov", 1 << 20),
    ("clip10.mov", (1 << 20) + 1),
    ("clip2.mov", (3 << 20) + 7),
    ("Clip3.MOV", 100),
    ("éclair.mp4", 10),
    ("eclair2.mp4", 11),
    ("e\u{301}te\u{301}.mkv", 12),
    ("Ölfass.avi", 13),
    ("Œuvre.webm", 14),
    ("ß-straße.mxf", 15),
    ("_under.mov", 16),
    ("-dash.mov", 17),
    ("(paren).mov", 18),
    ("[bracket].mov", 19),
    ("#hash.mov", 20),
    ("~tilde.mov", 21),
    ("!bang.mov", 22),
    ("+plus.mov", 23),
    (" lead.mov", 24),
    ("trail.mov ", 25),
    ("x.MoV", 26),
    ("a..mov", 27),
    (" .mov", 28),
    ("x.mov.", 29),
    ("x. mov", 30),
    ("noext", 31),
    ("mov", 32),
    ("x.mov.txt", 33),
    ("x.txt.mov", 34),
    ("notes.txt", 35),
    ("10.mov", 36),
    ("9.mov", 37),
    ("100.mov", 38),
    ("日本.mov", 39),
    ("αβγ.mov", 40),
    ("Жук.mov", 41),
    ("ÅÄÖ.mov", 42),
    ("Aa.mov", 43),
    ("hidden-flag.mov", 44),
    (".hidden.mov", 45),
    ("sub/", 0),
    ("sub/clip1.mov", 2),
    ("sub/inner/", 0),
    ("sub/inner/clip1.mov", 3),
    ("sub/inner/Deep.mov", 4),
    ("Sub2/", 0),
    ("Sub2/Clip1.mov", 5),
    ("Sub2/CLIP1.MP4", 6),
    ("Sub2/a.mov", 7),
    (".hiddendir/", 0),
    (".hiddendir/x.mov", 8),
    ("flag-hidden-dir/", 0),
    ("flag-hidden-dir/x.mov", 9),
    ("Thing.app/", 0),
    ("Thing.app/Contents/", 0),
    ("Thing.app/Contents/x.mov", 10),
    ("Lib.photoslibrary/", 0),
    ("Lib.photoslibrary/x.mov", 11),
    ("Plain.bundle/", 0),
    ("Plain.bundle/x.mov", 12),
    ("NotPkg.framework/", 0),
    ("NotPkg.framework/x.mov", 13),
    ("Folder.mov/", 0),
    ("Folder.mov/inside.mov", 14),
    ("WithContents/", 0),
    ("WithContents/Contents/", 0),
    ("WithContents/Contents/x.mov", 15),
    ("BundleBit/", 0),
    ("BundleBit/x.mov", 16),
    ("excluded/", 0),
    ("excluded/x.mov", 17),
    ("excluded/deeper/", 0),
    ("excluded/deeper/y.mov", 18),
    ("excludedSibling/", 0),
    ("excludedSibling/z.mov", 19),
    ("noperm/", 0),
    ("noperm/x.mov", 20),
    ("Empty/", 0),
    ("deep/", 0),
    ("deep/a/", 0),
    ("deep/a/b/", 0),
    ("deep/a/b/c/", 0),
    ("deep/a/b/c/d/", 0),
    ("deep/a/b/c/d/e.mov", 4 << 20),
    ("Café Films/", 0),
    ("Café Films/a.mov", 21),
    ("Cafe\u{301} Films 2/", 0),
    ("Cafe\u{301} Films 2/b.mov", 22),
    // The same name written composed and decomposed (a folder each: the file system treats them as one name).
    ("nfc/", 0),
    ("nfc/caf\u{e9}.mov", 23),
    ("nfd/", 0),
    ("nfd/cafe\u{301}.mov", 24),
    ("nfc/\u{c5}ngstr\u{f6}m \u{1ea0}\u{1ec7}.mp4", 25),
];

/// (link, target).
pub const LINKS: &[(&str, &str)] = &[("link.mov", "alpha.mov"), ("dangling.mov", "missing.mov"), ("linkdir", "sub"), ("sub/up.mov", "../clip1.mov")];

/// The excluded folders of the second walk (relative to the root).
pub const EXCLUDED: &[&str] = &["excluded", "sub/inner"];

/// Modification time of entry `i`: dates with nanoseconds, a few before 1970, before 2001 and after 2038.
pub fn mtime(i: usize) -> (i64, u32) {
    let i64i = i as i64;
    let sec = match i % 9 {
        0 => -1_000_000_000 + i64i * 7_777,
        1 => 900_000_000 + i64i * 13,
        2 => 3_000_000_000 + i64i * 101,
        _ => 1_600_000_000 + i64i * 86_413 + (i64i * i64i) % 7_919,
    };
    let nsec = ((i as u64 * 123_456_789 + 987) % 1_000_000_000) as u32;
    (sec, if i.is_multiple_of(11) { 0 } else { nsec })
}

/// The bytes of entry `i` (splitmix64 stream seeded by the index).
pub fn content(i: usize, size: u64) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64 ^ (i as u64);
    let mut out = Vec::with_capacity(size as usize);
    while (out.len() as u64) < size {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        for b in z.to_le_bytes() {
            if (out.len() as u64) < size {
                out.push(b);
            }
        }
    }
    out
}

/// Build the tree under `root` (which must not exist yet). Uses `chflags` and `xattr` for the hidden flag and the
/// Finder bundle bit (macOS tools; skipped where they are missing).
pub fn build(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    for (i, (rel, size)) in FILES.iter().enumerate() {
        let path = root.join(rel.trim_end_matches('/'));
        if rel.ends_with('/') {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        std::fs::write(&path, content(i, *size))?;
        let (sec, nsec) = mtime(i);
        let when = if sec >= 0 {
            UNIX_EPOCH + Duration::new(sec.unsigned_abs(), nsec)
        } else {
            (UNIX_EPOCH - Duration::from_secs(sec.unsigned_abs())) + Duration::from_nanos(u64::from(nsec))
        };
        let file = std::fs::OpenOptions::new().write(true).open(&path)?;
        file.set_times(std::fs::FileTimes::new().set_modified(when).set_accessed(SystemTime::now()))?;
    }
    #[cfg(unix)]
    for (link, target) in LINKS {
        std::os::unix::fs::symlink(target, root.join(link))?;
    }
    let tool = |args: &[&str]| std::process::Command::new(args[0]).args(&args[1..]).status().map(|s| s.success()).unwrap_or(false);
    let at = |rel: &str| root.join(rel).to_string_lossy().into_owned();
    tool(&["/usr/bin/chflags", "hidden", &at("hidden-flag.mov")]);
    tool(&["/usr/bin/chflags", "hidden", &at("flag-hidden-dir")]);
    // Finder info: the 0x2000 "has bundle" flag in the first word's flags field.
    tool(&["/usr/bin/xattr", "-wx", "com.apple.FinderInfo", "0000000000000000200000000000000000000000000000000000000000000000", &at("BundleBit")]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.join("noperm"), std::fs::Permissions::from_mode(0o000))?;
    }
    Ok(())
}

/// Make the tree removable again (the unreadable folder).
pub fn unlock(root: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(root.join("noperm"), std::fs::Permissions::from_mode(0o755));
    }
}
