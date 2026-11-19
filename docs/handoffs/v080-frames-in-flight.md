# v0.8.0 — frames in flight (glance-encode), DXVKit Plus v0.15.0, kit x86-64-v3

Brief: glenc-rs's `speed-build-and-fragmented-mp4` pass (2026-11-19), run from the glenc-rs session. Built from the
2026-11-18 speed diagnosis's prototypes (`~/claude-handoffs/glenc-rs_2026-11-18_speed-data/prototype-diffs/`).
Every DXV / HAP output is byte-identical to v0.7.2.

## What changed
- **glance-encode, frames in flight** (`pipeline.rs`). The source is read in file order under one lock
  (`SourceReader::next_pending`; DXV / HAP readers return the read packet with its decode still to do, the native
  reader the decoder's copy with the swap still to do). One worker per core decodes the frame it took, crops, resizes
  and encodes it with its own encoder (`Sink::maker`; DXV / HAP encoders on one thread each, `set_threads(1)`; a job
  with fewer frames than cores gets one worker per frame and the spare cores inside each). The calling thread writes
  strictly in source order, reports progress after each frame, checks cancel before each; an error stops the job at
  its frame with the frames before it written, as v0.7.2's one-at-a-time loop did. Frames taken but not written stay
  below 2 × workers. Frames before a trim window are read but never decoded. The platform's writer gets 4 workers.
- **Native BGRA kept** when the frame goes to the platform's writer unchanged (no crop, original size): v0.7.2 swapped
  BGRA → RGBA on read and back into a copy in the sink. Swaps are in place now; the DXV fill is in place.
- **A fragmented file's exact count**: DXVKit Plus 0.15.0 reads `moof` fragments, so a fragmented native source's
  rate and `total_frames` are its samples' (for every other file `total_frames` stays Swift's `round(duration × fps)`).
  A user's capture whose AVFoundation length is twice its samples' made GlEnc's progress stop near half.
- **DXVKit Plus v0.15.0** (all pins): fragmented MP4; DXV CPU decode 2.5× (no per-row allocation); vectorisable DXV
  BC1 / BC4 searches; `DxvEncoder::set_threads`.
- **Kit**: the Intel slice is built for `x86-64-v3` (every Intel Mac macOS 14 supports has AVX2 / FMA / BMI2; Apple
  support 105113: iMac Pro 2017, MacBook Air 2018 Amber Lake and later). arm64 unchanged (already apple-m1).

## Proof
- Tests: `pipeline::tests` (fake reader / sink, scrambled decode times, 1 / 2 / 5 / 18 workers): source order,
  progress strictly rising to 1, a decode failure and a read failure each stop at their frame, cancel after 3 frames
  writes 3 and reads at most 3 + 2 × workers, a trim window writes its frames only and reads nothing past it.
  `a_fragmented_mp4_converts_at_its_rate` rewritten: the demuxer's 24 frames, progress 1.0 on the last frame.
- Gates (`docs/data/v080/gates.txt`): clippy, test 458 passed / 0 failed (GLANCE_REQUIRE_GPU=1, GLANCE_TESTFILES),
  deny, build; fmt applied once, then clean. CI 44508562393 green (lint, windows, macos).
- glenc-cli 1.7.2 vs 1.8.0 (this core), 6 sources (1080p / 4K × DXV3 DXT5, ProRes 422, H.264; 60 frames) × 11 outputs,
  3 interleaved runs (`docs/data/v080/encode-1.7.2-vs-1.8.0.md`): every one of the 54 DXV / HAP outputs byte-identical;
  1.06–12.4× less wall time; 4K peak RSS ≤ 1.81 GB. ProRes / H.264 files differ on every run of any build; their
  decoded pictures are compared in glenc-rs (docs/data/p3).

## Not built (measured, glenc-rs docs/handoffs/p3-speed-build.md)
PGO (no gain beyond noise on top of frames in flight), band stealing (no bands left: one thread per encoder), the
CVPixelBuffer hand-over to AVAssetWriter.
