# glance-core

The shared Rust core of the Glance family (Glance, CueGlance, GlEnc, MaCrate, later GlancEhr):
egui + wgpu, macOS and Windows, on top of [DXVKit Plus](https://github.com/AmritusG/DXVKit-Plus).

Start with `CLAUDE.md`, then `docs/00-ROADMAP.md`. State of the work: `CHECKPOINT.md`.

```sh
cargo build --release
./target/release/glance-proof clip_hap.mov clip_dxv.mov      # the W0 proof window
```
