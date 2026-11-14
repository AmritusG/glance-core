//! Content routing, the line shown when nothing plays a file, and hostile files: every failure is
//! a value with a message, never a crash or a hang.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use glance_media::{AlphaRule, AlphaView, Clip, NotPlayable, Route, alpha_rule, dxv_variant_label, route, unsupported_message};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "../../../tests/support/movie.rs"]
mod support;
use support::{Kind, Scratch, Track, fixtures, movie, movie_laid_out, rng, video_movie};

fn message(path: &Path) -> String {
    match route(path) {
        Err(NotPlayable { message, .. }) => message,
        Ok(clip) => panic!("{} routed to {:?}", path.display(), clip.route),
    }
}

#[test]
fn every_dxv_and_hap_variant_routes_by_what_the_file_holds() {
    let scratch = Scratch::new("routing");
    for kind in Kind::ALL {
        // The name says nothing: every movie is called `.dat`.
        let clip = route(&scratch.file(&format!("{kind:?}.dat"), &kind.movie(20, 12, 2))).unwrap();
        assert_eq!(clip.route, if kind.is_dxv() { Route::Dxv } else { Route::Hap }, "{kind:?}");
        let rule = alpha_rule(&clip.info);
        let want = match kind {
            Kind::Dxt5 | Kind::Yg10 | Kind::LegacyDxt5 => AlphaRule::Choosable,
            Kind::Hap5 | Kind::HapM | Kind::HapA | Kind::Hap7 => AlphaRule::Stated(AlphaView::Straight),
            _ => AlphaRule::Opaque,
        };
        assert_eq!(rule, want, "{kind:?}");
        let mut source = clip.open().unwrap();
        assert_eq!(source.decode(1).unwrap().size(), (20, 12), "{kind:?}");
    }
    // DXD4 is a DXV3 sample entry too, and the packet decides the generation: a legacy packet in a
    // DXD3 track decodes as legacy, a DXV3 packet in a DXDI track as DXV3.
    let dxt1 = Kind::Dxt1.packet(&Kind::Dxt1.frame_planes(20, 12, 0));
    let legacy5 = Kind::LegacyDxt5.packet(&Kind::LegacyDxt5.frame_planes(20, 12, 0));
    for (fourcc, packet, label) in [(b"DXD4", &dxt1, "Normal Quality"), (b"DXD3", &legacy5, "Normal Quality With Alpha"), (b"DXDI", &dxt1, "Normal Quality")] {
        let clip = route(&scratch.file("packet-decides.mov", &video_movie(fourcc, 20, 12, std::slice::from_ref(packet)))).unwrap();
        assert_eq!(clip.route, Route::Dxv);
        assert_eq!(dxv_variant_label(&clip.open().unwrap().decode(0).unwrap()), Some(label));
    }
}

#[test]
fn the_committed_fixtures_route_as_their_content_says() {
    let table = [
        ("tiles_dxt1.mov", Route::Dxv),
        ("tiles_yg10.mov", Route::Dxv),
        ("dxvkit/streams/synthetic/synthetic-dxdi_dxt1-lzf_pat64.mov", Route::Dxv),
        ("tiles_hap1.mov", Route::Hap),
        ("tiles_hapm_chunks4.mov", Route::Hap),
        ("tiles_hapa.mov", Route::Hap),
        ("synthetic_hap7.mov", Route::Hap),
        ("synthetic_haph.mov", Route::Hap),
    ];
    for (name, want) in table {
        assert_eq!(route(&fixtures().join(name)).unwrap().route, want, "{name}");
    }
    #[cfg(target_os = "macos")]
    for name in ["tiles_h264.mp4", "tiles_hevc.mp4", "tiles_mjpeg.mov", "tiles_prores422.mov", "tiles_prores4444.mov"] {
        assert_eq!(route(&fixtures().join(name)).unwrap().route, Route::Native, "{name}");
    }
}

