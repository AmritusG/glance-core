//! The exactness bar on the core's own pipeline (file -> route -> open -> decode -> `ClipView::show`
//! -> read back): the picture equals the CPU reference, max 0 per channel, at 1:1 — every frame of
//! every committed fixture, and movies this test writes itself for every DXV and HAP variant at
//! 1x1 ... 3840x2160 (odd and non-16 sizes included) from random blocks. Each Alpha Type a clip can
//! be read with is covered: as stored (Premultiplied) against the CPU decode itself, Straight against
//! the CPU decode times its alpha in whole code values.
//! Needs a GPU (`glance_gpu::verify::test_gpu`): skips without one, unless `GLANCE_REQUIRE_GPU=1`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use glance_gpu::verify::{Reference, Row, clip_rows, diff};
use glance_gpu::{BlockPath, ClipView, Gpu, Presenter, expected_picture, read_texture_rgba};
use glance_media::{AlphaView, Clip, Route, route};
use sha2::{Digest, Sha256};
use std::path::Path;

#[path = "../../../tests/support/movie.rs"]
mod support;
use support::{Kind, SIZES, Scratch, block_fixtures, fixtures};

/// Frames per generated movie (each from its own random blocks).
const FRAMES: u32 = 2;

fn gpu() -> Option<(Gpu, Presenter)> {
    let g = glance_gpu::verify::test_gpu().unwrap()?;
    let presenter = Presenter::new(&g.device);
    Some((g, presenter))
}

fn routed(path: &Path) -> Clip {
    route(path).unwrap_or_else(|e| panic!("{}: {e} ({})", path.display(), e.cause))
}

fn line(what: &str, r: &Row) -> String {
    format!("{what} [{}]: frames {}, max diff {}, texels differing {} / {} ({:?})", r.alpha, r.frames, r.worst, r.differing, r.texels, r.reference)
}

fn assert_exact(rows: &[(String, Row)]) {
    let bad: Vec<String> = rows.iter().filter(|(_, r)| !r.exact() || r.reference != Reference::CpuDecode).map(|(w, r)| line(w, r)).collect();
    assert!(bad.is_empty(), "{} of {} rows differ from the CPU reference:\n{}", bad.len(), rows.len(), bad.join("\n"));
}

/// The contract on the committed fixtures: max 0 per channel, every frame, each Alpha Type.
#[test]
fn every_fixture_frame_equals_the_cpu_reference() {
    let Some((g, presenter)) = gpu() else { return };
    let mut rows = Vec::new();
    let files = block_fixtures();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        for row in clip_rows(&g, &presenter, &routed(path), BlockPath::Exact, 1).unwrap() {
            eprintln!("{}", line(&name, &row));
            rows.push((name.clone(), row));
        }
    }
    assert_exact(&rows);
    // 36 files, every frame that decodes, none skipped: 3 opaque HAP tile movies read both ways
    // (24 frames each), 4 HAP tile movies with stated alpha (24 each) and HAP R (3), 28 DXV movies
    // read both ways (160 frames each way).
    let frames: u32 = rows.iter().map(|(_, r)| r.frames).sum();
    eprintln!("fixtures: {} files, {} rows, {} frame comparisons", files.len(), rows.len(), frames);
    assert_eq!((files.len(), rows.len(), frames), (36, 67, 563));
}

/// The contract at every size: max 0 per channel, every variant, each Alpha Type.
#[test]
fn every_variant_at_every_size_equals_the_cpu_reference() {
    let Some((g, presenter)) = gpu() else { return };
    let scratch = Scratch::new("exact-sweep");
    let mut rows = Vec::new();
    for kind in Kind::ALL {
        for (w, h) in SIZES {
            let path = scratch.file(&format!("{kind:?}_{w}x{h}.mov"), &kind.movie(w, h, FRAMES));
            let clip = routed(&path);
            assert_eq!((clip.info.width, clip.info.height, clip.info.frames), (w, h, FRAMES), "{kind:?} {w}x{h}");
            assert_eq!(clip.route, if kind.is_dxv() { Route::Dxv } else { Route::Hap }, "{kind:?}");
            let clip_rows = clip_rows(&g, &presenter, &clip, BlockPath::Exact, 1).unwrap();
            // DXV alpha and opaque clips are read both ways; HAP alpha is stated.
            assert_eq!(clip_rows.len(), if kind.is_dxv() || matches!(kind, Kind::Hap1 | Kind::HapY) { 2 } else { 1 }, "{kind:?}");
            for row in clip_rows {
                assert_eq!(row.frames, FRAMES, "{kind:?} {w}x{h}");
                eprintln!("{}", line(&format!("{kind:?} {w}x{h}"), &row));
                rows.push((format!("{kind:?} {w}x{h}"), row));
            }
            std::fs::remove_file(&path).unwrap();
        }
    }
    // 8 variants read both ways + 4 HAP variants with stated alpha, at 9 sizes.
    assert_eq!(rows.len(), (8 * 2 + 4) * SIZES.len());
    assert_exact(&rows);
}

