//! glance-encode end to end on the core's CC0 fixtures (`tests/fixtures/tiles_*`): every DXV / HAP format from DXV,
//! HAP and native sources decodes back with the frame count, size and FourCC asked for; trim, crop, resize and
//! letterbox give the frames and sizes Swift GlEnc's pipeline gives; failures and cancels leave no file; the source
//! limits and words are Swift's. Byte identity with Swift GlEnc itself is glenc-rs's cross-test (its oracle).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use glance_encode::audio::AudioPlan;
use glance_encode::codec::{DxvFormat, OutputCodec, OutputContainer, VideoSettings};
use glance_encode::frame::CropRect;
use glance_encode::media::container::Movie;
use glance_encode::size::{AspectMode, OutputSize, ResizeQuality};
use glance_encode::source::{AlphaType, SourceError, validate_geometry};
use glance_encode::{EncodeError, EncodeRequest, encode};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(name)
}

fn out(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("glance-encode-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn req(src: &str, out_name: &str, codec: OutputCodec) -> EncodeRequest {
    EncodeRequest {
        source: fixture(src),
        output: out(out_name),
        codec,
        container: OutputContainer::Mov,
        video: VideoSettings::default(),
        output_size: OutputSize::Original,
        resize_quality: ResizeQuality::Auto,
        aspect: AspectMode::Letterbox,
        crop: None,
        frames: None,
        writer_version: "GlEnc test".into(),
        hap_chunks: 1,
        alpha_type: AlphaType::Premultiplied,
        audio: AudioPlan::default(),
        refined_bc4: false,
    }
}

fn probe(p: &Path) -> (String, u32, u32, usize) {
    let m = Movie::open(p).unwrap();
    let t = m.first().unwrap();
    (String::from_utf8_lossy(&t.fourcc).into_owned(), t.width, t.height, t.samples.len())
}

fn run(r: &EncodeRequest) -> Result<glance_encode::EncodeOutcome, EncodeError> {
    encode(r, &|_| {}, &AtomicBool::new(false))
}

#[test]
fn every_dxv_and_hap_format_from_every_source_kind() {
    let src = probe(&fixture("tiles_dxt5.mov"));
    for s in ["tiles_dxt5.mov", "tiles_hap5.mov", "tiles_prores4444.mov"] {
        for f in DxvFormat::ALL {
            let name = format!("{s}-{}.mov", f.raw());
            let r = req(s, &name, OutputCodec::Dxv(f));
            let o = run(&r).unwrap_or_else(|e| panic!("{name}: {e}"));
            let (cc, w, h, n) = probe(&r.output);
            assert_eq!(cc.as_bytes(), &f.stream_fourcc(), "{name}");
            let sp = probe(&fixture(s));
            assert_eq!((w, h), (sp.1, sp.2), "{name}");
            assert_eq!(n as u64, o.frames, "{name}");
            assert!(n > 0);
            let _ = src;
            std::fs::remove_file(&r.output).unwrap();
        }
    }
}

#[test]
fn trim_crop_and_resize_shape_the_output() {
    let (_, sw, sh, n) = probe(&fixture("tiles_dxt1.mov"));
    assert!(n >= 4);
    let mut r = req("tiles_dxt1.mov", "trim.mov", OutputCodec::Dxv(DxvFormat::Dxt1));
    r.frames = Some(1..3);
    let o = run(&r).unwrap();
    assert_eq!(o.frames, 2);
    assert_eq!(probe(&r.output).3, 2);
    r.frames = None;
    r.crop = Some(CropRect { x: 1, y: 2, width: sw / 2 + 1, height: sh / 2 + 3 });
    r.output = out("crop.mov");
    run(&r).unwrap();
    assert_eq!((probe(&r.output).1, probe(&r.output).2), (sw / 2 + 1, sh / 2 + 3));
    r.crop = None;
    r.output = out("resize.mov");
    r.output_size = OutputSize::Custom { width: 64, height: 48 };
    r.resize_quality = ResizeQuality::Nearest;
    run(&r).unwrap();
    assert_eq!((probe(&r.output).1, probe(&r.output).2), (64, 48));
}

#[test]
fn bad_geometry_fails_with_swifts_words_and_leaves_no_file() {
    let mut r = req("tiles_dxt1.mov", "bad.mov", OutputCodec::H264);
    r.container = OutputContainer::Mp4;
    r.crop = Some(CropRect { x: 0, y: 0, width: 9, height: 10 });
    let e = run(&r).unwrap_err().to_string();
    assert_eq!(e, "EncodePipeline: cropRect (x=0, y=0, 9×10) must have integer-valued 4-pixel-aligned coords (L3); got non-aligned");
    assert!(!r.output.exists());
    r.crop = Some(CropRect { x: 0, y: 0, width: 99_998, height: 10 });
    let e = run(&r).unwrap_err().to_string();
    assert!(e.starts_with("EncodePipeline: cropRect (0.0, 0.0, 99998.0, 10.0) extends outside source dims"), "{e}");
    assert!(!r.output.exists());
}

#[test]
fn a_cancelled_encode_leaves_no_file() {
    let r = req("tiles_dxt5.mov", "cancel.mov", OutputCodec::Dxv(DxvFormat::Hap5));
    let e = encode(&r, &|_| {}, &AtomicBool::new(true)).unwrap_err();
    assert!(matches!(e, EncodeError::Cancelled));
    assert_eq!(e.to_string(), "Cancelled");
    assert!(!r.output.exists());
}

#[test]
fn a_file_without_video_fails_and_leaves_no_file() {
    let junk = out("junk.mov");
    std::fs::write(&junk, b"not a movie at all").unwrap();
    let mut r = req("tiles_dxt1.mov", "junk-out.mov", OutputCodec::Dxv(DxvFormat::Dxt1));
    r.source = junk;
    let e = run(&r).unwrap_err().to_string();
    assert!(e.starts_with("EncodePipeline: "), "{e}");
    assert!(!r.output.exists());
}

#[test]
fn a_rate_outside_the_writers_set_is_refused() {
    for fps in [12.5, 47.952, 119.88] {
        assert!(glance_encode::enc::mov::derive_timescale(fps).is_err(), "{fps}");
    }
    assert_eq!(glance_encode::enc::mov::derive_timescale(30.0).unwrap(), (15_360, 512));
    assert_eq!(glance_encode::enc::mov::derive_timescale(30_000.0 / 1001.0).unwrap(), (30_000, 1001));
}

#[test]
fn source_limits_are_swifts() {
    assert_eq!(validate_geometry(0, 10, 30.0, 1.0), Err(SourceError::DimensionsInvalid { width: 0, height: 10 }));
    assert_eq!(validate_geometry(65_536, 10, 30.0, 1.0), Err(SourceError::DimensionsTooLarge { width: 65_536, height: 10 }));
    assert_eq!(validate_geometry(20_000, 20_000, 30.0, 1.0), Err(SourceError::FrameTooLarge { width: 20_000, height: 20_000 }));
    assert_eq!(validate_geometry(10, 10, 0.0, 1.0).unwrap_err().to_string(), "source reports an invalid frame rate (0.0 fps)");
    assert_eq!(validate_geometry(10, 10, f64::NAN, 1.0).unwrap_err().to_string(), "source reports an invalid frame rate (nan fps)");
    assert_eq!(validate_geometry(10, 10, 30.0, -1.0).unwrap_err().to_string(), "source reports an invalid duration (-1.0 seconds)");
    assert_eq!(SourceError::NoVideoTrack.to_string(), "source has no video track");
    assert!(validate_geometry(16_384, 16_384, 30.0, 0.0).is_ok());
}

#[test]
fn progress_runs_to_one_over_the_trimmed_range() {
    let mut r = req("tiles_dxt1.mov", "progress.mov", OutputCodec::Dxv(DxvFormat::Dxt1));
    r.frames = Some(0..2);
    let seen = std::sync::Mutex::new(Vec::new());
    encode(&r, &|p| seen.lock().unwrap().push(p), &AtomicBool::new(false)).unwrap();
    let s = seen.into_inner().unwrap();
    assert_eq!(s, vec![0.5, 1.0, 1.0]);
}