#[test]
fn what_nothing_plays_gets_swift_glances_line() {
    let scratch = Scratch::new("messages");
    let frame = Kind::Dxt1.packet(&Kind::Dxt1.frame_planes(16, 16, 0));
    // A texture tag, or the format's name, as the sample entry is not DXV (Arena's rule): the
    // FourCC is shown as it is. QuickTime Animation has a name.
    for (fourcc, line) in [
        (b"DXT1", "DXT1 in MOV is not supported"),
        (b"YCG6", "YCG6 in MOV is not supported"),
        (b"DXV3", "DXV3 in MOV is not supported"),
        (b"rle ", "QuickTime Animation in MOV is not supported"),
        (b"zz  ", "zz in MOV is not supported"),
    ] {
        assert_eq!(message(&scratch.file("clip.mov", &video_movie(fourcc, 16, 16, std::slice::from_ref(&frame)))), line);
    }
    // A DXV track whose first packet does not state a variant is refused: version byte below 4,
    // stored flag above 1.
    for (at, value) in [(4, 3u8), (6, 2u8)] {
        let mut bad = frame.clone();
        bad[at] = value;
        assert_eq!(message(&scratch.file("clip.mov", &video_movie(b"DXD3", 16, 16, &[bad]))), "DXV3 in MOV is not supported");
    }
    assert_eq!(message(&scratch.file("legacy.mov", &video_movie(b"DXDI", 16, 16, &[vec![0xFF; 3]]))), "DXV1 in MOV is not supported");
    // The container is the extension in upper case; without one, Swift Glance's words.
    let hapy = Kind::HapY.movie(16, 16, 2);
    // Cut off inside the sample data, the sample tables (written first) whole: the codec can still be named.
    let whole = movie_laid_out(&[Track { fourcc: b"HapY", width: 16, height: 16, samples: &[vec![0; 400], vec![0; 400]] }], true);
    let cut = &whole[..whole.len() - 300];
    assert_eq!(message(&scratch.file("cut.MoV", cut)), "HAP Q in MOV is not supported");
    assert_eq!(message(&scratch.file("cut", cut)), "HAP Q in this container is not supported");
    assert_eq!(message(&scratch.file("no-moov.mp4", &hapy[..hapy.len() / 2])), "MP4 files are not supported");
    assert_eq!(message(&scratch.file("zero.mov", &[])), "MOV files are not supported");
    assert_eq!(message(&scratch.file("zero", &[])), "this container files are not supported");
    assert_eq!(message(Path::new("/nonexistent/clip.avi")), "AVI files are not supported");
    assert_eq!(message(&scratch.0), "this container files are not supported"); // a folder
    // Matroska: the codec from the first `V_...` id.
    for (name, id, line) in [
        ("a.webm", "V_VP9", "VP9 in WEBM is not supported"),
        ("a.mkv", "V_MPEGH/ISO/HEVC", "HEVC in MKV is not supported"),
        ("a.mkv", "V_FOO", "FOO in MKV is not supported"),
        ("a.mkv", "V_", "MKV files are not supported"),
        ("a.mov", "V_VP9", "MOV files are not supported"),
    ] {
        assert_eq!(message(&scratch.file(name, &[&[0x1A, 0x45, 0xDF, 0xA3][..], &[0x80; 40], id.as_bytes(), &[0x86, 0x00]].concat())), line);
    }
    // Stills are not clips (Glance G1 decides them).
    assert_eq!(message(&scratch.file("still.png", b"\x89PNG\r\n\x1a\n")), "PNG files are not supported");
    // The line is the same whether or not routing was tried.
    assert_eq!(unsupported_message(&scratch.file("zero.mov", &[])), "MOV files are not supported");
}

/// Known difference, pinned so a change is noticed: a movie with a DXV track and a HAP track. Swift
/// Glance plays the DXV track (DXV is tried first); DXVKit Plus's probe takes the HAP track.
/// Reported to DXVKit Plus (`DXVKit-Plus_2026-11-14_c1-findings_brief.md`).
#[test]
fn a_movie_with_a_dxv_and_a_hap_track_plays_the_hap_track() {
    let scratch = Scratch::new("two-tracks");
    let (dxv, hap) = (vec![Kind::Dxt1.packet(&Kind::Dxt1.frame_planes(16, 16, 0))], vec![Kind::Hap1.packet(&Kind::Hap1.frame_planes(16, 16, 0))]);
    let both = movie(&[Track { fourcc: b"DXD3", width: 16, height: 16, samples: &dxv }, Track { fourcc: b"Hap1", width: 16, height: 16, samples: &hap }]);
    assert_eq!(route(&scratch.file("both.mov", &both)).unwrap().route, Route::Hap);
}

/// Route a file and, when it routes, open it and decode its first, middle and last frame to RGBA
/// on the CPU. Whatever comes back is fine; coming back is the point.
fn exercise(path: &Path) -> Result<Clip, NotPlayable> {
    let clip = route(path)?;
    if let Ok(mut source) = clip.open() {
        for frame in [0, clip.info.frames / 2, clip.info.frames.saturating_sub(1)] {
            if let Ok(decoded) = source.decode(frame) {
                let _ = decoded.to_rgba();
            }
        }
    }
    Ok(clip)
}

/// Run `work` on its own thread; a hang fails the test instead of stalling it.
fn within<T: Send + 'static>(limit: Duration, what: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || tx.send(work()));
    match rx.recv_timeout(limit) {
        Ok(v) => v,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: no answer within {limit:?} (a hang)"),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: panicked: {:?}", worker.join().err()),
    }
}