/// The pictures of DXVKit's test vectors hash to DXVKit's own reference RGBA (the C library the
/// Swift apps link): the bar's reference is DXVKit, not only DXVKit Plus's port of it.
#[test]
fn dxvkit_vector_pictures_hash_to_dxvkits_reference_rgba() {
    let Some((g, presenter)) = gpu() else { return };
    let table = std::fs::read_to_string(fixtures().join("dxvkit/vectors.tsv")).unwrap();
    let mut view = ClipView::new(&g.device);
    let (mut checked, mut wrong) = (0, Vec::new());
    for row in table.lines().filter(|l| !l.starts_with('#')) {
        let col: Vec<&str> = row.split('\t').collect();
        let (vector, group, frame, expected) = (col[0], col[1], col[2].parse::<u32>().unwrap(), col[8]);
        let path = fixtures().join(format!("dxvkit/streams/{group}/{vector}.mov"));
        let Some(reference) = col[9].split(' ').find_map(|h| h.strip_prefix("rgba=")) else { continue };
        if expected != "decode" || !path.exists() {
            continue;
        }
        let decoded = routed(&path).open().unwrap().decode(frame).unwrap();
        // As stored: DXVKit's RGBA is the raw decoded texel.
        let picture = view.show(&g, &presenter, &decoded, AlphaView::Premultiplied).unwrap().clone();
        let rgba = read_texture_rgba(&g.device, &g.queue, &picture).unwrap();
        let hash: String = Sha256::digest(&rgba).iter().map(|b| format!("{b:02x}")).collect();
        if hash != reference {
            wrong.push(format!("{vector} frame {frame}"));
        }
        checked += 1;
    }
    assert!(wrong.is_empty(), "{} of {checked} pictures do not hash to DXVKit's RGBA: {wrong:?}", wrong.len());
    eprintln!("DXVKit vectors: {checked} pictures hash to the reference RGBA");
    assert_eq!(checked, 64);
    // Negative control: the same picture read Straight is another picture (alpha below 255 exists).
    let decoded = routed(&fixtures().join("dxvkit/streams/glenc/glenc_dxt5_pat64.mov")).open().unwrap().decode(0).unwrap();
    let stored = read_texture_rgba(&g.device, &g.queue, &view.show(&g, &presenter, &decoded, AlphaView::Premultiplied).unwrap().clone()).unwrap();
    let straight = read_texture_rgba(&g.device, &g.queue, &view.relook(&g, &presenter, AlphaView::Straight).unwrap().clone()).unwrap();
    assert_ne!(Sha256::digest(&stored), Sha256::digest(&straight));
}

/// Negative controls for the comparisons above, for every variant: one flipped bit in one block
/// fails the comparison with the original frame (and the picture follows the flip: it still equals
/// the reference of the flipped frame); and a clip with alpha read with the other Alpha Type fails.
#[test]
fn a_flipped_bit_and_the_wrong_alpha_type_are_caught() {
    let Some((g, presenter)) = gpu() else { return };
    let scratch = Scratch::new("exact-flip");
    let mut view = ClipView::new(&g.device);
    let (w, h) = (17u32, 33u32);
    let (mut flips, mut alpha_checks) = (0, 0);
    for kind in Kind::ALL {
        let planes = kind.frame_planes(w, h, 0);
        let decode = |planes: &[Vec<u8>]| {
            let path = scratch.file(&format!("{kind:?}.mov"), &support::video_movie(kind.fourcc(), w, h, &[kind.packet(planes)]));
            routed(&path).open().unwrap().decode(0).unwrap()
        };
        let original = decode(&planes);
        for alpha in [AlphaView::Premultiplied, AlphaView::Straight] {
            let want = expected_picture(&original, alpha).unwrap();
            let caught = (0..256usize).any(|bit| {
                let mut flipped = planes.clone();
                flipped[0][bit / 8] ^= 1 << (bit % 8);
                let frame = decode(&flipped);
                let want_flipped = expected_picture(&frame, alpha).unwrap();
                if want_flipped == want {
                    return false; // this bit does not reach the picture; try the next
                }
                let got = read_texture_rgba(&g.device, &g.queue, &view.show(&g, &presenter, &frame, alpha).unwrap().clone()).unwrap();
                assert_eq!(diff(&want_flipped, &got), (0, 0), "{kind:?} {alpha:?}: exact on the flipped frame");
                assert!(diff(&want, &got).0 > 0, "{kind:?} {alpha:?} bit {bit}: the flip must be seen");
                true
            });
            assert!(caught, "{kind:?} {alpha:?}: no bit of the first blocks changes the picture");
            flips += 1;
        }
        // Random alpha: reading the frame the other way must not match.
        let (stored, straight) = (expected_picture(&original, AlphaView::Premultiplied).unwrap(), expected_picture(&original, AlphaView::Straight).unwrap());
        // An alpha-only frame is white at its alpha either way; an opaque one does not change.
        let has_alpha = kind != Kind::HapA && stored.as_chunks::<4>().0.iter().any(|p| p[3] != 255);
        assert_eq!(stored != straight, has_alpha, "{kind:?}: only clips with alpha change with the Alpha Type");
        if has_alpha {
            let got = read_texture_rgba(&g.device, &g.queue, &view.show(&g, &presenter, &original, AlphaView::Straight).unwrap().clone()).unwrap();
            assert_eq!(diff(&straight, &got), (0, 0), "{kind:?}");
            assert!(diff(&stored, &got).0 > 0, "{kind:?}: Straight must not pass as Premultiplied");
            alpha_checks += 1;
        }
    }
    // Dxt5, Yg10, LegacyDxt5, Hap5, HapM, Hap7 carry alpha below 255 in random blocks.
    assert_eq!((flips, alpha_checks), (24, 6));
}

