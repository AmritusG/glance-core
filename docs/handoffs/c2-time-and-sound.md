# C2 — time and sound (glance-core v0.2.0)

Brief: `~/claude-handoffs/glance-core_2026-11-15_c2-time-and-sound_brief.md`. Machine: <host>
(Apple M5 Max, macOS 27.0.1). Measured numbers: `docs/05-C2-RESULTS.md`; raw data
`docs/data/c2-*`, `docs/data/c2-runs/`.

## What shipped

- **DXVKit Plus pinned at v0.11.0** (29fa9f5); every workaround its W2 / W4 notes replace is deleted
  (DIAGNOSE 3) and the five pinned tests flipped to the Swift-matching answers.
- **Deployment targets** (`.cargo/config.toml`): macOS 14.0, iOS 15.0 (placeholder). CI builds the
  Intel `glance-proof` and starts it under Rosetta 2.
- **`glance-audio`** (new crate): the clip's first audio track, decoded whole by DXVKit Plus (PCM; AAC
  through AudioToolbox), checked at the trust boundary with Swift's words, resampled once to the
  output device's own rate (rubato FFT), mapped to the device's channels in the callback, volume and
  mute as a gain only, and the position heard (CoreAudio timestamps). Errors map to Swift's
  `none` / `failed (reason)` states.
- **`glance-playback::Clock`**: the audio-master clock (nearest frame to the sound heard when the
  picture is shown, clamped), the 0.25 s fallback re-anchored at the frame on screen, the wall clock
  on DXVKit Plus's transport, and Glance's transport rules: seek, step, restart, scrub, loop, end of
  a non-looping clip (Play restarts from frame 0 on every path — Amrit: "fix"). Behind an
  `AudioMaster` trait so the rules are tested on CI with a scripted sound. `Clip::from_timing`
  everywhere (`clip_of`). `ClipPlayer` with `Sound::{Device, Off}`.
- **`glance-proof`** plays clips with sound (`--mute`, `--no-audio`, `--pace N`, `--no-loop`) and
  reports the clock, the sound's state, the A/V samples and loop periods; new `scrub` and `media`
  subcommands; `tools/c2-measure.sh` runs the measurement set.
- Fixtures (copied with `cp -p`, `ATTRIBUTION.md`): DXVKit Plus's CC0 `audio_pcm_formats.mov`,
  `audio_aac_priming.mov`; Swift Glance's eight MIT `frame-timing/frames_*.mov`.

## DIAGNOSE

### 1. Swift Glance's clock and transport (glance @ a9dc601, quoted from the source)

- **Audio-master target, round to nearest.** `FrameClock.audioMasterTarget`:
  `var target = Int((audioSeconds * frameRate).rounded())`, then `if target >= totalFrames { target = totalFrames - 1; if !loops { reachedEOF = true } }`, `if target < 0 { target = 0 }`
  ("Round-to-nearest (MaCrate Phase 7.c lesson): floor leaves video up to a full frame behind audio")
  — `Sources/GlancePlayback/FrameClock.swift:365-378`. Swift's `rounded()` rounds a half away from
  zero: a tie goes to the later frame, as DXVKit Plus's `Clip::nearest_frame`.
- **0.25 s never-prime fallback.** `audioPrimeGrace: CFTimeInterval = 0.25`; in `tick()`, a nil audio
  position holds the frame, and `audioWatchdogTripped(now:lastAudioOK:grace:alreadyFired:)` =
  `!alreadyFired && (now - lastAudioOK) > grace` fires `onAudioUnavailable` — `FrameClock.swift:99,
  285-301, 384-389`; armed in `play()` (`:216-219`). The player answers with
  `fallBackToWallClock(reason: "audio failed to start")`: stops the engine, clears the hooks and
  re-anchors with `clock.seek(to: clock.currentFrame)` — `DXVPlayer.swift:222-224, 252-265`.
  `play()` falls back with "audio engine failed to start" when the engine refuses (`:284-288`).
