# glance-core v0.9.0 — several outputs, Syphon and Spout (for GlancEhr)

Asked by GlancEhr's goal "all functions and transitions" (2026-11-21, brief
`~/claude-handoffs/GlancEhr_2026-11-21_all-functions-and-transitions_brief.md`, functions 7 and 8).

## What changed
- **glance-ui: several output windows at once.** `Host::add_output(screen)` opens one more output beside those open,
  `Host::close_output_on(screen)` closes one, `Host::output_screens()` lists them; `open_output` / `close_output` /
  `output_screen` keep their meaning (one output; the first). Each output keeps its own surface and pacing;
  `OutputFrame::screen` (new) names the screen `App::output` draws for. `OutputFrame` is no longer `Copy` (it carries
  the id). A screen unplugged closes only its own output (`OutputClosed(ScreenGone(id))`). The pointer is hidden over
  any output.
- **glance-platform: `SyphonServer` (macOS).** Syphon.framework (BSD-3-style) is loaded at run time from the app
  bundle's Frameworks (or `GLANCE_SYPHON_FRAMEWORK`); `SyphonServer::new(name, device, queue)` makes a
  SyphonMetalServer on wgpu's Metal device, `publish(texture)` hands the texture's MTLTexture over on a command buffer of
  wgpu's own queue (no copy), `has_clients()`. `syphon_available()` says where it looked when missing.
- **glance-platform: `SpoutSender` (Windows).** Spout 2's sender protocol written from Spout2's source (BSD-2-Clause):
  a shared D3D11 texture (R8G8B8A8, legacy share handle), the sorted "SpoutSenderNames" list (MaxSenders × 256 from the
  registry or 64) and "ActiveSenderName", the sender's 280-byte `SharedTextureInfo`, writes under
  "<name>_SpoutAccessMutex"; `send(rgba, w, h)` copies a frame in with `UpdateSubresource`. Unit test of the list and
  record layout; built and linted for `x86_64-pc-windows-msvc` on this Mac, run on Windows at the app's Windows run.

## Gates
fmt, clippy -D warnings (and clippy for the Windows target on glance-platform), tests (GLANCE_REQUIRE_GPU=1,
GLANCE_TESTFILES set): all pass except `glance-library` `files_swift_parity::test_media_reads_as_swift_reads_it`
(182 files where the Swift oracle read 183): the test-media folder changed since the oracle was taken; glance-library
is untouched by this change. cargo deny ok, build ok.

## Proven in GlancEhr
Two outputs on virtual screens, Syphon with a Syphon client: GlancEhr `docs/handoffs/f6-screens-feeds.md`.
