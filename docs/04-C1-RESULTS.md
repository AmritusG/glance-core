# C1 results — exact picture

2026-11-14. Release build (`cargo build --release`, rustc 1.99.1), <host> — Apple M5 Max, Metal,
macOS 27.0.1. DXVKit Plus `v0.8.0` (9bc1977), exact block path. Raw data: `docs/data/c1-*.json`;
test data from Swift Glance's own code: `tests/data/swift-glance-*.tsv`. Nobody looked at a
window: every number is a GPU read-back compared in code.

What is compared: the picture the core hands to the window (`glance_gpu::ClipView::show`: DXVKit
Plus's upload and conversion on the exact block path, then the core's Alpha Type pass) read back
from the GPU, against the CPU reference (`glance_gpu::expected_picture`: DXVKit Plus's CPU decode
`Decoded::to_rgba`, read with the same Alpha Type in whole code values). "Max" is the largest
difference of any channel of any texel, in 8-bit code values. A clip whose alpha is the user's
choice (DXV) and an opaque clip are read both ways (Premultiplied = as stored, Straight =
round(colour x alpha / 255)); a HAP clip with alpha is read Straight, as stated.

## 1. At 1:1: fixtures and generated movies (tests, run by `cargo test --workspace --release`)

| What | Rows | Frames compared | Max | Texels differing | Test |
|---|---|---|---|---|---|
| Every variant at every size: DXV3 DXT1 / DXT5 / YCG6 / YG10, legacy DXT1 / DXT5, HAP Hap1 / Hap5 / HapY / HapM / HapA / Hap7, each at 1x1, 2x2, 3x3, 4x4, 5x7, 15x9, 17x33, 1920x1080, 3840x2160, 2 frames of random blocks each | 180 | 360 | **0** | 0 | `every_variant_at_every_size_equals_the_cpu_reference` |
| Every frame of the 36 committed DXV and HAP movies (CC0 tile movies, HAP R, DXVKit's MIT vector streams) | 67 | 563 | **0** | 0 | `every_fixture_frame_equals_the_cpu_reference` |
| DXVKit's own reference: the picture of each vector frame hashes to the SHA-256 DXVKit's C library gives for its RGBA | 64 pictures | 64 | equal | — | `dxvkit_vector_pictures_hash_to_dxvkits_reference_rgba` |
| HAP HDR (no CPU decode): the picture equals DXVKit Plus's own GPU conversion | 2 | 6 (3 frames, read both ways) | **0** | 0 | `hap_hdr_plays_and_matches_dxvkit_plus` |
| H.264, HEVC, Motion JPEG, ProRes 422, ProRes 4444 fixtures: route to the OS decoder, all 24 frames of each shown, the clip's size, some frame with colour | 10 | 240 | no reference | — | `os_codec_fixtures_route_to_the_os_decoder_and_show` |

Negative controls (each assertion above can fail):
- One flipped bit in one block is caught for every variant and Alpha Type (24 cases), and the
  picture follows the flip (`a_flipped_bit_and_the_wrong_alpha_type_are_caught`).
- A clip with alpha read with the other Alpha Type is caught (6 variants).
- The same clips on DXVKit Plus's sampled path are **not** exact: max 2 (DXT5) and 3 (Hap5, HapM)
  on random blocks — the harness tells the paths apart
  (`the_block_path_is_per_view_and_only_the_exact_one_is_exact`).
- A clip wider than the device's largest texture is an error value, not a blank picture
  (`a_clip_larger_than_the_gpu_allows_is_an_error_value`; this machine's limit is 16,384).
- A BC7 block of a reserved mode, which no encoder writes: transparent black on this GPU, as the
  CPU decoder gives (`reserved_bc7_blocks_are_shown_and_their_value_is_the_gpus`). On GitHub's
  virtual GPU it is something else — section 6.

## 2. At 1:1: Amrit's test clips (`glance-proof verify`, every frame)

Folder (read-only, nothing copied): `<testfiles>`,
all 174 movie files in it and its subfolders. Raw: `docs/data/c1-verify-testfiles.json` (one entry
per file, by name). Run time 837 s.

| | Files | Rows | Frame comparisons | Texels compared | Max | Texels differing |
|---|---|---|---|---|---|---|
| DXV and HAP, against the CPU reference | 134 | 225 | 43,475 | 65,222,310,280 | **0** | 0 |
| OS decoder (H.264, HEVC, ProRes): frames shown; no CPU reference | 33 | | 11,378 of 11,393 | | — | — |
| Nothing plays it: the message is shown | 7 | | | | | |

- Rows that are not exact: none.
- Errors: none.
- DXV / HAP frames that did not decode (counted, not compared): 3, all in files made to be damaged: `adv_corrupt_inner.mov`, `adv_corrupt_outer.mov`, `bad-audio-fixtures/bad-audio.mov`.
- OS-decoder frames that did not come through: 15 of the 768 frames of `hap-testclips/std_film_h264_audio.mp4`. They are the last 15 (753–767): the probe reports the container's 768 samples, the OS decoder serves 753 frames and DXVKit Plus's native source refuses the rest as out of range. What Swift Glance shows there was not examined; frame counts and timing on the OS-codec path are core wave C2 and DXVKit Plus's container-timing wave.
- Frames without any colour (R, G and B zero in every texel; a clip may hold such frames): 2 among the DXV / HAP frames shown, 0 among the OS-decoder frames shown.
- OS-decoder clips with no frame that has colour: none.
- What the 134 DXV and HAP files are: DXV Normal Quality With Alpha x35; HAP Q Alpha x21; HAP Alpha x21; DXV High Quality With Alpha x16; DXV Normal Quality x12; DXV High Quality x12; HAP Q x10; HAP x6; HAP Alpha-only x1.
- Their sizes: 1920x1080 x59, 512x288 x18, 1280x720 x15, 512x512 x9, 1080x720 x4, 1080x1080 x4, 540x960 x4, 3840x2160 x3, 333x221 x3, 996x288 x2, 1284x724 x2, 1280x724 x2, 1284x720 x2, 640x360 x1, 800x800 x1, 1336x852 x1, 1024x1024 x1, 1500x844 x1, 2048x2048 x1, 999x666 x1.
- The clips of the W0 proof whose picture W0 checked (W0: the frame on screen at the end of a run, sampled path; now: every frame, exact path): `Metric_01_HapM.mov` W0 max 2, now max 0 over 201 frames; `DXT5-DXV3.mov` W0 max 2, now max 0 over 300 frames; `glenc_hap1_1920x1080_chunks4.mov` W0 max 1, now max 0 over 90 frames; `hap-testclips/std_film_h264_audio_DXV High Quality With Alpha [00.00.00-00.10.00].mov` W0 max 0, now max 0 over 251 frames; `glenc_hapm_LEAKTEST_4k.mov` W0 max 1, now max 0 over 90 frames; `DXT1-DXV.mov` W0 max 1, now max 0 over 300 frames; `YCG6-DXV3_DXV Normal Quality With Alpha.mov` W0 max 1, now max 0 over 300 frames.
- Files nothing plays, with the line shown: `YCG6-DXV3_Hap5.mov` — "MOV files are not supported"; `adv_garbage.mov` — "MOV files are not supported"; `adv_trunc_hapm_50pct.mov` — "MOV files are not supported"; `adv_trunc_hapm_midframe.mov` — "HAP Q Alpha in MOV is not supported"; `adv_trunc_hapm_no_moov.mov` — "MOV files are not supported"; `adv_zero.mov` — "MOV files are not supported"; `bad-audio-fixtures/bad-audio2.mov` — "MOV files are not supported".

## 3. Window-scaled: 1.5x and 0.5x against a software bilinear model

The real widget (`glance_ui::video`) drawn offscreen by the same egui renderer a window uses
(`glance_ui::headless::Stage`), read back, and compared with `glance_ui::headless::on_screen`:
the clip's premultiplied picture sampled bilinearly at each screen pixel's centre (texel centres
at half integers, edges clamped, the sample position kept to 1/256 of a texel) and blended over
the background. Distance = |screen - rounded model| over R, G, B.

| What | Screens | Largest distance | Pixels more than 1 away |
|---|---|---|---|
| Every variant, random blocks (the hardest content for a filter), 17x33 / 64x36 / 100x36, each Alpha Type, both scales; 5 readings of 4 fixture clips (DXV DXT5 both ways, YG10, HapM, Hap1) over black and over the checkerboard at 1x and 2x display scale; 1920x1080 and 3840x2160 over the checkerboard at 2x (`window_scaled_screens_are_within_1_of_the_bilinear_model`) | 134 | **1** | 0 |
| The middle frame of each of Amrit's DXV and HAP clips, each Alpha Type, both scales, over black (`glance-proof verify`) | 450 | **1** | 0 |

- At 1:1 the screen **is** the picture: 60 screens of every variant at 1x and 2x display scale,
  R, G and B equal over black (`at_one_to_one_the_screen_is_the_exact_picture`). The window adds nothing.
- Negative control: the same screen against the model of a picture 1.5 pixels to the side is more
  than 1 away in over 100 pixels.
- Checkerboard: 8-point squares (8 pixels at 1x, 16 at 2x), values 217 and 166 (0.85 and 0.65 of
  255), only under the picture, black bars beside it; an opaque clip is identical over both
  backgrounds (`the_checkerboard_is_8_point_squares_only_under_the_picture`).
- Where this differs from Swift Glance: the Rust picture is premultiplied first and filtered
  after (egui samples the finished picture); Swift filters the stored texels and premultiplies
  after, and keeps DXV High Quality chroma nearest. Identical for opaque and premultiplied clips
  and for every clip at 1:1. Not measured against Swift's renderer.

## 4. Routing, the message, the clip key and the Alpha Type hint against Swift Glance's own code

A small Swift program built on Swift Glance's `GlanceCore` (`tools/swift-crosstest`, against the
read-only clone of glance @ a9dc601, which pins DXVKit 1.0.0; rebuilt from the repo it regenerates
the three committed tables byte for byte) printed, per file: Swift's route, the line it would show, the first video track's FourCC and
name, `AlphaTypeProbe.clipKey`, and `AlphaTypeProbe.probeDXV`'s samples / exceeding / hint. The
Rust code is compared with that table in `crates/glance-media/tests/swift_parity.rs`.

| Files | Compared | Route | Message | FourCC + name | Clip keys equal | Hints equal (samples, exceeding, label) |
|---|---|---|---|---|---|---|
| The 52 committed movies | all | 50 / 52 — the two that differ are HAP R and HAP HDR, played on purpose | 50 / 50 | FourCC 52 / 52, name 50 / 50 | 35 / 35 (and none where Swift has none: 17) | 12 / 12 |
| Amrit's 174 test files | all | 173 / 174 | 174 / 174 | FourCC 174 / 174, name 174 / 174 | 75 / 75 (and none where Swift has none: 99) | 51 / 51 |

- Routes in Amrit's folder, by Swift: 75 DXV, 60 HAP, 33 AVFoundation, 6 nothing. The Rust core
  agrees on 173.
- **The one difference that is not on purpose:** `adv_trunc_hapm_midframe.mov`, a HAP Q Alpha movie cut off in the middle
  of its sample data. Swift Glance's HAP demuxer does not check that samples lie inside the file,
  opens it, and the frames past the cut fail one by one; DXVKit Plus's demuxer refuses a track with
  a sample outside the file, so the Rust core shows "HAP Q Alpha in MOV is not supported".
- The two fixture movies whose route differs on purpose, and whose name and message are not
  compared, are HAP R and HAP HDR: Swift shows "Hap7 in MOV is not supported" and "HapH in MOV is
  not supported"; the Rust family plays them. So over all 226 files the routes differ in three:
  these two and the truncated HAP file.
- Hints in Amrit's folder: 34 "can't tell", 14 "looks premultiplied", 3 "looks straight" — the same
  counts and labels from both.
- The key's layout is recomputed from the bytes in `the_clip_key_is_the_documented_bytes`
  (controls: a renamed copy keeps its key; one changed byte in the last packet changes it; the
  middle packet is not part of it; a one-frame clip hashes its packet twice).
- Hostile files (`hostile_files_give_a_message_never_a_crash_or_a_hang`): 2,930 cases — 2,725
  damaged copies of six fixture movies (253 truncations; 400 overwrites each in the sample tables
  and in the frames; the sample entry renamed to 12 other codecs each), 200 garbage files, one of
  zero bytes and 4 with bad atom sizes. 1,082 were refused with a message, the others opened and
  decoded or failed frame by frame; no crash, no hang (time limit 300 s; the test takes about 1 s).
  Amrit's own `adv_*` files: all come back (`amrits_test_files_come_back`).
- Known limits of DXVKit Plus's probe, pinned or stated and briefed to DXVKit Plus: a movie with a
  DXV track and a HAP track plays the HAP track (Swift: the DXV track); a movie named `.png` or
  `.lmbd` is judged by its name. *(C2, 2026-11-15: both fixed by DXVKit Plus 0.9.0 and no longer
  true; `docs/handoffs/c2-time-and-sound.md`.)*

## 4b. The proof window on the exact path (a smoke run, not a timing claim)

`./target/release/glance-proof Metric_01_HapM.mov DXT5-DXV3.mov --seconds 5` (the W0 gate pair,
1 s warm-up, 120 Hz display; `docs/data/c1-window-smoke.json`): 595 presents in 5 s, present
interval p50 8.33 ms; HAP Q Alpha 126 frames shown, 0 skipped; DXV3 Normal Quality With Alpha
151 shown, 0 skipped; **picture against the CPU reference max 0** for both, on the frame on screen when the run ended
(frames 149 and 179; W0, sampled path: 2).
A harness run was using the same GPU at the time, so the timing is only a sign that the exact
path keeps up, not a measurement. The window is on the exact path because `ClipView::new` is.


## 5. HAP Q: the Rust family's picture against Swift Glance's convention

Decision (Cowork, 2026-11-14): the Rust family follows DXVKit Plus = the Vidvox reference
(untruncated BC3 palette, chroma offset 128/255). Swift Glance truncates the palette to 8 bits and
centres chroma at 127.5. `glance-proof hapq` computes both pictures of each frame on the CPU from
the same blocks and compares R, G, B per texel. Its Swift side is Swift's decoder: all 72 frames
of the three committed HAP Q fixtures hash to Swift Glance's own CPU decode
(`the_swift_convention_here_is_swift_glances_decoder`), and frame 0 of four of Amrit's clips
hashes the same from Swift's decoder (`tests/data/swift-glance-hapq-testfiles.tsv`) and from this
port (`swift_frame0_sha256` in the raw data): `Metric_01_HapY.mov`, `Metric_01_HapM.mov`,
`glenc_hapm_LEAKTEST_4k.mov`, `glenc_hapy_fromMaster.mov`.

Every frame of Amrit's 31 HapY / HapM files (raw: `docs/data/c1-hapq-testfiles.json`):

| Frames | Texels | Max difference | Texels differing | by 1 | by 2 | by 3 or more |
|---|---|---|---|---|---|---|
| 4,560 | 5,193,385,216 | **2** | **43.42 %** | 43.26 % | 0.16 % | 0 |

- Per file the share runs from 7.1 % to 99.3 %; on `Metric_01_HapM.mov` (the W0 gate clip) it is 57.25 %.
- Direction, over the channels of differing texels: the Rust picture is the brighter one in 1,318,814,219, the darker one in 1,463,318,732.
- Left out: 92 frames of the deliberately damaged files (`adv_corrupt_inner.mov`, `adv_corrupt_outer.mov`, `adv_stsd_hapm_is_hap5.mov`) that do not decode or are not HAP Q inside.
- Against DXVKit Plus's own CPU decode the Rust picture is exact (sections 1 and 2); the numbers
  here are the size of the deliberate difference from the Swift apps.

## 6. CI (GitHub Actions) and the other targets

Workflow `.github/workflows/ci.yml`, on every push; GitHub-hosted runners (the repository is
private, so the runs draw on the account's included minutes and are billed per minute after that;
a superseded run is cancelled).

| Job | Runner | What it runs | Result |
|---|---|---|---|
| `lint` | `ubuntu-latest` | `cargo fmt --all --check`, `cargo deny check` | ok |
| `macos` | `macos-latest` (Apple silicon) | `cargo clippy --workspace --all-targets --release -- -D warnings`, `cargo build --release`, `cargo test --workspace --release`, `cargo check --workspace` for `x86_64-apple-darwin` and for `aarch64-apple-ios` | ok |
| `windows` | `windows-latest` | `cargo check --workspace --all-targets --target x86_64-pc-windows-msvc` | ok |

- **The macOS runner has a GPU for wgpu**, which GitHub's runner reference does not say:
  `glance-proof gpu` there prints adapter "Apple Paravirtual device", Metal, BC textures yes,
  largest texture 16,384. So the GPU tests **run in CI** — the size sweep, the fixtures, DXVKit's
  reference hashes, the scaled screens, the checkerboard — and pass: the exact block path and the
  integer present pass are max 0 on a second Metal implementation, not only on the M5 Max.
- **One thing that GPU does differently: reserved-mode BC7 blocks.** On the first run (6b80bf1)
  the generated HAP R movies still held such blocks and two tests of the first GPU test binary
  failed on them: in the size sweep 4 of 180 rows, all HAP R (3x3, 17x33, 1920x1080, 3840x2160),
  max 255 on about 2.3 % of their texels — the blocks whose first byte is zero — while the other
  176 rows were max 0; and the flipped-bit control on HAP R. cargo stopped at that binary, so the
  later test binaries did not run in that run. A reserved block is transparent black by the BC7
  definition, on the CPU decoder and on the M5 Max (`[0, 0, 0, 0]`); the runner's GPU returns
  opaque black (`[0, 0, 0, 255]`). No encoder writes such blocks. The generated movies now hold
  valid BC7 blocks only, and `reserved_bc7_blocks_are_shown_and_their_value_is_the_gpus` records
  each GPU's answer (a CI step prints it). Reported to DXVKit Plus.
- **What does not run in CI:** the two tests on Amrit's media and `glance-proof verify` over his
  folder (the folder is on no runner: those tests skip there and are a local gate); anything on a
  real Windows machine (the Windows job compiles, it runs nothing).
- Runs: 43637568972 (6b80bf1) lint ok, windows ok, macos failed as described; 43638144973
  (2069c2b), 43638498291 (ba20a42) and 43639819736 (f4bb509): all three jobs green, 32 tests each.
  The runs of the later commits, up to the tagged one, are in the handoff.
- The cross targets are proven by CI (rustc 1.100.0). On the Mac `x86_64-apple-darwin` and
  `aarch64-apple-ios` were also checked by hand with the same toolchain (no log kept);
  `x86_64-pc-windows-msvc` failed there in libzstd's C build for want of the MSVC headers, which
  is why it has its own runner.

## 7. Gates on the final tree

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`
(rustc 1.99.1 and 1.100.0), `cargo test --workspace --release` (32 tests), `cargo deny check`,
`cargo build --release`: all clean. The command output is in the handoff's receipts
(`docs/handoffs/c1-exact-picture.md`).

## 8. What these numbers do not say

- Direct3D 12: nothing here ran on Windows. The bar on D3D12 is proven at the Windows milestone.
- Time: no claim about frame pacing in C1 (W0's numbers stand, measured on the sampled path; the
  exact path costs more per frame — DXVKit Plus measured 50–93 µs against 30–48 µs per 1080p layer).
- The OS-decoder path has no CPU reference here: its frames are shown, at the clip's size, and
  counted when they hold no colour; whether they are the right picture is not checked. One clip
  came 15 frames short (section 2).
- The scaled filter is checked against the core's own model, not against Swift Glance's renderer.
- Nobody has looked at the picture with their eyes.