- **Wall clock re-anchored at the loop.** `let intoLoop = target % totalFrames; startFrame = intoLoop;
  startWallTime = CACurrentMediaTime() - (Double(intoLoop) / frameRate)` — `FrameClock.swift:328-336`;
  frame = `startFrame + Int((elapsed * frameRate).rounded(.down))` (`:325-327`).
- **Audio loops at its own length; video clamped.** `wrappedPositionSeconds`: `loops ?
  absFrames.truncatingRemainder(dividingBy: total) : min(absFrames, total - 1)` —
  `AudioClock.swift:184-197`; "a longer audio track holds the last video frame until audio wraps; a
  shorter one truncates to the audio length" — `FrameClock.swift:356-362`.
- **Seek / step / restart / scrub.** `FrameClock.seek(to:)` clamps, re-anchors, calls `onSeek` (the
  audio moves to `frame / frameRate`) and `onFrameChange` — `FrameClock.swift:234-246`,
  `DXVPlayer.swift:216-219`. `DXVPlayer.stepFrame`: `pause()` first ("A bare `clock.stepFrame` pauses
  only the video clock, so the follow-on seek hits `AudioClock.seek`'s isPlaying branch and resumes
  audio"), then `seek(to: max(0, min(totalFrames - 1, currentFrame ± 1)))` — `DXVPlayer.swift:310-319`;
  `AudioClock.seek` while paused defers to the next `play()` (`AudioClock.swift:227-239`). Restart:
  `func restart() { seekAbsolute(time: 0) }` "preserving play/pause state" —
  `Glance/PlayerModel.swift:480-481`. Scrub: `beginScrubDrag` pauses if playing and remembers it,
  `endScrubDrag` resumes only if it was playing — `PlayerModel.swift:524-545`. No backlog, latest
  frame wins: `requestDecode` overwrites `pendingFrame` under a lock and enqueues one service block
  at most — `DXVPlayer.swift:349-422`.
- **End of a non-looping clip.** Wall clock: `target = totalFrames - 1; isPaused = true`
  (`FrameClock.swift:337-340`); audio clock: `if result.reachedEOF { isPaused = true }` (`:314`).
  `play()` does not seek back, so Play at the end stays on the last frame on DXV and HAP; only
  `AVPlaybackController.play()` restarts (`if atEnd || currentFrame >= lastFrame { seek(toFrame: 0) }`,
  `Glance/AVPlaybackController.swift:166-172`). Amrit 2026-11-14: "fix".
- **Gain and mute never move the clock.** `applyGain()` "sets ONLY `playerNode.volume` … does NOT
  touch the sample-time position"; `effectiveGain = muted ? 0 : min(1, max(0, volume))` —
  `AudioClock.swift:266-281`. Mute is separate from volume "so unmute restores the prior level"
  (`:79-83`).
- **Malformed formats rejected.** `validateAudioFormat`: `sampleRate.isFinite, sampleRate > 0` else
  "non-finite or zero sample rate (…)"; `channels >= 1, channels <= 64` else "channel count … out of
  [1,64]" — `AudioClock.swift:356-363`; errors are `no audio track` / `invalid audio format: …` /
  `audio unreadable: …` (`:49-55`), and the player records `.none` / `.failed(reason:)` and keeps the
  video on the wall clock (`DXVPlayer.swift:193-231`).
- **No contradiction with the brief.** One quirk found by measuring (not in the brief): the clamp at
  the audio loop seam (section "Deviations").

### 2. cpal on macOS: position and output latency; the A/V offset

- **What cpal reports** (cpal 0.18.2, `src/host/coreaudio/macos/device.rs:877-923`): the output
  callback's `callback` instant is the AUHAL render callback's `AudioTimeStamp.mHostTime` converted
  with `mach_timebase_info`; `playback = callback + (device buffer frames + kAudioDevicePropertyLatency
  + kAudioDevicePropertySafetyOffset) / rate` (`get_device_extra_latency_frames`, `:1042-1051`).
  `Stream::now()` is on the same clock (`src/traits.rs:495-504`). cpal opens the device at the
  requested rate and *tries to set the hardware's physical format to it* (`set_physical_format`,
  `:838-849`), so glance-audio asks for the device's own default rate and never changes it.
