# v0.9.1 — the headless stage as a scripted host, glance-encode's FrameWriter, DXVKit Plus v0.15.2

Made for GlancEhr's crawl-eval-transitions pass (2026-11-21): CrawlEhr's UI crawl and scenarios drive GlancEhr's real
app, which runs on glance-ui's shell (not eframe), and its transition reel is written as H.264.

## What shipped
- **glance-ui `headless::Stage`** (no window, as before) is also a scripted host: `step` runs one frame with input
  events and hands back egui's whole output (the AccessKit tree once `context().enable_accesskit()`), textures uploaded
  and nothing drawn or read back; `set_screens` makes up the screens attached (a change is a `ShellEvent::Screens`, an
  output whose screen left closes with `OutputEnd::ScreenGone`, as the shell does once a second); the output windows an
  app asks for (`open_output`, `add_output`, `close_output`, `close_output_on`) are kept as a list by the shell's rules
  (`outputs`), their events queued (`take_events`); `with_host` runs an `App::event` call with the stage's host;
  `quit_asked` records `Host::quit`. `frame` (the readback path) uses the same host.
- **glance-encode `FrameWriter`**: frames an app makes itself, written by the engine's own platform writer (the same
  AVAssetWriter path, compression properties and RGBA → BGRA step an encode job uses); platform codecs only, macOS.
- **DXVKit Plus v0.15.2**: the CVMetalTextureCache import route held its textures without their cache and its lock;
  a texture released by `Queue::maintain` on another thread crashed in `CVMetalTextureCache::bufferBackingNotInUse`
  (found by GlancEhr's crawl: its probe and analysis threads open decks beside the show).

## Validated
- `crates/glance-ui/tests/stage_host.rs`: a click reaches the UI and the tree names the button; outputs follow the
  shell's rules through an unplug and a replug; a quit is recorded (2 passed).
- `crates/glance-encode/tests/encode.rs` `a_frame_writer_writes_what_it_is_given`: 30 frames of 128 x 72 at 30 fps come
  back as 30, red then blue (the H.264 round trip shifts red 220, 30, 30 to 203, 9, 33); a wrong-sized frame and a DXV
  codec are refused.
- DXVKit Plus: `textures_released_on_another_thread_while_the_cache_works` crashed 3 of 3 runs before the fix, green 5
  of 5 after.
- Gates: fmt, clippy -D warnings, tests (GPU required, test media set), deny, build, path-guard. One failure, known and
  not from this change: `files_swift_parity` (the test-media folder changed since its Swift oracle: 182 files where the
  oracle read 183).

## Open
- DXVKit Plus on the home machine: three golden tests against the Swift apps' platform reads fail on unmodified
  v0.15.1 / v0.15.2 too (`bgra_reader_serves_what_glencs_avassetreader_serves`, two in `macrate_av`): the OS's H.264
  decode drifted from the goldens. Not touched here.
