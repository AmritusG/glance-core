//! The folder listing on this machine's OS (macOS): Finder's order, hidden files, the type test.
//! The order is the current locale's (this machine: en); the conforming set is the system's.
#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_platform::{conforms_to_movie, localized_standard_compare, movie_files};
use std::path::PathBuf;

/// Names a plain natural sort gets wrong, in the order macOS 27.0.1 (en) gives them: case after
/// the letter (b, B), "2" before "02", a space before `_` `-` `.`, accents after the plain letter,
/// Latin before Greek before Cyrillic, and digit runs past 2^64 compared modulo 2^64 (2^64 sorts
/// before 3; 2^64 − 1 is the largest).
const FINDER_ORDER: [&str; 22] = [
    "a.mov",
    "b.mov",
    "B.mov",
    "clip 1.mov",
    "clip 1a.mov",
    "clip 2.mov",
    "clip 02.mov",
    "Clip 3.mov",
    "clip 10.mov",
    "clip_1.mov",
    "clip-1.mov",
    "clip.1.mov",
    "eclair.mov",
    "Eclair.mov",
    "éclair.mov",
    "fig.mov",
    "n 18446744073709551616.mov",
    "n 3.mov",
    "n 99999999999999999999999.mov",
    "n 18446744073709551615.mov",
    "Ω omega.mov",
    "ж zhe.mov",
];

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let d = std::env::temp_dir().join(format!("glance-platform-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        TempDir(d)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn names_sort_in_finders_order() {
    let mut names = FINDER_ORDER.to_vec();
    names.reverse();
    names.sort_by(|a, b| localized_standard_compare(a, b));
    assert_eq!(names, FINDER_ORDER);
}

#[test]
fn a_folder_lists_its_movies_in_finders_order_without_hidden_files() {
    let dir = TempDir::new("list");
    // The volume is case-insensitive (APFS default): names that differ only in case are one file.
    let names: Vec<&str> = FINDER_ORDER.iter().copied().filter(|n| !matches!(*n, "b.mov" | "eclair.mov")).collect();
    // Written in reverse so the directory's own order is not the answer.
    for n in names.iter().rev().chain(["notes.txt", "still.png", ".dotfile.mov", "flagged.mov"].iter()) {
        std::fs::write(dir.0.join(n), b"").unwrap();
    }
    let flagged = std::process::Command::new("chflags").arg("hidden").arg(dir.0.join("flagged.mov")).status().unwrap();
    assert!(flagged.success());
    let got: Vec<String> = movie_files(&dir.0).unwrap().iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(got, names, "dot file, UF_HIDDEN file, .txt and .png left out");
}

#[test]
fn a_missing_folder_is_an_error_not_a_crash() {
    assert!(movie_files(std::path::Path::new("/nonexistent/glance-platform")).is_err());
}

/// The OS's answer for every extension of the open panel's list and a few that are not movies,
/// as macOS 27.0.1 gives it (recorded in docs/handoffs/v031-g1-fixes.md).
#[test]
fn the_type_test_is_the_systems() {
    for ext in ["mov", "MOV", "qt", "mp4", "m4v", "avi", "mkv", "webm", "mpg", "mpeg", "m2v", "mts", "m2ts", "ts", "dv", "3gp", "flv", "wmv"] {
        assert!(conforms_to_movie(ext), "{ext} should be a movie on macOS 27");
    }
    for ext in ["mk3d", "txt", "png", "wav", "mp3", "gif", ""] {
        assert!(!conforms_to_movie(ext), "{ext} should not be a movie on macOS 27");
    }
}
