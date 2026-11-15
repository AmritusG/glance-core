//! The clip's sound as glance-audio decodes it, on the committed audio fixtures (DXVKit Plus's,
//! CC0): every PCM layout DXVKit Plus reads comes back as checked `f32` frames; the refused
//! layout is an "audio unreadable" error naming the codec; AAC decodes on macOS; resampled to a
//! device rate the track keeps its length.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use glance_audio::{AudioError, AudioState, decode_track, resample};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(name)
}

#[test]
fn every_pcm_layout_decodes_and_the_refused_one_says_why() {
    let path = fixture("audio_pcm_formats.mov");
    let info = glance_media::route(&path).unwrap().info;
    assert_eq!(info.audio.len(), 27);
    let mut decoded = 0;
    for track in &info.audio {
        let pcm = glance_media::open_audio(&path, track.index).map_err(AudioError::from).and_then(|mut r| decode_track(&mut r));
        match pcm {
            Ok(p) => {
                assert_eq!((p.channels, p.rate, p.frames()), (track.channels, track.sample_rate, track.frames), "track {}", track.index);
                // Integers scale into -1..1; float tracks keep their values as stored (the fixture's reach 1.5).
                let peak = p.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                assert!(p.samples.iter().all(|s| s.is_finite()), "track {}", track.index);
                assert!(track.codec_name.contains("float") || peak <= 1.0, "track {} peak {peak}", track.index);
                assert!(p.samples.iter().any(|s| *s != 0.0), "track {} is not silent", track.index);
                decoded += 1;
            }
            Err(e) => {
                // Track 26: non-interleaved lpcm, refused by DXVKit Plus as by AVFoundation.
                assert_eq!(track.index, 26, "{e}");
                assert!(matches!(&e, AudioError::Unreadable(m) if m.contains("audio")), "{e}");
                assert!(matches!(e.state(), AudioState::Failed { .. }));
            }
        }
    }
    assert_eq!(decoded, 26);
}

#[test]
fn aac_decodes_through_the_platform_and_resamples() {
    let path = fixture("audio_aac_priming.mov");
    let info = glance_media::route(&path).unwrap().info;
    let reader = glance_media::open_audio(&path, 0);
    if cfg!(not(target_os = "macos")) {
        // No AAC decoder on this platform yet (DXVKit Plus's Windows backend): a reason, never a panic.
        assert!(reader.is_err());
        return;
    }
    let pcm = decode_track(&mut reader.unwrap()).unwrap();
    assert_eq!((pcm.rate, pcm.frames()), (info.audio[0].sample_rate, info.audio[0].frames));
    let to = if pcm.rate == 48_000 { 44_100 } else { 48_000 };
    let out = resample(pcm.samples.clone(), pcm.channels, pcm.rate, to).unwrap();
    let frames = out.len() as f64 / f64::from(pcm.channels);
    let want = pcm.frames() as f64 * f64::from(to) / f64::from(pcm.rate);
    assert!((frames - want).abs() <= 1.0, "{frames} vs {want}");
}
