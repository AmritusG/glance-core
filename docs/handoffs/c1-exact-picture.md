project:    glance-core
pass:       c1-exact-picture
STATUS:     COMPLETE
parent:     6063231
result:     6b80bf1 0ce9e89 2069c2b ba20a42 f4bb509 fda39bd + the handoff commit; tag v0.1.0
session:    34988e75-6369-4148-8f97-a6c4394c1382
written_at: 2026-11-14T21:30Z

# glance-core C1 — exact picture: result

Brief: `~/claude-handoffs/glance-core_2026-11-14_c1-exact-picture_brief.md`. Numbers and raw data: `docs/04-C1-RESULTS.md`, `docs/data/c1-*`. Release build, <host> (Apple M5 Max, Metal, macOS 27.0.1), DXVKit Plus v0.8.0.

## What shipped

1. **The picture is exact.** DXVKit Plus re-pinned to tag `v0.8.0` (9bc1977); `glance_gpu::ClipView` renders on `BlockPath::Exact` by default and a view can be put on `Sampled` (`with_path`, `set_path`) for a later many-layer grid. The core's own last step, the Alpha Type pass, now works in whole code values (Straight = round(colour x alpha / 255)), so it has one answer on every backend; an alpha-only HAP frame is shown as white at its alpha, as Swift Glance shows it. No HAP Q hash was pinned in this repo from 0.7.1, so none had to be re-derived.
2. **The harness.** `glance_gpu::verify` runs a clip through the core's own pipeline (route, open, decode, show, read back) and compares every frame with the CPU reference for each Alpha Type the clip can have. Tests run it over the committed fixtures and over movies the tests write themselves (every variant, nine sizes); `glance-proof verify` runs it over any files without a window. `glance_ui::headless` draws the real egui widget offscreen, which is how the scaled filter and the checkerboard are tested.
3. **Routing and the message.** `glance_media::route` routes by content and returns Swift Glance's wording for a file nothing plays; `Clip::open` gives "This DXV / HAP file could not be opened". Hap7, HapH and legacy DXV route and play.
4. **`clip_key` and the Alpha Type hint**, identical to Swift Glance's on every file compared.
5. **Amrit's decisions of 2026-11-14 applied**: licence MIT (`LICENSE`, `license = "MIT"`, README, `ATTRIBUTION.md`; the repo stays private), the parity checklists, the roadmap and research §9 (16 decisions closed with his words) — `git diff 6063231 -- docs/`.
6. **CI** on GitHub Actions and `cargo deny` as a gate.
7. One defect found by running Amrit's folder and fixed here: a clip taller than 8,192 pixels (`SCANTEST/scantest4.mp4`, 3840x9626) hit wgpu's default texture limit: the first harness run logged wgpu validation errors for every frame of it (that log was not kept; the regression test is `a_clip_larger_than_the_gpu_allows_is_an_error_value`). The family's device now asks for the adapter's limit (16,384 here) and a clip beyond the limit is an error value.

Fixtures (1.2 MB: DXVKit Plus's CC0 tile movies, its synthetic HAP R / HAP HDR movies, DXVKit's MIT vector streams) were copied unchanged with `cp -p` into `tests/fixtures/`.

## DIAGNOSE (a source per fact; paths relative to `.ref/`)

