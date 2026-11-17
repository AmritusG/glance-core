//! The end of a clip that does not loop, with its sound driving the clock, on the real output
//! device (docs/02-PARITY.md, Glance "Transport": with loop off playback stops on the last frame,
//! Play restarts from frame 0 — as without sound). Found by MaCrate's preview (Glance 0.19.0):
//! the heard position stalled 3 to 10 ms short of the end (759.4–761.2e9 of 762.0e9 ticks on a
//! 3.0 s HAP clip), so the clock never stopped.
//!
//! Runs on Amrit's test media (`_master_hapm_512x512.mov`: HAP Q, 3.0 s, sound as long as the
//! picture) when `GLANCE_TESTFILES` names its folder, and only with an audio output device (it
//! skips otherwise). The sound plays muted: the position runs on at gain 0.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_playback::{AudioClock, AudioMaster, Clock, Master, clip_of, seconds};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn with_sound_a_clip_that_does_not_loop_stops_on_its_last_frame_and_play_starts_from_frame_0() {
    let Some(root) = std::env::var_os("GLANCE_TESTFILES").map(PathBuf::from).filter(|p| p.is_dir()) else {
        eprintln!("SKIP: GLANCE_TESTFILES does not name Amrit's test media folder");
        return;
    };
    if glance_audio::default_output().is_err() {
        eprintln!("SKIP: no audio output device");
        return;
    }
    let path = root.join("_master_hapm_512x512.mov");
    let routed = glance_media::route(&path).unwrap();
    let clip = clip_of(routed.open().unwrap().as_ref());
    let last = clip.frames - 1;
    let mut audio = AudioClock::open(&path, &routed.info.audio, true).unwrap();
    audio.set_muted(true);
    let audio_end = AudioMaster::end(&audio).unwrap();
    let mut clock = Clock::new(clip, Ok(Box::new(audio)));
    clock.set_looping(false);
    assert_eq!(clock.master(), Master::Audio);

    // Play through: ticks every 4 ms until the clock stops, for at most the clip plus 1.5 s.
    let start = Instant::now();
    let limit = Duration::from_secs_f64(3.0 + 1.5);
    let mut heard = None;
    while !clock.is_paused() && start.elapsed() < limit {
        let now = Instant::now();
        clock.tick(now);
        heard = clock.audio_position(now).or(heard);
        std::thread::sleep(Duration::from_millis(4));
    }
    let took = start.elapsed();
    eprintln!(
        "stopped: {} after {:.3} s on frame {} of {last}; last heard {:?} s, the sound ends at {:.6} s",
        clock.is_paused(),
        took.as_secs_f64(),
        clock.current(),
        heard.map(seconds),
        seconds(audio_end)
    );
    assert_eq!(clock.master(), Master::Audio, "the sound drove the clock to the end");
    assert!(clock.is_paused(), "stops at the end (last heard {:?} of {audio_end:?})", heard);
    assert_eq!(clock.current(), last, "on the last frame");
    // Stopped when the end was heard, not long after (the clip is 3.0 s; the device's latency and
    // the 0.25 s prime are inside the margin).
    assert!(took < Duration::from_secs_f64(3.0 + 0.5), "{took:?}");

    // Play starts again from frame 0 and moves on.
    clock.play();
    assert_eq!(clock.current(), 0);
    let start = Instant::now();
    while clock.tick(Instant::now()) < 10 && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(4));
    }
    assert!((10..last).contains(&clock.current()), "playing again from 0: frame {}", clock.current());
    assert!(!clock.is_paused());
    assert_eq!(clock.master(), Master::Audio);
}
