//! `clip_id`, `header`, `enumerate`, `roots` and `thumbkey` against Swift MaCrate's own code (MaCrate 0.16.4,
//! GlanceCore f0b917a), run on the same inputs. `tests/data/files/swift-*` is what Swift printed —
//! tools/swift-library-oracle/FilesOracle.swift, built and run by files-oracle.sh; the method and the numbers are in
//! docs/data/m1/files-crosstest.txt. The folder and movie inputs are generated here (files_support/), identical
//! to the ones the oracle read; Amrit's test media is read only when `GLANCE_TESTFILES` names its folder, and is
//! matched by content (clip ids), never by name.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use glance_library::clip_id::{clip_id, size_from_clip_id};
use glance_library::enumerate::{direct_videos, foundation_mtime, foundation_time, localized_case_insensitive_cmp, recursive_videos};
use glance_library::header::{HeaderInfo, container_info, extract_info};
use glance_library::roots;
use glance_library::thumbkey::{downscaled_size, swift_double};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

mod files_support;
use files_support::{headers, tree};

fn data(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/files").join(name)).unwrap()
}

fn rows(name: &str) -> Vec<Vec<String>> {
    data(name).lines().filter(|l| !l.is_empty()).map(|l| l.split('\t').map(str::to_string).collect()).collect()
}

fn bits(v: Option<f64>) -> String {
    v.map_or_else(|| "-".to_string(), |d| format!("{:016x}", d.to_bits()))
}

/// The oracle's 8 header columns: container reader (duration, fps, width, height), then the routed read.
fn info_cols(path: &Path) -> Vec<String> {
    let cols = |i: HeaderInfo| {
        let (w, h) = i.size.map_or(("-".to_string(), "-".to_string()), |(w, h)| (w.to_string(), h.to_string()));
        vec![bits(i.duration), bits(i.fps), w, h]
    };
    [cols(container_info(path)), cols(extract_info(path))].concat()
}