### 1. Routing and the message in Swift Glance
- Order: `let dxv = try? DXVDemuxer.demux(url: url)`; `let hap = dxv == nil ? (try? HAPDemuxer.demux(url: url)) : nil`; DXV, else HAP, else `routeToAV` (`glance/Sources/Glance/PlayerModel.swift:209-229`).
- DXV = the first `trak` whose first sample entry is `DXD3` / `DXD4` / `DXDI` (`dxvkit/src/mov.c:149-161, 351-369`), accepted only if sample 0 reads and states a variant (`glance/Sources/GlanceCore/DXVKitBridge.swift:220-224`). `DXD4` is a DXV3 entry; the packet decides the generation, not the FourCC (`DXVKitBridge.swift:193-195`). `DXT1`, `YCG6`, `DXV3` as sample entries are not DXV.
- HAP = the first **video** track, and only `Hap1` / `Hap5` / `HapY` / `HapM` / `HapA`; any other codec in that track throws (`glance/Sources/GlanceCore/HAPDemuxer.swift`, `parseTrakIfVideo`, the `switch codec`). So in Swift, `Hap7` and `HapH` fall to AVFoundation and show "Hap7 in MOV is not supported" / "HapH in MOV is not supported" (the lines Swift's code gives for the two fixtures, `tests/data/swift-glance-fixtures.tsv`).
- Everything else: AVFoundation. Not playable means: no video track, or not `isPlayable`, or not `isDecodable`, or `FrameTiming.load` throws (`glance/Sources/Glance/AVPlaybackController.swift:92-104`).
- The wording (`AVPlaybackController.swift:85-91`): container = the path extension in upper case, or `this container` when there is none; codec = `DXVDetector.compressorFourCC` mapped through `DXVDetector.displayName` (else the FourCC with spaces trimmed), else the Matroska `V_...` codec id (`:110-123`). Message `"\(codec) in \(container) is not supported"`, or `"\(container) files are not supported"` when no codec can be named. A demuxed DXV / HAP whose player does not start: `"This DXV file could not be opened"` / `"This HAP file could not be opened"` (`PlayerModel.swift:334, 420`).
- Names (`glance/Sources/GlanceCore/DXVDetector.swift:28-57`): DXD3 / DXD4 "DXV3", DXDI "DXV1", `rle ` "QuickTime Animation", Hap1 "HAP", Hap5 "HAP Alpha", HapY "HAP Q", HapM "HAP Q Alpha", HapA "HAP Alpha-only", jpeg / mjpa / mjpb / mjpg "Motion JPEG", apch / apcn / apcs / apco "ProRes 422 HQ / 422 / 422 LT / 422 Proxy", ap4h / ap4x "ProRes 4444 / 4444 XQ", avc1 / avc3 "H.264", hvc1 / hev1 "HEVC", av01 "AV1", vp09 "VP9", mp4v "MPEG-4".
- What each entry means in the Rust core: DXD3 / DXD4 / DXDI -> `Route::Dxv`; Hap1 / Hap5 / HapY / HapM / HapA -> `Route::Hap`; Hap7 / HapH -> `Route::Hap` (added on purpose); everything else -> `Route::Native` if the OS decoder opens it, else the message.

### 2. `clipKey` and the alpha-look probe (`glance/Sources/GlanceCore/AlphaType.swift`)
- `clipKey` (`:178-187`):
  ```swift
  guard let movie = try? DXVMovie(url: url), movie.frameCount > 0,
        let first = try? movie.packet(at: 0), let last = try? movie.packet(at: movie.frameCount - 1) else { return nil }
  let bytes = (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? NSNumber)?.int64Value ?? 0
  var h = SHA256()
  h.update(data: Data("\(bytes) \(movie.frameCount) \(movie.width)x\(movie.height)".utf8))
  h.update(data: first)
  h.update(data: last)
  return h.finalize().map { String(format: "%02x", $0) }.joined()
  ```
  So the SHA-256 input is: the UTF-8 text `<file size> <frame count> <width>x<height>` in decimal (no integer is hashed in binary, so there is no width or endianness to match), then the whole first packet, then the whole last packet (the same packet twice for a one-frame clip); output lower-case hex. "Packet" is the complete QuickTime sample, header included, read at the sample-table offset and size (`dxvkit/swift/Sources/DXVKit/DXVKit.swift:261-266`, `dxvkit/src/mov.c:476-492`). Width and height are the sample entry's 16-bit fields (`mov.c:190-191`). `DXVMovie.init` fails, and the key is nil, when the first packet does not state a variant (`DXVKit.swift:247-253`). Nil for non-DXV files.
- Probe (`:138-153`): only DXV clips whose first packet's variant has alpha; frames `Set([0, movie.frameCount / 2]).sorted()`; each decoded to DXVKit's RGBA (as stored); counts summed over the two frames. Count (`:74-87`): every 7th pixel in reading order (`i += 7` over `pixelCount`); a sample when `a > 8 && a < 200`; exceeding when `max(R, G, B) > a + 24`. Classify (`:89-95`): fewer than 500 samples "can't tell"; share <= 0.005 "looks premultiplied"; share >= 0.03 "looks straight"; else "can't tell". A decode error makes the whole report "no alpha" (`:126`, `(try? probeDXV(url)) ?? AlphaTypeReport(kind: .none)`). HAP Hap5 / HapM / HapA: stated straight, no hint (`:128-132`).

### 3. The exactness bar as Swift Glance states and tests it
- Statement (`glance/CLAUDE.md:472`): rendered frames equal DXVKit's CPU RGBA (`DXVMovie.decodeRGBA`, "raw texels, no premultiply"), "GL offscreen at 1:1, blend off", max 0 per channel, RGB and alpha, every variant, every size 1x1 ... 3840x2160; window-scaled (1.5x / 0.5x) within 1 of a software model; HAP since 0.17.2 against GlanceCore's CPU HAP decoder.
- Test (`glance/Tests/GlancePlaybackTests/DXVRendererExactTests.swift`): the renderer at its default Alpha Type (Premultiplied = texels out as stored), black background, into an RGBA8 framebuffer the size of the clip, compared with `DXVMovie(url:).decodeRGBA(at: 0)`, `XCTAssertEqual(worst, 0)`.
- **Where the Alpha Type pass sits:** the 0 bar is on the stored texel, which is what Premultiplied shows; nothing is composited on either side. Straight is `rgb *= a` in the shader after the texel is decoded (`glance/Sources/GlancePlayback/DXVRenderer.swift:834-838`); Swift tests it against a model within 1 (`AlphaTypeRenderTests`: `XCTAssertLessThanOrEqual(sw, 1)`), and for HapM against the CPU decode premultiplied with `(c * a + 127) / 255` at max 0 (`HapYRendererExactTests.swift`). **The checkerboard** is a separate pass drawn before the frame and blended `ONE, ONE_MINUS_SRC_ALPHA`; it is outside the 0 comparison and tested within 1.
- So in the Rust core the bar applies as: read as Premultiplied, picture = CPU decode, max 0; read as Straight, picture = CPU decode with `(c * a + 127) / 255`, max 0 (stricter than Swift's 1, possible because the pass is integer); the composite over the checkerboard within 1.
- Scaled rule: Swift's model is "bilinear over the display texels, texel centres clamped, 8-bit filter weights" (`glance/tools/dxv-render-bar/Sources/DXVRenderBar/main.swift:287-308`), and its shader filters the decoded texels first and premultiplies after; DXV High Quality keeps chroma nearest (`DXVRenderer.swift:895-908`).

### 4. DXVKit Plus v0.8.0
- `BlockPath` (`DXVKit-Plus/crates/dxvkit-plus-wgpu/src/convert.rs:21`): `Sampled = 0` (default), `Exact = 1`; `FrameRenderer::with_path` (`:639`), `path` (`:660`), `set_path` (`:666`, drops the pending frame). Contract of `Exact` (`:30-33`): "the output equals `Decoded::to_rgba`, every channel of every texel"; BC1 / BC3 frames have no `direct_view` on it. BC7, DXV High Quality, LMBX and RGBA8 take the same route on both paths; HAP HDR has no CPU oracle (`:16-18`).
- `crates/dxvkit-plus-wgpu/tests/exact.rs`: the size sweep (9 variants x 9 sizes, random blocks with forced boundary cases), the fixture run (63 rows, 488 frames), the flipped-bit control. The core's harness follows that shape and adds what DXVKit Plus's does not have: YCG6, YG10 and HAP R in the sweep, the Alpha Type pass, DXVKit's reference hashes, the scaled screen, Amrit's whole folder.
- `dxvkit-plus verify --block-path exact <files>`: the per-frame comparison on DXVKit Plus's own renderer (`crates/dxvkit-plus-cli/src/gpu.rs:38`).
- HAP Q moved by 1 on some texels of the CPU decode between 0.7.1 and 0.8.0 (exact integers, halves up); nothing in this repo depended on the old values.

### 5. GitHub-hosted runners
- GitHub's runner reference lists the standard macOS runners as arm64 "3 CPU (M1), 7 GB RAM" (`macos-latest`, `macos-15`, `macos-26`) and Intel `macos-15-intel` / `macos-26-intel`; arm64 limits: "Nested-virtualization is not supported due to the limitation of Apple's Virtualization Framework". It says nothing about Metal or a GPU on the standard macOS runners (docs.github.com/en/actions/reference/runners/github-hosted-runners, read 2026-11-14). For a private repository the standard runners draw on the account's included minutes and are charged per minute after that (same page).
- So it was measured instead of assumed: the workflow prints the runner's adapter (`glance-proof gpu`) and the GPU tests run when there is one. Measured: the standard macOS runner gives wgpu a Metal adapter, "Apple Paravirtual device" (BC textures, largest texture 16,384). So the GPU tests run in CI and pass there; only the two tests on Amrit's media skip on a runner (local gate). That GPU decodes reserved-mode BC7 blocks as opaque black where the CPU decoder and the M5 Max give transparent black (`docs/04-C1-RESULTS.md` §6).
- Cross targets: `cargo check --workspace` passes for `x86_64-apple-darwin` and `aarch64-apple-ios` in CI (rustc 1.100.0; also run by hand on the Mac, no log kept). DXVKit Plus's Apple island is a dependency under `cfg(target_os = "macos")` only, so it is compiled for the Intel Mac target and not for iOS (no OS decoder on iOS yet; DXV and HAP need none). `x86_64-pc-windows-msvc` is checked on a Windows runner (on the Mac libzstd's C build stops for want of the MSVC headers).

### 6. HAP Q: the two conventions, measured
- Swift Glance (`glance/Sources/GlanceCore/HAPHQDecoder.swift:339-343`): `d = B + 8`, `R' = Y + 8 (R - G) / d`, `G' = Y + 4 (2G - 255) / d`, `B' = Y - 8 (R + G - 255) / d` on the BC3 palette truncated to 8 bits, rounded half up: midpoint 127.5. DXVKit Plus (`DXVKit-Plus/crates/dxvkit-plus-core/src/hap.rs:303-336`): the untruncated palette (thirds and 35ths), offset 128/255, the Vidvox reference.
- `glance-proof hapq` computes Swift's convention from the same blocks (`crates/glance-proof/src/hapq.rs`). That port is Swift's decoder: on the three committed HAP Q fixtures all 72 frames hash to the SHA-256 of Swift Glance's own CPU decode (`HAPThumbnail.rgbaOfFrame`), and so does frame 0 of four of Amrit's clips (`tests/data/swift-glance-hapq-testfiles.tsv` against `swift_frame0_sha256` in the raw data).
- Result on every frame of Amrit's 31 HapY / HapM files: 4,560 frames, 5,193,385,216 texels; **max 2**; **43.42 %** of texels differ — 43.26 % by 1, 0.16 % by 2, none by more; per file 7.1 % to 99.3 %. 92 frames of three deliberately damaged files were left out. Raw: `docs/data/c1-hapq-testfiles.json`.

## Validated (the number or the test per claim)
All on Metal, Apple M5 Max, release build; details and raw data in `docs/04-C1-RESULTS.md`.
- **1:1, every variant and size:** 12 variants (DXV3 DXT1 / DXT5 / YCG6 / YG10, legacy DXT1 / DXT5, Hap1 / Hap5 / HapY / HapM / HapA / Hap7) x 9 sizes (1x1 ... 3840x2160) x each Alpha Type = 180 rows, 360 frames: **max 0** (`every_variant_at_every_size_equals_the_cpu_reference`).
- **1:1, committed fixtures:** 36 movies, 67 rows, 563 frame comparisons: **max 0**; 64 DXVKit vector pictures hash to DXVKit's own reference RGBA; HAP HDR equals DXVKit Plus's GPU conversion (3 frames, read both ways).
- **1:1, Amrit's clips:** all 174 movie files of `<testfiles>` (read-only), every frame: 134 DXV / HAP clips, 225 rows, 43,475 frame comparisons, 65,222,310,280 texels: **max 0, 0 texels differing**; 33 OS-codec clips: 11,378 of their 11,393 frames shown (no CPU reference; 15 frames of one H.264 clip did not come through the OS decoder — see Deviations); 7 files refused with the message. `docs/data/c1-verify-testfiles.json`.
- **Negative controls:** a flipped bit (24 cases), the wrong Alpha Type (6 variants), the sampled path (max 2-3, not exact), a clip larger than the device limit (error value), a shifted scaled model (fails) — each makes its assertion fail.
- **Window-scaled 1.5x / 0.5x:** 134 screens in the test + 450 screens of Amrit's clips: largest distance from the bilinear model **1**, none beyond. At 1:1 the screen is the picture exactly (60 screens).
- **Checkerboard:** 8-point squares, 217 / 166, at 1x and 2x, only under the picture.
- **Routing, message, FourCC, `clip_key`, hint vs Swift Glance's own code:** 52 fixtures: FourCC 52 / 52, keys 35 / 35, hints 12 / 12, names and messages 50 / 50, routes 50 / 52 (HAP R and HAP HDR play on purpose). Amrit's 174 files: FourCC, names and messages 174 / 174, keys 75 / 75, hints 51 / 51, routes 173 / 174.
- **Hostile files:** 2,930 damaged or garbage files (2,725 damaged copies of six fixtures, 205 garbage / empty / bad-size files) + Amrit's `adv_*`: every one comes back as a clip or a message; no crash, no hang.
- **HAP Q vs Swift's convention:** max 2; 43.42 % of 5.19 billion texels differ (43.26 % by 1, 0.16 % by 2).
- **The proof window on the exact path** (5 s smoke run, the W0 gate pair): 0 frames skipped; the frame on screen at the end vs CPU max 0 (W0: 2).
- **Gates:** fmt, clippy `-D warnings` (rustc 1.99.1 and 1.100.0), 32 tests, `cargo deny`, release build: clean. CI green (lint, macos incl. the GPU tests and the x86_64-apple-darwin / aarch64-apple-ios checks, windows check).
- **Parity:** Core "Decode and render" 9 of 10 items ticked on Metal (open: the D3D12 half of "every backend", C4); "Alpha and background" 5 of 7 (open: native codecs' stated alpha mode — DXVKit Plus API gap; "writers write straight" — no writer until the GlEnc wave). "Time" and "Settings, identity, release" are not C1.

## Deviations, and why
- **Routes differ from Swift Glance in 3 of 226 files.** Two on purpose (the HAP R and HAP HDR fixtures play). One not: `adv_trunc_hapm_midframe.mov` (HAP Q Alpha cut off mid-data). Swift opens it and fails frame by frame; the Rust family shows "HAP Q Alpha in MOV is not supported", because DXVKit Plus's demuxer refuses a track with a sample outside the file. Pinned in the test; briefed to DXVKit Plus.
- **Two limits of DXVKit Plus's probe**, pinned or stated, briefed: a movie with a DXV and a HAP track plays the HAP track (Swift: DXV); a movie named `.png` or `.lmbd` is judged by that name. *(C2, 2026-11-15: all three fixed by DXVKit Plus 0.9.0 and no longer true — the DXV track plays, content comes before the name, and the cut-off HAP movie plays its whole samples; `docs/handoffs/c2-time-and-sound.md`.)*
- **The scaled filter** is bilinear over the finished, premultiplied picture (egui samples the core's picture). Swift filters first and premultiplies after, and keeps High Quality chroma nearest. Same picture for opaque and premultiplied clips and for every clip at 1:1; at other scales straight-alpha edges and HQ chroma differ slightly. Filtering premultiplied colour is the correct order for compositing; left as is and written into the parity item.
- **A `FourCC` reader in glance-media** (`first_video_fourcc`, atom headers only), needed to name the codec of damaged files for the message. Not a decoder or a demuxer; offered to DXVKit Plus so one walker remains.
- **HAP Q differs from Swift Glance on purpose** (the brief's decision), measured above.
- **Windows check** runs on a GitHub Windows runner only (cannot run on the Mac).
- **Amrit's media in the tests:** two tests and `glance-proof verify` read his folder when it exists and skip otherwise; only file names and numbers are committed (`tests/data/swift-glance-testfiles.tsv`, `docs/data/c1-*.json`), no media.
- **The Swift cross-test program is committed** (`tools/swift-crosstest`, 120 lines; the brief allowed leaving it out). The verifier could not prove where the tables came from without it. Rebuilt from the repo against `.ref/glance` (a9dc601, DXVKit 1.0.0) it regenerates the three committed tables byte for byte. It calls `AlphaTypeProbe.clipKey`, `AlphaTypeProbe.probeDXV`, `DXVDetector.compressorFourCC` / `displayName`, Swift's routing order (`DXVDemuxer.demux`, `HAPDemuxer.demux`, AVFoundation's playable test with `FrameTiming.load`) and `HAPThumbnail.rgbaOfFrame`; the message expression is copied from `AVPlaybackController.swift:84-123` because the app target cannot be imported.
- **One OS-codec clip comes 15 frames short:** `hap-testclips/std_film_h264_audio.mp4` (H.264, 25 fps). The probe reports 768 frames (the container's sample count); the OS decoder gives 753, and frames 753–767 come back as "frame 753 is out of range (clip has 753)" (`DXVKit-Plus/crates/dxvkit-plus-core/src/source.rs:954-961`: the shorter count bounds the clip). So `MediaInfo.frames` overstates such a clip by 0.6 s, and a player that trusts it asks for 15 frames that do not exist at the end of every loop. Not fixed in C1 (the core does not patch DXVKit Plus, and the time rules are C2). **For DXVKit Plus's container-timing wave:** the probe's frame count for a native clip should be the count the decoder will serve; it is not in the C1 findings brief. What Swift Glance shows at the end of this clip was not examined. The other 32 OS-codec clips showed every frame.
- **What "shown" means for OS-codec clips:** right size, counted, and counted separately when a frame holds no colour. There is no CPU reference for that path in C1.
- **Not re-checked here:** that Vidvox's own shader uses 128/255 (the brief states Cowork verified it; DXVKit Plus's `hap.rs:312` states the same formula).
- The revised DXVKit Plus brief says CI found the BC7 difference "an hour later"; it was 11 minutes.

## For Amrit (one line each)
1. **Licence exception, needs your yes:** egui's built-in fonts (Ubuntu Light under the Ubuntu Font Licence, Noto Emoji under the SIL Open Font License) are in the proof binary, as they were in W0; `cargo deny` found them. They are allowed for one named crate until the shell wave (C3) switches to the system font like the Swift apps. Say no and C3 does that first.
2. The checkerboard's phase differs from Swift Glance (which square is the light one at the corner): a look item for Glance G2, not changed here.
3. Nobody has looked at a window in C1 either: everything is measured by read-back.

## Briefs written
- `~/claude-handoffs/DXVKit-Plus_2026-11-14_c1-findings_brief-r2.md` — eight findings (device texture limit, track choice, truncated HAP track, name before content, corrupt container and the platform backend, the Apple island on Intel and iOS, the FourCC reader, reserved-mode BC7 blocks). It supersedes `..._c1-findings_brief.md`, written an hour earlier without the eighth.

## RECEIPTS
```
$ cargo fmt --all --check && echo ok
ok
$ cargo clippy --workspace --all-targets --release -- -D warnings
    Finished `release` profile [optimized + debuginfo] target(s) in 0.20s
$ GLANCE_REQUIRE_GPU=1 cargo test --workspace --release   (test result lines)
     Running unittests src/lib.rs
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/exact.rs
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.02s
     Running tests/present.rs
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
     Running unittests src/lib.rs
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/routing.rs
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 20.01s
     Running tests/swift_parity.rs
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.48s
     Running unittests src/lib.rs
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running unittests src/main.rs
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running unittests src/lib.rs
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/screen.rs
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok
$ cargo build --release
    Finished `release` profile [optimized + debuginfo] target(s) in 0.09s
$ ./target/release/glance-proof gpu
{
  "adapter": "Apple M5 Max",
  "backend": "Metal",
  "bc_textures": true,
  "device_type": "IntegratedGpu",
  "driver": "",
  "max_texture_dimension_2d": 16384
}
$ ls -la docs/data/ | grep c1
-rw-r--r--   1 <user>  staff   14245 14 Nov 21:16 c1-hapq-testfiles.json
-rw-r--r--   1 <user>  staff  210023 14 Nov 22:20 c1-verify-testfiles.json
-rw-r--r--   1 <user>  staff    2770 14 Nov 21:41 c1-window-smoke.json
$ ls -la tests/data/
total 96
drwxr-xr-x  6 <user>  staff    192 14 Nov 22:07 .
drwxr-xr-x  5 <user>  staff    160 14 Nov 21:01 ..
-rw-r--r--  1 <user>  staff   7579 14 Nov 21:03 swift-glance-fixtures.tsv
-rw-r--r--  1 <user>  staff  12084 14 Nov 21:09 swift-glance-hap-rgba.tsv
-rw-r--r--  1 <user>  staff    359 14 Nov 22:07 swift-glance-hapq-testfiles.tsv
-rw-r--r--  1 <user>  staff  23008 14 Nov 21:03 swift-glance-testfiles.tsv
$ du -sh tests/fixtures
1.2M	tests/fixtures
$ git -C .ref/DXVKit-Plus describe --tags; git -C .ref/glance log --oneline -1
v0.8.0
a9dc601 Handoff: release flow leaves one LaunchServices record (ls-cleanup.sh)
$ gh run view 43642394798 (fda39bd)
lint: success / macos: success (Apple Paravirtual device; 32 tests; x86_64-apple-darwin and aarch64-apple-ios checks) / windows: success
$ git log --oneline 6063231..HEAD
fda39bd C1: verifier corrections; Swift cross-test program; black-frame count
f4bb509 C1: results (Amrit's 174 test files through the harness, HAP Q measurement, CI)
ba20a42 C1: CI records the runner GPU's reserved-BC7 answer; CLAUDE.md and checkpoint
2069c2b C1: clippy (as_chunks_mut in the test support)
0ce9e89 C1: valid BC7 blocks in the generated movies; reserved BC7 blocks get their own test
6b80bf1 C1: exact picture on DXVKit Plus 0.8.0, routing, clip key, alpha probe, MIT, CI
```
