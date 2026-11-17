//! Routing, the "not supported" line, `clip_key` and the Alpha Type hint against Swift Glance's own
//! code run on the same files. `tests/data/swift-glance-*.tsv` is the output of a throwaway Swift
//! program built on GlanceCore (glance @ a9dc601, DXVKit 1.0.0) — `docs/handoffs/c1-exact-picture.md`
//! says how it was made. One line per file, tab separated:
//! path, route (dxv / hap / av / unsupported), the line Swift Glance shows if nothing plays the
//! file, the first video track's FourCC in brackets, its display name, the clip key, the probe's
//! kind, its hint, samples, exceeding. `-` where Swift returns nothing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use glance_media::{AlphaReport, AlphaRule, Route, alpha_report, clip_key, first_video_fourcc, names, route, unsupported_message};
use sha2::{Digest, Sha256};
use std::path::Path;

#[path = "../../../tests/support/movie.rs"]
mod support;
use support::{Kind, Scratch, fixtures, testfile, testfiles};

/// What was compared, to pin that nothing was skipped.
#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    files: usize,
    keys: usize,
    hints: usize,
    unsupported: usize,
}

/// Every line of `table` against the Rust code on the same file under `root`.
fn compare(table: &str, root: &Path) -> Counts {
    let mut counts = Counts::default();
    let mut wrong = Vec::new();
    for row in table.lines() {
        let col: Vec<&str> = row.split('\t').collect();
        let [name, swift_route, message, fourcc, shown_as, key, kind, look, samples, exceeding] = col[..] else { panic!("malformed line: {row}") };
        let path = testfile(root, name);
        let mut check = |what: &str, ours: String, swift: &str| {
            if ours != swift {
                wrong.push(format!("{name}: {what}: Rust {ours:?}, Swift {swift:?}"));
            }
        };
        let dash = |v: Option<String>| v.unwrap_or_else(|| "-".into());
        // HAP R and HAP HDR: Swift Glance does not play them and shows their FourCC; the Rust family plays them.
        let rust_only = matches!(fourcc, "[Hap7]" | "[HapH]");
        let cc = first_video_fourcc(&path);
        check("FourCC", dash(cc.map(|c| format!("[{}]", String::from_utf8_lossy(&c)))), fourcc);
        if !rust_only {
            check("name", dash(cc.and_then(|c| names::codec_name(&c)).map(str::to_string)), shown_as);
            check("message", unsupported_message(&path), message);
        }
        check("clip key", dash(clip_key(&path)), key);
        counts.keys += usize::from(key != "-");
        let routed = route(&path);
        let ours = match &routed {
            Ok(clip) if clip.route == Route::Dxv => "dxv",
            Ok(clip) if clip.route == Route::Hap => "hap",
            Ok(clip) if clip.route == Route::Native => "av",
            Ok(_) => "other",
            Err(_) => "unsupported",
        };
        // The OS decoder is DXVKit Plus's macOS backend; other systems have none yet.
        let expected = match swift_route {
            "unsupported" if rust_only => "hap",
            "av" if cfg!(not(target_os = "macos")) => "unsupported",
            other => other,
        };
        check("route", ours.into(), expected);
        match &routed {
            Err(e) if swift_route == "unsupported" => {
                check("shown message", e.message.clone(), message);
                counts.unsupported += 1;
            }
            Ok(clip) if clip.route == Route::Dxv => {
                let report = alpha_report(clip);
                let ours = match report {
                    AlphaReport { rule: AlphaRule::Choosable, look: Some(look), samples, exceeding } => {
                        format!("choosable/{}/{samples}/{exceeding}", look.label())
                    }
                    // Swift's probe reports no alpha both for an opaque clip ("none") and for one whose frames do not decode ("-").
                    AlphaReport { rule: AlphaRule::Opaque, look: None, .. } => "none".into(),
                    other => format!("{other:?}"),
                };
                let swift = if kind == "choosable" { format!("choosable/{look}/{samples}/{exceeding}") } else { "none".into() };
                check("alpha hint", ours, &swift);
                counts.hints += usize::from(kind == "choosable");
            }
            _ => {}
        }
        counts.files += 1;
    }
    assert!(wrong.is_empty(), "{} differences from Swift Glance:\n{}", wrong.len(), wrong.join("\n"));
    counts
}

