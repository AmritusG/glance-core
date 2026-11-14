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

No decoder, demuxer or encoder is in this repository: they are DXVKit Plus's, a dependency
pinned by tag (`Cargo.toml`).

## Assets

| Asset | What | Licence |
| --- | --- | --- |
| `tests/fixtures/tiles_*.{mov,mp4}` (16 files, sidecars `*.attribution`) | DXVKit Plus's procedural tile pattern, 128x72, 24 frames; copied unchanged (`cp -p`) from DXVKit Plus v0.8.0 (9bc1977) `tests/fixtures/` | CC0-1.0 |
| `tests/fixtures/synthetic_hap7.mov`, `tests/fixtures/synthetic_haph.mov` (sidecars) | HAP R and HAP HDR movies written byte by byte by DXVKit Plus; copied unchanged from DXVKit Plus v0.8.0 | CC0-1.0 |
| `tests/fixtures/dxvkit/` (`vectors.tsv` + 34 streams, sidecars) | DXVKit 1.0.0's test vectors (GlEnc / synthetic / invalid groups) with their reference SHA-256 rows; copied unchanged from DXVKit Plus v0.8.0 | MIT (`LICENSES/DXVKit-MIT.txt`) |
| `tests/data/swift-glance-*.tsv` | text only: what Swift Glance's own code (GlanceCore @ a9dc601) returns for the fixtures above and for Amrit's test files, by name — routes, messages, FourCCs, clip keys, hint counts, frame hashes; made by `tools/swift-crosstest`. No media | our own |

No third-party media, and none of Amrit's footage, is in this repository. The sidecar
`*.attribution` files keep the paths they had in DXVKit Plus.

## Dependencies

Every dependency is permissive (MIT, Apache-2.0, BSD-2/3, Zlib, ISC, MIT-0), enforced by
`cargo deny check` (`deny.toml`), with two crate-scoped exceptions: `unicode-ident`
(Unicode-3.0; Unicode's data tables, build time only) and `epaint_default_fonts` (egui's built-in
fonts: Ubuntu Light under the Ubuntu Font Licence 1.0, Noto Emoji under the SIL Open Font License
1.1) until the shell wave gives the apps the system font. FFmpeg, libmpv and GStreamer bindings
are banned, and so are the MPL-2.0 demuxers.