/// A fresh scratch folder (removed on drop).
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("glance-library-{tag}-{}", std::process::id()));
        tree::unlock(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        tree::unlock(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn doubles_are_written_as_swift_writes_them() {
    let table = rows("swift-doubles.tsv");
    let mut wrong = Vec::new();
    for r in &table {
        let x = f64::from_bits(u64::from_str_radix(&r[0], 16).unwrap());
        if swift_double(x) != r[1] {
            wrong.push(format!("{}: Rust {} Swift {}", r[0], swift_double(x), r[1]));
        }
    }
    assert!(wrong.is_empty(), "{} of {}: {wrong:?}", wrong.len(), table.len());
    assert!(table.len() > 3800);
}

#[test]
fn modification_dates_are_foundations() {
    let table = rows("swift-mtimes.tsv");
    for r in &table {
        let got = foundation_time(r[0].parse().unwrap(), r[1].parse().unwrap());
        assert_eq!(format!("{:016x}", got.to_bits()), r[2], "{} s {} ns", r[0], r[1]);
    }
    assert_eq!(table.len(), 315);
}

#[test]
fn names_sort_as_swift_sorts_them() {
    // Core Foundation's prefix shortcut puts `ss.mov` before `ß-straße.mxf` (docs/data/m1/files-crosstest.txt);
    // every other name lands where Swift puts it.
    let quirk = ["ss.mov", "ß-straße.mxf"];
    let mut names: Vec<String> = data("names-in.txt").lines().map(str::to_string).collect();
    names.sort_by(|a, b| localized_case_insensitive_cmp(a, b));
    let ours: Vec<&String> = names.iter().filter(|n| !quirk.contains(&n.as_str())).collect();
    let swift = data("swift-names-sorted.txt");
    let theirs: Vec<&str> = swift.lines().filter(|n| !quirk.contains(n)).collect();
    assert_eq!(ours, theirs);
    assert_eq!(theirs.len(), 143);
}

#[test]
fn clip_ids_give_their_size_back_as_swift_does() {
    for r in rows("swift-clipsize.tsv") {
        let ours = size_from_clip_id(&r[0]).map_or_else(|| "-".to_string(), |s| s.to_string());
        assert_eq!(ours, r[1], "{:?}", r[0]);
    }
}

#[test]
fn thumbnails_downscale_as_swift_does() {
    for r in rows("swift-downscale.tsv") {
        let v: Vec<u32> = r[0].split(' ').map(|x| x.parse().unwrap()).collect();
        let (w, h) = downscaled_size(v[0], v[1]);
        assert_eq!(format!("{w} {h}"), r[1], "{}", r[0]);
    }
}

#[test]
fn library_roots_json_reads_and_writes_as_swift_does() {
    let text = data("swift-library-roots.jsonl");
    let field = |s: &str, key: &str| -> String {
        let at = s.find(&format!("\"{key}\":")).unwrap() + key.len() + 3;
        let rest = &s[at..];
        if rest.starts_with('"') {
            let mut end = 1;
            let b = rest.as_bytes();
            while b[end] != b'"' {
                end += if b[end] == b'\\' { 2 } else { 1 };
            }
            rest[..=end].to_string()
        } else {
            rest[..rest.find([',', '}']).unwrap()].to_string()
        }
    };
    let mut n = 0;
    for line in text.lines() {
        let read = roots::decode_library_roots(line.as_bytes()).unwrap();
        let written = roots::encode_library_roots(&read).unwrap();
        for key in ["id", "displayName", "bookmarkData", "dateAdded"] {
            assert_eq!(field(&written, key), field(line, key), "{key} in {line}");
        }
        n += 1;
    }
    assert_eq!(n, 120);
}

#[cfg(target_os = "macos")]
#[test]
fn path_forms_are_foundations() {
    for r in rows("swift-paths.tsv") {
        let std = roots::standardized_path(&r[0]);
        assert_eq!(std, r[1], "standardized {}", r[0]);
        assert_eq!(roots::canonical_path(&r[0]), r[2], "canonical {}", r[0]);
        assert_eq!(roots::is_dangerous_root(&std).to_string(), r[3], "denylist {}", r[0]);
        assert_eq!(roots::last_path_component(&r[0]), r[4], "last component {}", r[0]);
    }
}

/// The walk of the synthetic folder, with and without the exclusions, line for line as Swift lists it. One known
/// difference: `BundleBit/` has Finder's bundle bit set, which makes it a package to Foundation; it is not read
/// here (docs/data/m1/files-crosstest.txt).
#[cfg(target_os = "macos")]
#[test]
fn folders_enumerate_as_swift_enumerates_them() {
    let scratch = Scratch::new("tree");
    tree::build(&scratch.0).unwrap();
    let real = std::fs::canonicalize(&scratch.0).unwrap();
    let prefix = format!("{}/", real.display());
    let mut ours = Vec::new();
    for (run, excluded) in [("all", vec![]), ("excluded", tree::EXCLUDED.to_vec())] {
        match direct_videos(&scratch.0) {
            Ok(clips) => {
                for c in clips {
                    ours.push(format!("{run}\tdirect\t{}\t{}\t{}", c.path.replacen(&prefix, "", 1), c.size, bits(Some(c.mtime))));
                }
            }
            Err(_) => ours.push(format!("{run}\tdirect-error")),
        }
        let set: HashSet<String> = excluded.iter().map(|e| roots::standardized_path(&scratch.0.join(e).to_string_lossy())).collect();
        let mut reports = Vec::new();
        let mut progress = |n: usize| reports.push(n);
        for c in recursive_videos(&scratch.0, &set, Some(&mut progress), &|| false) {
            ours.push(format!("{run}\trecursive\t{}\t{}\t{}", c.path.replacen(&prefix, "", 1), c.size, bits(Some(c.mtime))));
        }
        assert!(reports.last().is_some_and(|&n| n > 0), "the final progress report");
    }
    let swift: Vec<String> = data("swift-tree-enum.tsv").lines().map(str::to_string).collect();
    let known = |l: &String| l.contains("\tBundleBit/x.mov\t");
    let missing: Vec<&String> = swift.iter().filter(|l| !ours.contains(l)).collect();
    let extra: Vec<&String> = ours.iter().filter(|l| !swift.contains(l) && !known(l)).collect();
    assert!(missing.is_empty() && extra.is_empty(), "missing {missing:?}\nextra {extra:?}");
    let ours_known: Vec<String> = ours.iter().filter(|l| !known(l)).cloned().collect();
    assert_eq!(ours_known, swift, "order");
    assert_eq!(swift.len(), 82 + 78);
    // `café.mov` stored composed and decomposed: the walk's path keeps each on-disk form byte for byte, as
    // Swift's `clip.url.path` does (no file-system decomposition), and Swift's table says the same.
    for (rel, want) in [("nfc/caf\u{e9}.mov", "caf\u{e9}.mov"), ("nfd/cafe\u{301}.mov", "cafe\u{301}.mov")] {
        let line = format!("all\trecursive\t{rel}\t");
        assert!(swift.iter().any(|l| l.starts_with(&line)), "{rel} in Swift's walk");
        assert!(ours.iter().any(|l| l.starts_with(&line) && l.contains(want)), "{rel} in ours");
    }
    // A link to a folder, a missing folder and a file are not listed.
    assert!(direct_videos(&scratch.0.join("linkdir")).is_err());
    assert!(recursive_videos(&scratch.0.join("linkdir"), &HashSet::new(), None, &|| false).is_empty());
    assert!(direct_videos(&scratch.0.join("missing")).is_err());
    assert!(direct_videos(&scratch.0.join("alpha.mov")).is_err());
    // A cancelled walk stops.
    assert!(recursive_videos(&scratch.0, &HashSet::new(), None, &|| true).is_empty());
}

/// Every file of the synthetic folder: clip id, both header reads, Foundation's date and size.
#[cfg(target_os = "macos")]
#[test]
fn files_read_as_swift_reads_them() {
    let scratch = Scratch::new("files");
    tree::build(&scratch.0).unwrap();
    tree::unlock(&scratch.0);
    let table = rows("swift-tree-files.tsv");
    for r in &table {
        let path = scratch.0.join(&r[0]);
        let meta = std::fs::symlink_metadata(&path).unwrap();
        let mut ours = vec![r[0].clone(), r[1].clone(), clip_id(&path).unwrap_or_else(|_| "-".into())];
        ours.extend(info_cols(&path));
        // A link's own date is when this test made it.
        ours.push(if r[1] == "link" { r[11].clone() } else { bits(foundation_mtime(&meta)) });
        ours.push(meta.len().to_string());
        assert_eq!(&ours, r, "{}", r[0]);
    }
    assert_eq!(table.len(), 72);
}

/// The generated movies: every box layout, both reads. One known difference: a HAP track whose sample entry says
/// 0 × 0 is refused by DXVKit Plus's demux, where GlanceCore's HAP demuxer still gives its duration and rate.
#[test]
fn headers_read_as_swift_reads_them() {
    let scratch = Scratch::new("headers");
    headers::write(&scratch.0).unwrap();
    let table = rows("swift-headers.tsv");
    let mut wrong = Vec::new();
    for r in &table {
        let path = scratch.0.join(&r[0]);
        let mut ours = vec![r[0].clone(), r[1].clone(), clip_id(&path).unwrap()];
        ours.extend(info_cols(&path));
        ours.push(std::fs::metadata(&path).unwrap().len().to_string());
        if &ours != r {
            wrong.push(r[0].clone());
        }
    }
    assert_eq!(wrong, ["hap-width0.mov"]);
    assert_eq!(table.len(), 51);
}

/// The committed fixtures (tests/fixtures: DXV, HAP, H.264, HEVC, ProRes, Motion JPEG, DXVKit's streams).
#[test]
fn fixtures_read_as_swift_reads_them() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let table = rows("swift-fixtures.tsv");
    for r in &table {
        let path = root.join(&r[0]);
        let mut ours = vec![r[0].clone(), r[1].clone(), clip_id(&path).unwrap()];
        ours.extend(info_cols(&path));
        ours.push(std::fs::metadata(&path).unwrap().len().to_string());
        assert_eq!(&ours, r, "{}", r[0]);
    }
    assert_eq!(table.len(), 98);
}

/// Amrit's test media (when `GLANCE_TESTFILES` names it): every file Swift read, found again by its clip id, gives
/// the same header reads.
#[test]
fn test_media_reads_as_swift_reads_it() {
    let Some(root) = std::env::var_os("GLANCE_TESTFILES").map(PathBuf::from).filter(|p| p.is_dir()) else {
        eprintln!("GLANCE_TESTFILES not set: skipped");
        return;
    };
    let table: HashMap<String, Vec<String>> = rows("swift-testfiles.tsv").into_iter().map(|r| (r[0].clone(), r)).collect();
    let mut found = HashSet::new();
    let mut dirs = vec![root];
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let meta = std::fs::symlink_metadata(e.path()).unwrap();
            if meta.is_dir() {
                dirs.push(e.path());
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let id = clip_id(&e.path()).unwrap();
            if let Some(r) = table.get(&id) {
                let mut ours = vec![id.clone()];
                ours.extend(info_cols(&e.path()));
                assert_eq!(&ours, r, "clip {}", &id[..12]);
                found.insert(id);
            }
        }
    }
    assert_eq!(found.len(), table.len(), "every file Swift read is there");
    assert_eq!(table.len(), 183);
}