/// The block path is the view's own: a one-clip viewer gets the exact path by default, a view can
/// be put on the sampled path (a many-layer grid), and the harness tells them apart — the GPU's
/// fixed-function BC decode is not exact on random blocks.
#[test]
fn the_block_path_is_per_view_and_only_the_exact_one_is_exact() {
    let Some((g, presenter)) = gpu() else { return };
    assert_eq!(ClipView::new(&g.device).path(), BlockPath::Exact);
    assert_eq!(ClipView::with_path(&g.device, BlockPath::Sampled).path(), BlockPath::Sampled);
    let scratch = Scratch::new("exact-path");
    for kind in [Kind::Dxt5, Kind::Hap5, Kind::HapM] {
        let clip = routed(&scratch.file(&format!("{kind:?}.mov"), &kind.movie(256, 144, 1)));
        let exact = clip_rows(&g, &presenter, &clip, BlockPath::Exact, 1).unwrap();
        let sampled = clip_rows(&g, &presenter, &clip, BlockPath::Sampled, 1).unwrap();
        assert!(exact.iter().all(Row::exact), "{kind:?}");
        assert!(sampled.iter().all(|r| r.worst > 0 && r.worst <= 3 && !r.exact()), "{kind:?}: {sampled:?}");
        eprintln!("{kind:?}: sampled path max diff {}", sampled[0].worst);
    }
    // A view changes path between frames.
    let clip = routed(&scratch.file("switch.mov", &Kind::Hap5.movie(64, 64, 1)));
    let frame = clip.open().unwrap().decode(0).unwrap();
    let want = expected_picture(&frame, AlphaView::Straight).unwrap();
    let mut view = ClipView::with_path(&g.device, BlockPath::Sampled);
    let sampled = read_texture_rgba(&g.device, &g.queue, &view.show(&g, &presenter, &frame, AlphaView::Straight).unwrap().clone()).unwrap();
    view.set_path(BlockPath::Exact);
    assert!(view.relook(&g, &presenter, AlphaView::Straight).is_err(), "the frame on the GPU belongs to the old path");
    let exact = read_texture_rgba(&g.device, &g.queue, &view.show(&g, &presenter, &frame, AlphaView::Straight).unwrap().clone()).unwrap();
    assert_eq!(exact, want);
    assert_ne!(sampled, want);
}

/// HAP HDR has no CPU decode: it routes, plays, and the picture is DXVKit Plus's own GPU
/// conversion of the frame.
#[test]
fn hap_hdr_plays_and_matches_dxvkit_plus() {
    let Some((g, presenter)) = gpu() else { return };
    let clip = routed(&fixtures().join("synthetic_haph.mov"));
    assert_eq!((clip.route, clip.info.codec.as_str()), (Route::Hap, "HapH"));
    let rows = clip_rows(&g, &presenter, &clip, BlockPath::Exact, 1).unwrap();
    assert!(!rows.is_empty() && rows.iter().all(|r| r.exact() && r.reference == Reference::DxvkitPlusGpu && r.frames == clip.info.frames), "{rows:?}");
}