/// Damaged copies of real movies: cut short at many lengths, bytes overwritten in the sample tables
/// and in the frames, the sample entry renamed to a codec the payload is not, plain garbage and
/// zero bytes. Each one must come back as a clip or as a line to show, within the time limit.
#[test]
fn hostile_files_give_a_message_never_a_crash_or_a_hang() {
    let bases: Vec<PathBuf> = [
        "tiles_hap1.mov",
        "tiles_hapm_chunks4.mov",
        "tiles_dxt5.mov",
        "tiles_yg10.mov",
        "synthetic_hap7.mov",
        "dxvkit/streams/synthetic/synthetic-dxdi_dxt5-lzf_pat64.mov",
    ]
    .iter()
    .map(|n| fixtures().join(n))
    .collect();
    let (cases, refused) = within(Duration::from_secs(300), "hostile files", move || {
        let scratch = Scratch::new("hostile");
        let mut state = 0xC1_u64;
        let (mut cases, mut refused) = (0u32, 0u32);
        let mut run = |name: &str, bytes: &[u8]| {
            cases += 1;
            if let Err(e) = exercise(&scratch.file(name, bytes)) {
                assert!(e.message.ends_with("not supported") || e.message.ends_with("could not be opened"), "{name}: {:?}", e.message);
                refused += 1;
            }
        };
        for base in &bases {
            let good = std::fs::read(base).unwrap();
            assert!(exercise(base).is_ok(), "{}", base.display());
            // Truncated: every power of two, the last bytes, and 24 random lengths.
            let mut lengths: Vec<usize> =
                (0..24).map(|b| 1usize << b).chain([good.len() - 1, good.len() - 9, good.len() - 100]).filter(|&n| n < good.len()).collect();
            lengths.extend((0..24).map(|_| (rng(&mut state) % good.len() as u64) as usize));
            for n in lengths {
                run("truncated.mov", &good[..n]);
            }
            // One to four bytes overwritten: in the moov (the tail of these files) and anywhere.
            let moov = good.windows(4).rposition(|w| w == b"moov").unwrap();
            for i in 0..400 {
                let mut bad = good.clone();
                for _ in 0..=(i % 4) {
                    let span = if i % 2 == 0 { moov..good.len() } else { 0..good.len() };
                    let at = span.start + (rng(&mut state) % (span.end - span.start) as u64) as usize;
                    bad[at] = rng(&mut state) as u8;
                }
                run("damaged.mov", &bad);
            }
            // The wrong FourCC for the payload.
            let entry = good.windows(4).rposition(|w| w == b"stsd").unwrap() + 16;
            for fourcc in [b"Hap1", b"Hap5", b"HapY", b"HapM", b"HapA", b"Hap7", b"HapH", b"DXD3", b"DXD4", b"DXDI", b"avc1", b"ap4h"] {
                let mut bad = good.clone();
                bad[entry..entry + 4].copy_from_slice(fourcc);
                run("renamed.mov", &bad);
            }
        }
        // Garbage: random bytes, and random bytes behind a plausible first atom.
        for i in 0..200u64 {
            let mut bytes: Vec<u8> = (0..(rng(&mut state) % 4096)).map(|_| rng(&mut state) as u8).collect();
            if i % 2 == 0 && bytes.len() >= 8 {
                bytes[..8].copy_from_slice(&[0, 0, 0, 20, b'f', b't', b'y', b'p']);
            }
            run(if i % 3 == 0 { "garbage.mov" } else { "garbage.mp4" }, &bytes);
        }
        run("zero.mov", &[]);
        // Atom sizes that point nowhere: 0 (to the end), 1 (64-bit size missing), and a moov inside a moov.
        for sizes in [[0u8, 0, 0, 0], [0, 0, 0, 1], [0xFF; 4], [0, 0, 0, 8]] {
            run("sizes.mov", &[&sizes[..], b"moov", &sizes, b"moov", &sizes, b"trak"].concat());
        }
        (cases, refused)
    });
    eprintln!("hostile files: {cases} cases, {refused} refused with a message, the rest played");
    // 6 movies x (about 51 truncations + 400 overwrites + 12 renames) + 200 garbage + 1 + 4.
    assert!(cases > 2900 && refused > 500, "{cases} {refused}");
}

/// Amrit's own adversarial files and every other clip of the test folder (read-only; skipped on a
/// machine without it): each comes back within the limit.
#[test]
fn amrits_test_files_come_back() {
    let Some(root) = support::testfiles() else {
        eprintln!("SKIP: Amrit's test media is not on this machine");
        return;
    };
    let (files, refused) = within(Duration::from_secs(600), "Amrit's test files", move || {
        let (mut files, mut refused) = (0u32, Vec::new());
        for line in include_str!("../../../tests/data/swift-glance-testfiles.tsv").lines() {
            let name = line.split('\t').next().unwrap();
            if let Err(e) = exercise(&root.join(name)) {
                refused.push(format!("{name}: {}", e.message));
            }
            files += 1;
        }
        (files, refused)
    });
    eprintln!("{files} files, {} refused:\n{}", refused.len(), refused.join("\n"));
    assert_eq!((files, refused.len()), (174, 7));
}
