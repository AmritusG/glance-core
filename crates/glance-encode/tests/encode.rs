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

/// A fragmented MP4 (samples in `moof` fragments, the `moov`'s tables empty, as in a DASH or streaming capture):
/// up to v0.7.1 it failed ("source reports an invalid frame rate (0.0 fps)"); v0.7.2 took the platform's timing;
/// since v0.8.0 (DXVKit Plus 0.15.0 reads fragments) the rate and the frame count are its samples'. Every frame is
/// written, at the source's 30 fps, and the reader counts the fragments' 24 frames (a user's capture whose
/// AVFoundation length is twice its samples' made the progress stop near half: glenc-rs docs/data/p3).
#[cfg(target_os = "macos")]
#[test]
fn a_fragmented_mp4_converts_at_its_rate() {
    let src = fixture("tiles_h264_fragmented.mp4");
    let m = Movie::open(&src).unwrap();
    assert!(m.fragments > 0 && m.first().unwrap().frames() == 24, "the demuxer reads the fragments");
    let info = glance_encode::source::open_source(&src, glance_encode::frame::FrameAlpha::Opaque, AlphaType::Premultiplied).unwrap().info();
    assert_eq!((info.fps, info.total_frames), (30.0, 24));
    for (codec, name) in
        [(OutputCodec::Dxv(DxvFormat::Dxt1), "frag-dxt1.mov"), (OutputCodec::Dxv(DxvFormat::Hap1), "frag-hap1.mov"), (OutputCodec::H264, "frag-h264.mov")]
    {
        let r = req("tiles_h264_fragmented.mp4", name, codec);
        let seen = std::sync::Mutex::new(Vec::new());
        let o = encode(&r, &|p| seen.lock().unwrap().push(p), &AtomicBool::new(false)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(o.frames, 24, "{name}");
        // The progress reaches 1 on the last frame, before the closing report.
        let s = seen.into_inner().unwrap();
        assert_eq!((s.len(), s[23]), (25, 1.0), "{name}");
        let m = Movie::open(&r.output).unwrap();
        let t = m.first().unwrap();
        assert_eq!((t.width, t.height, t.samples.len()), (128, 72, 24), "{name}");
        assert_eq!((t.timing.rate.num, t.timing.rate.den), (30, 1), "{name}");
    }
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

/// v0.8.1: a native source to the platform's writer with no crop or resize hands the decoder's own buffer over (no
/// copy), as Swift GlEnc does: every frame is written and the decoder's colour attachments reach the file (`colr`);
/// a resized job rebuilds the buffer and has none (Swift's rule, DXVKit Plus `glenc_av` tests).
#[cfg(target_os = "macos")]
#[test]
fn the_decoders_buffer_goes_to_the_platform_writer() {
    let has_colr = |p: &Path| std::fs::read(p).unwrap().windows(8).any(|w| w == b"colrnclc");
    let r = req("tiles_prores422.mov", "through.mov", OutputCodec::ProRes(glance_encode::codec::ProResVariant::ProRes422));
    assert_eq!(run(&r).unwrap().frames, 24);
    let m = Movie::open(&r.output).unwrap();
    assert_eq!((m.first().unwrap().samples.len(), has_colr(&r.output)), (24, true));
    let mut resized = req("tiles_prores422.mov", "rebuilt.mov", OutputCodec::ProRes(glance_encode::codec::ProResVariant::ProRes422));
    resized.output_size = OutputSize::Custom { width: 64, height: 36 };
    assert_eq!(run(&resized).unwrap().frames, 24);
    assert!(!has_colr(&resized.output));
}

/// v0.9.2: frames an app made, written by the platform writer: the count, the size and the colours come back; a DXV
/// codec and a wrong-sized frame are refused.
#[cfg(target_os = "macos")]
#[test]
fn a_frame_writer_writes_what_it_is_given() {
    use glance_encode::FrameWriter;
    use glance_encode::frame::FrameAlpha;
    use glance_encode::source::open_source;
    let path = out("frames.mp4");
    let (w, h) = (128u32, 72u32);
    let mut fw = FrameWriter::create(&path, OutputCodec::H264, OutputContainer::Mp4, (w, h), 30, VideoSettings::default()).unwrap();
    for i in 0..30u32 {
        // Red for the first half, blue for the second.
        let c = if i < 15 { [220u8, 30, 30, 255] } else { [30, 40, 220, 255] };
        fw.push(c.repeat((w * h) as usize)).unwrap();
    }
    assert!(fw.push(vec![0; 16]).is_err(), "a frame of the wrong size is refused");
    assert_eq!(fw.finish().unwrap(), 30);
    let mut r = open_source(&path, FrameAlpha::Opaque, AlphaType::Premultiplied).unwrap();
    let info = r.info();
    assert_eq!((info.width, info.height, info.total_frames), (w, h, 30));
    assert!((info.fps - 30.0).abs() < 0.01, "{}", info.fps);
    let mut frames = Vec::new();
    while let Some(f) = r.next_frame().unwrap() {
        let mid = ((h / 2 * w + w / 2) * 4) as usize;
        frames.push([f.rgba[mid], f.rgba[mid + 1], f.rgba[mid + 2]]);
    }
    assert_eq!(frames.len(), 30);
    // H.264 is YUV: the colours come back within the round trip's shift (measured: red 220, 30, 30 → 203, 9, 33).
    let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| (i32::from(*x) - i32::from(y)).abs() <= 28);
    assert!(near(frames[3], [220, 30, 30]) && frames[3][0] > 150, "{:?}", frames[3]);
    assert!(near(frames[25], [30, 40, 220]) && frames[25][2] > 150, "{:?}", frames[25]);
    assert!(FrameWriter::create(&out("x.mov"), OutputCodec::Dxv(DxvFormat::Dxt1), OutputContainer::Mov, (w, h), 30, VideoSettings::default()).is_err());
}