- **What CoreAudio's timestamps mean** (Apple's Core Audio engineers on the coreaudio-api list, checked
  online; no Apple reference page states it): the output `mHostTime` "represents the time at which
  the first output sample" is consumed by the hardware, "in the future" of now; the delay "between
  the audio being consumed by the hardware and hitting the speaker" is the device latency plus the
  stream latency (Dan Klingler, 2017-07-29,
  https://www.mail-archive.com/coreaudio-api@lists.apple.com/msg01238.html). Jeff Moore (2009) gives
  the HAL's own relation `scheduled IOProc time + safety offset + buffer frame size == inOutputTime`,
  plus the device latency to reach the converter (coreaudio-api 2009-07, as summarised by the search;
  the archive page returned 502 when fetched). So cpal's `playback` counts the buffer and the safety
  offset twice in effect (they lie *before* the host time) and leaves out the stream latency.
- **What that means for the offset.** glance-audio takes `playback − buffer / rate` as the heard time
  of the callback's first frame (= host time + device latency + safety offset); remaining error: the
  safety offset (sound a few frames *earlier* than computed) and the unread stream latency (*later*).
  Measured here: cpal's whole delay 13.97 ms = 512-frame buffer 11.61 ms + 2.36 ms. The method, its
  limits (the picture estimate too) and the numbers: `docs/05-C2-RESULTS.md` §2.
- **Swift Glance's AVAudioEngine path**: its position is `playerNode.playerTime(forNodeTime:
  lastRenderTime)` — the *render* time, ahead of what is heard by the output latency — and its frame
  is drawn after a CVDisplayLink tick and a main-thread hop (`AudioClock.swift:166-177`,
  `FrameClock.swift:141-150`). Neither delay is compensated; they partly cancel. Glance-core asks the
  clock for the moment the picture will be shown and the sound heard at that moment.

### 3. What glance-core carried that DXVKit Plus now owns (W2 / W4 notes)

| Carried in C1 | Replaced by | Done |
|---|---|---|
| `glance_media::first_video_fourcc` + atom helpers (`route.rs`) | `dxvkit_plus_core::container::first_video_fourcc` (re-exported) | deleted |
| `display_name`, `unsupported_message`, `matroska_codec` (`route.rs`) | `dxvkit_plus_core::names::{codec_name, unsupported_message}` | deleted (`Route`, `NotPlayable`, `first_dxv_packet` kept) |
| `codec_label` (glance-media) | `MediaInfo::codec_name` | deleted |
| `dxv_variant_label(&Decoded)` (planes) | `dxv_variant_label(MediaInfo::dxv_variant)` (label text glance-core's) | input changed |
| `MediaInfo { … }` literal (test helper) | `..MediaInfo::default()` | changed |
| `transport::Clip { frames, rate }` literal | `Clip::from_timing(Arc<FrameTiming>)` | changed |
| `open_gpu`'s limits code | `Gpu::with_adapter` (DX12 choice kept) | deleted |
| `check_size` | DXVKit Plus's size check (`GpuError::Unsupported`, same words) | deleted |
| pinned tests (two-track movie → HAP; `KNOWN`; 7 refused; cut-off HapY message; small-device test; reserved BC7 GPU's own value) | the Swift-matching answers | flipped |

## Validated (evidence per claim)

- Gates on the final tree: fmt; clippy `-D warnings`; `GLANCE_REQUIRE_GPU=1 cargo test --workspace
  --release` all passed (RECEIPTS); `cargo deny check` ok; `cargo build --release` ok; CI green
  (RECEIPTS).
- The rules: 11 clock tests (`crates/glance-playback/src/lib.rs`), `tests/frames.rs` (40 clips / 427
  frames; Swift's 8 timing fixtures 40 of 40, every frame start, loop onto 0, negative control),
  glance-audio 5 unit + 2 fixture tests.
- Measured (`docs/05-C2-RESULTS.md`): 0 skipped at 120 Hz in 30 runs; 60 Hz cadence 26 of 30 runs 0,
  4 first-pass runs 1–3; A/V samples within half a frame on PCM and AAC; no measurable drift over
  19–20 loops on either clock; scrub: latest frame 6,000 of 6,000, request-to-frame = a same-moment
  decode; FLAC: picture on the wall clock with the reason; Intel binary loads, a 10.12 build does not.
- Parity (`docs/02-PARITY.md`): Core "Time" 4 of 6 ticked (the Windows half of audio is C4; 60 Hz on
  a real display not run); Glance "Audio" 5 of 8 ticked, 2 more with their core half proven, the
  speaker icon is UI; Glance "Transport" 4 of 15 ticked, 5 more with their core half proven, the rest
  are keys, labels and window rules (G1 / C3).

## Deviations, and why

- **The loop seam on the audio clock — fixed, differs from Swift on purpose.** Measured: frame 0 was
  skipped once in 20 loops because Swift's clamp shows the last frame for 1.5 frame periods and frame
  0 for 0.5. Now the half frame before the wrap shows the next loop's frame 0 when the sound's loop is
  no longer than the picture. Amrit's rule for Swift quirks is "fix" (brief, Cowork decision); recorded
  in the parity item.
- **End of a non-looping clip on the audio clock**: also stops when a *shorter* sound has ended
  (Swift kept "playing" on a held frame). Part of the same "fix".
- **A/V: the frame is picked for the moment it is shown**, and the sound's position is the heard
  one (Swift: render time, frame for the tick). Correct by construction; DIAGNOSE 2.
- **60 Hz simulated** (Amrit 2026-11-15: "Simulate at 120 Hz"); **Blender open** during the runs (Amrit:
  "Keep Blender open"); Arena quit by Amrit ("I've quit Arena, go").
- **2160p DXV clips made** in the scratch folder with DXVKit 1.0.0's `dxvtool` (none in the test folder);
  no 2160p DXV or 1080p HAP clip with sound exists, so those two cells are not measured.
- Kept out of scope (W2 says "can", the brief does not ask): `alpha_rule` reading `MediaInfo::alpha` for
  native clips (closes the C1 native-alpha parity half), DXVKit Plus's Apple island under `cfg(ios)`,
  generated HAP R movies with reserved BC7 blocks.
- The whole track is decoded into memory at open, as Swift does (sample-accurate seek and loop). A
  long clip costs memory (10 min stereo 48 kHz ≈ 230 MB of `f32`) and open time; not measured beyond
  the test clips.
- If the default output device changes rate while a clip plays (rerouting), the track resampled at
  open plays at the wrong speed until the clip is reopened. Not handled in C2 (named in NEXT).
- Minimum macOS (Cowork decision 10, "14.0 unless a measured need (cpal) says 14.2; decide in C2 from
  the build"): **14.0** — cpal builds and plays with `MACOSX_DEPLOYMENT_TARGET = 14.0`.

## For Amrit (one line each)

1. The 60 Hz no-skip item is not ticked: on the simulated 60 Hz cadence 4 of 30 runs skipped 1–3 frames when the window's presents came late (Blender open, the old build alike). A run on a real 60 Hz display, or with Blender closed, would settle it — your call when.
2. FLAC audio stays "not supported" (as AVFoundation), unless you say otherwise.

## Answers from Amrit in this wave

- 2026-11-15, quit Arena for the measurements: "I've quit Arena, go".
- 2026-11-15, the 60 Hz run: "Simulate at 120 Hz (Recommended)".
- 2026-11-15, Blender open: "Keep Blender open".