/// The OS codecs route to the OS decoder and show a picture of the clip's size.
#[cfg(target_os = "macos")]
#[test]
fn os_codec_fixtures_route_to_the_os_decoder_and_show() {
    let Some((g, presenter)) = gpu() else { return };
    for name in ["tiles_h264.mp4", "tiles_hevc.mp4", "tiles_mjpeg.mov", "tiles_prores422.mov", "tiles_prores4444.mov"] {
        let clip = routed(&fixtures().join(name));
        assert_eq!(clip.route, Route::Native, "{name}");
        let rows = clip_rows(&g, &presenter, &clip, BlockPath::Exact, 1).unwrap();
        assert!(rows.iter().all(|r| r.reference == Reference::None && r.frames == clip.info.frames && r.nonblank), "{name}: {rows:?}");
        assert_eq!(rows[0].texels, u64::from(clip.info.frames) * u64::from(clip.info.width) * u64::from(clip.info.height), "{name}");
    }
}

/// A clip larger than the device's largest texture is an error that says so, not a wgpu validation
/// error and a blank picture (DXVKit Plus's check since v0.9.0). The family's device asks for the
/// adapter's real limit; this test uses a device with wgpu's default limit (8192) and a frame 8196
/// wide.
#[test]
fn a_clip_larger_than_the_gpu_allows_is_an_error_value() {
    if gpu().is_none() {
        return;
    }
    // A device with wgpu's default limits, made by hand (DXVKit Plus's own devices ask for the
    // adapter's limit since v0.9.0), as DXVKit Plus's `wgpu/tests/limits.rs` does.
    let small = {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..wgpu::InstanceDescriptor::new_without_display_handle() });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
        let required_features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features,
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .unwrap();
        Gpu { instance, adapter, device, queue }
    };
    let limit = small.device.limits().max_texture_dimension_2d;
    let ours = glance_gpu::open_gpu().unwrap();
    assert!(ours.device.limits().max_texture_dimension_2d >= limit);
    eprintln!("largest texture: default device {limit}, the family's device {}", ours.device.limits().max_texture_dimension_2d);
    let scratch = Scratch::new("exact-limit");
    let frame = routed(&scratch.file("wide.mov", &Kind::Hap1.movie(limit + 4, 4, 1))).open().unwrap().decode(0).unwrap();
    let presenter = Presenter::new(&small.device);
    let error = ClipView::new(&small.device).show(&small, &presenter, &frame, AlphaView::Premultiplied).expect_err("refused");
    assert!(matches!(&error, glance_gpu::GpuError::Unsupported(m) if m.contains("larger than this GPU's largest texture")), "{error}");
    // The family's device holds it when the adapter does, and the picture is exact.
    if ours.device.limits().max_texture_dimension_2d >= limit + 4 {
        let presenter = Presenter::new(&ours.device);
        let rows = clip_rows(&ours, &presenter, &routed(&scratch.file("wide.mov", &Kind::Hap1.movie(limit + 4, 4, 1))), BlockPath::Exact, 1).unwrap();
        assert!(rows.iter().all(Row::exact), "{rows:?}");
    }
}

/// A reserved-mode BC7 block (HAP R from a hostile or broken writer) is transparent black on every
/// GPU on the exact block path: DXVKit Plus decodes BC7 from integer texels there since v0.9.0
/// (`core::bc7` line for line), as the CPU decoder does. Up to v0.8.0 the GPU's own BC7 decoder gave
/// its own value (opaque black on CI's paravirtual GPU).
#[test]
fn reserved_bc7_blocks_are_transparent_black_on_the_exact_path() {
    let Some((g, presenter)) = gpu() else { return };
    let scratch = Scratch::new("exact-bc7-reserved");
    let clip = routed(&scratch.file("reserved.mov", &support::reserved_bc7(16, 8)));
    let frame = clip.open().unwrap().decode(0).unwrap();
    // As stored, so the GPU's own answer is read, not its product with alpha.
    let cpu = expected_picture(&frame, AlphaView::Premultiplied).unwrap();
    assert!(cpu.iter().all(|&b| b == 0), "the CPU decode of a reserved block is transparent black");
    let mut view = ClipView::new(&g.device);
    let got = read_texture_rgba(&g.device, &g.queue, &view.show(&g, &presenter, &frame, AlphaView::Premultiplied).unwrap().clone()).unwrap();
    let adapter = g.adapter.get_info().name;
    eprintln!("reserved BC7 block on {adapter}: GPU {:?}, CPU [0, 0, 0, 0], max diff {}", &got[..4], diff(&cpu, &got).0);
    assert_eq!(view.path(), BlockPath::Exact);
    assert_eq!(diff(&cpu, &got), (0, 0), "{adapter}: a reserved BC7 block is not transparent black on the exact path");
}
