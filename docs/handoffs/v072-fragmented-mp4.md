# v0.7.2 — a fragmented MP4 converts again (glance-encode)

Brief: glenc-rs's `train-loop-and-speed` pass (r2), PART A, run from the glenc-rs session (2026-11-18).

## What was wrong

A fragmented MP4 (the samples in `moof` / `trun` fragments, the `moov`'s sample tables empty: a DASH or streaming
capture) did not convert in GlEnc 1.7.0 / 1.7.1: "EncodePipeline: source reports an invalid frame rate (0.0 fps)".
Swift GlEnc 1.6.2 and 1.3.0 convert it.

Cause (read, then reproduced): `glance-encode` `platform::open_native` took the rate only from the core's demuxer
(`Movie::open(path).ok()`, `map_or(0.0, …)`). DXVKit Plus's demuxer does not read fragments by design (its
`container.rs` header: the track is damaged and the file goes to the platform backend), so the rate became 0.0 and
`validate_geometry` refused it. Swift's `AVAssetReaderSourceReader` (`SourceFrameReader.swift:181-245`) takes
AVFoundation's timing for every native source, so it never needed the tables.

## The fix

`source_rate`: the container's FrameTiming rate as before; when the demuxer does not read the file, the platform
decoder's timing (DXVKit Plus `NativeReader`: Swift's rate rules over AVFoundation's own minimum frame duration,
nominal rate and duration), the same timing every app's playback already uses for such files. Files the demuxer reads
take the same path as in v0.7.1 (unchanged bytes).

Test: `crates/glance-encode/tests/encode.rs` `a_fragmented_mp4_converts_at_its_rate` on the new CC0 fixture
`tests/fixtures/tiles_h264_fragmented.mp4` (`tiles_h264.mp4` remuxed, no re-encode): DXT1, Hap1 and H.264, 24 of 24
frames at 30/1. With v0.7.1's `platform.rs` it fails ("frag-dxt1.mov: EncodePipeline: source reports an invalid frame
rate (0.0 fps)"); with the fix it passes. Gates: 453 tests, 0 failed (GLANCE_REQUIRE_GPU=1, GLANCE_TESTFILES set).

## Left open

- AVFoundation reports that user file's track as twice its length (timeRange 7397.6 s for 110,964 samples at 30 fps =
  3698.8 s), so the platform timing's grid counts 221,928 frames: GlEnc's progress (Swift's `round(duration × fps)`,
  equal to Swift) and every app's playback count are doubled for it; frames past 110,963 are "not delivered (end of
  track)". Swift Glance counted 110,964 there (its FrameTiming walks AVFoundation's samples). A demuxer that reads
  `moof` fragments (DXVKit Plus) would give the exact table on every path. Not done here.
