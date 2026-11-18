# Attribution

glance-core is MIT-licensed (`LICENSE`, Copyright (c) 2026 Amrit Rosell). This file lists what
came from somewhere else, with its licence.

## Code

| Code in this repo | From | Licence |
| --- | --- | --- |
| `clippy.toml`, `rustfmt.toml`, the never-crash lints in `Cargo.toml`, the shape of `deny.toml` | DXVKit Plus (Amrit Rosell; AmritusG/DXVKit-Plus), there from storytold/effectcraft via Loombda-rs | MIT |
| `tests/support/movie.rs` (the single-track QuickTime layout, the HAP section writer, the random-block boundary cases) | DXVKit Plus v0.8.0 `tests/support/qt_movie.rs` and `crates/dxvkit-plus-wgpu/tests/exact.rs` (Amrit Rosell), rewritten for several tracks and for every variant | MIT |
| `crates/glance-gpu/src/verify.rs`, `crates/glance-gpu/tests/exact.rs` (the shape of the harness: rows, the flipped-bit control) | DXVKit Plus v0.8.0 `crates/dxvkit-plus-wgpu/tests/exact.rs` (Amrit Rosell) | MIT |
| `crates/glance-media/src/route.rs` (routing order, the "not supported" wording, the FourCC names, the atom walk of `first_video_fourcc`, the Matroska codec id), `crates/glance-media/src/alpha.rs` (the hint's metric and thresholds, the clip key) | Swift Glance (Amrit Rosell; AmritusG/glance @ a9dc601): `Sources/Glance/PlayerModel.swift`, `Sources/Glance/AVPlaybackController.swift`, `Sources/GlanceCore/DXVDetector.swift`, `Sources/GlanceCore/AlphaType.swift`, written again in Rust | MIT (Glance's own licence; Amrit's own code) |
| `crates/glance-proof/src/hapq.rs` (`swift_ycocg`) | Swift Glance `Sources/GlanceCore/HAPHQDecoder.swift` `hapYCoCgToRGB`, written again in Rust for the measurement | MIT (as above) |
| `tools/swift-crosstest/Sources/xtest/main.swift` (`matroskaCodec`, `unsupportedMessage`, the routing order) | Swift Glance `Sources/Glance/AVPlaybackController.swift:84-123` and `Sources/Glance/PlayerModel.swift:209-229`, copied because the app target cannot be imported by a package; the rest of the program only calls GlanceCore | MIT (as above) |
| `crates/glance-ui` (the egui host on a winit loop, existing device handed to egui-wgpu) | the pattern of Loombda-rs `loombda-editor` (Amrit Rosell); no code copied | Amrit's own |
| `crates/glance-ui/src/{look,bar,menu,dialog,window}.rs` (sizes, colours, tooltips, menu items and shortcuts, alert words, the fit rule), `crates/glance-settings` (the keys, encodings, cap and newest-line rule), `crates/glance-platform/src/macos.rs` (the media commands, the default-handler and open-file flows) | Swift Glance @ a9dc601: `Sources/Glance/{TransportBar,ScrubBar,PlayerScreen,GlanceApp,DefaultHandlerControl,NowPlayingController,AlphaTypeMemory,PlayerModel}.swift`, `Sources/GlancePlayback/ClipTitle.swift`, written again in Rust | MIT (as above) |
| `tools/swift-settings/harness.swift` (`AlphaTypeMemory`, the background and prompt-flag lines) | Swift Glance @ a9dc601 `Sources/Glance/AlphaTypeMemory.swift:10-26`, `PlayerModel.swift:63-71`, `DefaultHandlerControl.swift:25,43-50`, copied for the round-trip test | MIT (as above) |
| `tools/package/ls-cleanup.sh`, the order and checks of `tools/package/*.sh` | Swift Glance @ a9dc601 `scripts/{ls-cleanup,build,sign,notarize,make-dmg,install-from-dmg}.sh` (and Cowork's `ls-cleanup.sh`), made project-agnostic | MIT (as above) |

No decoder, demuxer or encoder is in this repository: they are DXVKit Plus's, a dependency
pinned by tag (`Cargo.toml`).

## Assets

| Asset | What | Licence |
| --- | --- | --- |
| `tests/fixtures/tiles_*.{mov,mp4}` (16 files of the 17, sidecars `*.attribution`) | DXVKit Plus's procedural tile pattern, 128x72, 24 frames; copied unchanged (`cp -p`) from DXVKit Plus v0.8.0 (9bc1977) `tests/fixtures/` | CC0-1.0 |
| `tests/fixtures/tiles_h264_fragmented.mp4` (sidecar) | `tiles_h264.mp4` above remuxed without re-encoding into a fragmented MP4 (`moof` fragments, empty `moov` tables) by the ffmpeg binary as a black-box tool (nothing of FFmpeg linked or bundled); v0.7.2 | CC0-1.0 |
| `tests/fixtures/synthetic_hap7.mov`, `tests/fixtures/synthetic_haph.mov` (sidecars) | HAP R and HAP HDR movies written byte by byte by DXVKit Plus; copied unchanged from DXVKit Plus v0.8.0 | CC0-1.0 |
| `tests/fixtures/frame-timing/frames_*.mov` (8 files, sidecar `ATTRIBUTION`) | Swift Glance's frame-timing fixtures (frame N has luma 5 N; H.264 with B-frames at five rates, jittered H.264 and ProRes 4444); copied unchanged (`cp -p`) from AmritusG/glance @ a9dc601 `Tests/Fixtures/frame-timing` | MIT (Amrit Rosell) |
| `tests/fixtures/audio_pcm_formats.mov`, `tests/fixtures/audio_aac_priming.mov` (sidecars) | Sound tracks over a HAP picture (every PCM layout; AAC with priming), written byte by byte by DXVKit Plus's `scripts/make-audio-fixtures.py` from its own procedural signals; copied unchanged (`cp -p`) from DXVKit Plus v0.11.0 (29fa9f5) `tests/fixtures/` | CC0-1.0 |
| `tests/fixtures/dxvkit/` (`vectors.tsv` + 34 streams, sidecars) | DXVKit 1.0.0's test vectors (GlEnc / synthetic / invalid groups) with their reference SHA-256 rows; copied unchanged from DXVKit Plus v0.8.0 | MIT (`LICENSES/DXVKit-MIT.txt`) |
| `tests/data/swift-glance-*.tsv` | text only: what Swift Glance's own code (GlanceCore @ a9dc601) returns for the fixtures above and for Amrit's test files, by name (14 under a neutral name, found by their
bytes through `tests/data/testfiles-by-content.tsv`: sizes and hashes only) — routes, messages, FourCCs, clip keys, hint counts, frame hashes; made by `tools/swift-crosstest`. No media | our own |
| `crates/glance-library/tests/data/files/` | text only: what Swift MaCrate 0.16.4's own code (`tools/swift-library-oracle/FilesOracle.swift`) prints for generated inputs (doubles, names, paths, the folder and movies `crates/glance-library/tests/files_support/` writes), for the fixtures above by name, and for Amrit's test files by content id only (no names). No media | our own |

No third-party media, and none of Amrit's footage, is in this repository or its history (the history
was cleaned before publishing, 2026-11-17). The sidecar
`*.attribution` files keep the paths they had in DXVKit Plus.

## Dependencies

Every dependency is permissive (MIT, Apache-2.0, BSD-2/3, Zlib, ISC, MIT-0), enforced by
`cargo deny check` (`deny.toml`), with one crate-scoped exception: `unicode-ident` (Unicode-3.0;
Unicode's data tables, build time only). egui's built-in fonts (`epaint_default_fonts`, the second
exception until C3) are no longer linked: the apps draw with the system font, read from the OS at
run time and never bundled. FFmpeg, libmpv and GStreamer bindings are banned, and so are the
MPL-2.0 demuxers.