#[test]
fn the_committed_fixtures_route_key_and_probe_as_in_swift_glance() {
    let counts = compare(include_str!("../../../tests/data/swift-glance-fixtures.tsv"), &fixtures());
    // 52 movies: 35 DXV keys (3 invalid vectors have none), 12 with alpha measured, 3 refused.
    assert_eq!(counts, Counts { files: 52, keys: 35, hints: 12, unsupported: 3 });
}

/// The same on Amrit's own test media (174 files; skipped unless `GLANCE_TESTFILES` names the folder).
#[test]
fn amrits_test_files_route_key_and_probe_as_in_swift_glance() {
    let Some(root) = testfiles() else {
        eprintln!("SKIP: GLANCE_TESTFILES does not name Amrit's test media folder");
        return;
    };
    let counts = compare(include_str!("../../../tests/data/swift-glance-testfiles.tsv"), &root);
    eprintln!("{counts:?}");
    // 75 DXV keys, 51 DXV clips with alpha measured; 6 files nothing plays. (Up to DXVKit Plus
    // 0.8.0 a seventh, `adv_trunc_hapm_midframe.mov`, was refused here and played by Swift; since
    // 0.9.0 its whole samples play, as in Swift.)
    assert_eq!(counts, Counts { files: 174, keys: 75, hints: 51, unsupported: 6 });
}

/// The key's layout, recomputed here from the bytes: the text `"<size> <frames> <w>x<h>"`, then the
/// first packet, then the last. Controls: a renamed copy keeps its key, one changed byte in the
/// last packet changes it, and a one-frame clip hashes its packet twice.
#[test]
fn the_clip_key_is_the_documented_bytes() {
    let scratch = Scratch::new("clip-key");
    let hex = |h: Sha256| -> String { h.finalize().iter().map(|b| format!("{b:02x}")).collect() };
    let packets: Vec<Vec<u8>> = (0..3).map(|f| Kind::Dxt5.packet(&Kind::Dxt5.frame_planes(17, 33, f))).collect();
    let bytes = support::video_movie(b"DXD3", 17, 33, &packets);
    let path = scratch.file("three.mov", &bytes);
    let mut h = Sha256::new();
    h.update(format!("{} 3 17x33", bytes.len()));
    h.update(&packets[0]);
    h.update(&packets[2]);
    let key = clip_key(&path).unwrap();
    assert_eq!(key, hex(h));
    assert_eq!(clip_key(&scratch.file("renamed elsewhere.mov", &bytes)).unwrap(), key);
    // The last byte of the mdat is the last byte of the last packet.
    let at = bytes.windows(4).position(|w| w == b"moov").unwrap() - 5;
    let mut changed = bytes.clone();
    changed[at] ^= 1;
    assert_ne!(clip_key(&scratch.file("changed.mov", &changed)).unwrap(), key);
    // The middle packet is not part of the key.
    let mut other_middle = packets.clone();
    other_middle[1] = packets[0].clone();
    assert_eq!(clip_key(&scratch.file("middle.mov", &support::video_movie(b"DXD3", 17, 33, &other_middle))).unwrap(), key);

    let one = support::video_movie(b"DXD3", 17, 33, &packets[..1]);
    let mut h = Sha256::new();
    h.update(format!("{} 1 17x33", one.len()));
    h.update(&packets[0]);
    h.update(&packets[0]);
    assert_eq!(clip_key(&scratch.file("one.mov", &one)).unwrap(), hex(h));
    // Not DXV: no key.
    assert_eq!(clip_key(&scratch.file("hap.mov", &Kind::Hap5.movie(17, 33, 2))), None);
    assert_eq!(clip_key(Path::new("/nonexistent/clip.mov")), None);
}
