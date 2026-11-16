//! glance-sample (macOS; Windows compiles): the core's shell in one bundled app, under the throw-away bundle id
//! `com.amritus.glancecore.sample` (never Glance's). It plays a clip with Glance's look, bar, scrub
//! row, menus and window rules; reads and writes its own preferences domain with Glance's keys
//! and encodings (glance-settings); opens files sent by Finder at cold launch and while running,
//! in its one window; takes the media keys and publishes Now Playing. Not a product, not
//! published: the proof of core wave C3 (docs/06-C3-RESULTS.md).
//!
//! ```sh
//! glance-sample [<clip>]                 play (a clip from the command line, or ⌘O / drop / Finder)
//! glance-sample --toggle-background      flip the stored background and exit (settings proof)
//! glance-sample --set-default-test       ask the OS to make this app the default for its own
//!                                        throw-away type and exit with the answer
//! ```
//!
//! Every event is appended as one JSON line to
//! `~/Library/Logs/com.amritus.glancecore.sample/events.jsonl` (macOS; the system's log folder
//! elsewhere), so a test can see what arrived without driving the window.

use glance_gpu::{ClipView, Gpu, Presenter};
use glance_media::{AlphaRule, AlphaView};
use glance_playback::{ClipPlayer, Sound};
use glance_settings::{BackgroundMode, GlanceSettings, Store};
use glance_ui::bar::{self, AlphaControl, BarAction, BarState, ScrubAction, ScrubDrag, ScrubState};
use glance_ui::menu::{GlanceMenu, MenuCommand};
use glance_ui::{App, Background, Checker, Host, MediaCommand, NowPlaying, Picture, PlatformEvent, ShellConfig, ShellEvent, dialog, look};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const APP: &str = "Glance Sample";
const BUNDLE_ID: &str = "com.amritus.glancecore.sample";
/// The sample's own throw-away document type (Info.plist `UTExportedTypeDeclarations`): the
/// "Set as Default" flow is proven on it, never on `.mov` (that would take Amrit's movies from
/// Glance).
const SAMPLE_TYPE: &str = "com.amritus.glancecore.sample-clip";

fn log_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Logs").join(BUNDLE_ID).join("events.jsonl"))
}

fn log(event: serde_json::Value) {
    let Some(path) = log_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut line = event;
    if let Some(o) = line.as_object_mut() {
        o.insert(
            "unix_ms".into(),
            serde_json::json!(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)),
        );
        o.insert("pid".into(), serde_json::json!(std::process::id()));
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

/// A clip in the window.
struct Loaded {
    path: PathBuf,
    player: ClipPlayer,
    rule: AlphaRule,
    alpha: AlphaView,
    /// The key the Alpha Type is remembered by (DXV with alpha).
    key: Option<String>,
    hint: Option<(String, u64, f64)>,
    codec: String,
    readout: String,
    shown: Option<(u32, AlphaView)>,
    logged_first_frame: bool,
}

struct Sample {
    settings: GlanceSettings<dyn Store>,
    menu: GlanceMenu,
    media: Option<glance_platform::MediaKeys>,
    media_tried: bool,
    presenter: Presenter,
    view: ClipView,
    picture: Option<Picture>,
    drawn: Option<egui::load::SizedTexture>,
    checker: Option<Checker>,
    clip: Option<Loaded>,
    message: Option<String>,
    background: BackgroundMode,
    volume: f32,
    muted: bool,
    scrub: ScrubDrag,
    title: String,
    /// A clip opened: fit the window to it on the next frame.
    fit: Option<(u32, u32)>,
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

impl Sample {
    fn new(gpu: &Gpu, first: Option<PathBuf>) -> Result<Sample, String> {
        let store = glance_settings::app_store(BUNDLE_ID, "Amritus", APP)?;
        let settings = GlanceSettings::new(store);
        let background = settings.background();
        let menu = GlanceMenu::new(APP, env!("CARGO_PKG_VERSION"))?;
        menu.set_background(background == BackgroundMode::Checkerboard);
        log(serde_json::json!({"event": "launch", "background": background.stored_name(), "args_file": first.as_ref().map(|p| p.display().to_string())}));
        let mut s = Sample {
            settings,
            menu,
            media: None,
            media_tried: false,
            presenter: Presenter::new(&gpu.device),
            view: ClipView::new(&gpu.device),
            picture: None,
            drawn: None,
            checker: None,
            clip: None,
            message: None,
            background,
            volume: 1.0,
            muted: false,
            scrub: ScrubDrag::default(),
            title: String::new(),
            fit: None,
        };
        if let Some(path) = first {
            s.open(&path, "argument");
        }
        Ok(s)
    }

    /// Open `path` in the one window (Swift's `PlayerModel.open`): routed by content; a file
    /// nothing plays shows the message; the Alpha Type is the remembered choice, applied before
    /// the first frame.
    fn open(&mut self, path: &Path, source: &str) {
        self.clip = None;
        self.drawn = None;
        self.message = None;
        #[cfg(target_os = "macos")]
        let _ = glance_platform::note_recent(path);
        let routed = match glance_media::route(path) {
            Ok(c) => c,
            Err(e) => {
                log(serde_json::json!({"event": "open", "source": source, "path": path.display().to_string(), "message": e.message}));
                self.message = Some(e.message);
                return;
            }
        };
        let player = match ClipPlayer::open_with(path, glance_media::CacheSettings { capacity: 24, read_ahead: 8, threads: 4 }, Sound::Device) {
            Ok(p) => p,
            Err(e) => {
                log(serde_json::json!({"event": "open", "source": source, "path": path.display().to_string(), "message": e.message}));
                self.message = Some(e.message);
                return;
            }
        };
        let rule = glance_media::alpha_rule(&routed.info);
        let key = if rule == AlphaRule::Choosable { glance_media::clip_key(path) } else { None };
        let alpha = match rule {
            AlphaRule::Stated(v) => v,
            _ => key.as_deref().and_then(|k| self.settings.alpha_choice(k)).unwrap_or_default(),
        };
        let report = glance_media::alpha_report(&routed);
        let hint = report.look.map(|l| (l.label().to_string(), report.samples, report.share()));
        let info = &routed.info;
        let codec = match info.dxv_variant {
            Some(v) => format!("{} {}", info.codec_name, glance_media::dxv_variant_label(v)),
            None => info.codec_name.clone(),
        };
        let duration = f64::from(info.frames) / info.rate.as_f64().max(1e-9);
        let readout = bar::info_line(info.width, info.height, info.rate.as_f64(), duration);
        let mut player = player;
        player.clock_mut().set_volume(self.volume);
        player.clock_mut().set_muted(self.muted);
        log(serde_json::json!({
            "event": "open", "source": source, "path": path.display().to_string(), "codec": codec,
            "alpha_rule": format!("{rule:?}"), "alpha": alpha.stored_name(), "remembered": key.as_deref().and_then(|k| self.settings.alpha_choice(k)).map(|v| v.stored_name()),
            "audio": format!("{:?}", player.clock().audio_state()),
        }));
        self.fit = Some((info.width, info.height));
        self.clip = Some(Loaded { path: path.to_path_buf(), player, rule, alpha, key, hint, codec, readout, shown: None, logged_first_frame: false });
        self.menu.set_alpha(rule == AlphaRule::Choosable, alpha == AlphaView::Straight);
        self.publish_now_playing();
    }

    fn set_alpha(&mut self, view: AlphaView) {
        let Some(c) = self.clip.as_mut() else { return };
        if c.rule != AlphaRule::Choosable || c.alpha == view {
            return;
        }
        c.alpha = view;
        if let Some(k) = c.key.as_deref() {
            self.settings.remember_alpha(k, view);
        }
        self.menu.set_alpha(true, view == AlphaView::Straight);
        log(serde_json::json!({"event": "alpha", "alpha": view.stored_name()}));
    }

    fn set_background(&mut self, mode: BackgroundMode) {
        self.background = mode;
        self.settings.set_background(mode);
        self.menu.set_background(mode == BackgroundMode::Checkerboard);
        log(serde_json::json!({"event": "background", "background": mode.stored_name()}));
    }

    fn publish_now_playing(&self) {
        let Some(media) = self.media.as_ref() else { return };
        let info = match self.clip.as_ref() {
            None => NowPlaying::default(),
            Some(c) => {
                let clock = c.player.clock();
                let elapsed = glance_playback::seconds(glance_playback::ticks(clock.time_of_frame(clock.current())));
                let i = c.player.info();
                NowPlaying { title: Some(file_name(&c.path)), duration: f64::from(i.frames) / i.rate.as_f64().max(1e-9), elapsed, playing: !clock.is_paused() }
            }
        };
        media.now_playing(&info);
    }

    /// Relative seek in frames, as Swift does on the frame players: `current + round(seconds × fps)`.
    fn seek_by(&mut self, seconds: f64) {
        if let Some(c) = self.clip.as_mut() {
            let clock = c.player.clock_mut();
            let fps = clock.clip().rate.as_f64();
            let target = (i64::from(clock.current()) + (seconds * fps).round() as i64).clamp(0, i64::from(clock.clip().frames.saturating_sub(1)));
            clock.seek(u32::try_from(target).unwrap_or(0));
        }
    }

    fn with_clock(&mut self, f: impl FnOnce(&mut glance_playback::Clock)) {
        if let Some(c) = self.clip.as_mut() {
            f(c.player.clock_mut());
        }
    }

    fn adjust_volume(&mut self, by: f32) {
        self.volume = (self.volume + by).clamp(0.0, 1.0);
        let v = self.volume;
        self.with_clock(|c| c.set_volume(v));
    }

    fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        let m = self.muted;
        self.with_clock(|c| c.set_muted(m));
    }

    fn menu_command(&mut self, c: MenuCommand, host: &mut Host<'_>) {
        log(serde_json::json!({"event": "menu", "command": format!("{c:?}")}));
        match c {
            MenuCommand::Open => {
                if let Some(p) = dialog::open_movie() {
                    self.open(&p, "dialog");
                }
            }
            #[cfg(target_os = "macos")]
            MenuCommand::SetAsDefault => {
                if glance_platform::is_default_for(SAMPLE_TYPE).unwrap_or(false) {
                    dialog::inform(&format!("{APP} is already the default for its sample files."), "");
                } else if let Err(e) = glance_platform::set_default_for(SAMPLE_TYPE) {
                    let (t, body) = dialog::default_handler::failed(APP, &e.to_string());
                    dialog::warn(&t, &body);
                }
            }
            // Windows: an app cannot make itself the default; Settings does it (G4's flow).
            #[cfg(not(target_os = "macos"))]
            MenuCommand::SetAsDefault => {}
            MenuCommand::DefaultWindowSize => host.default_size(look::DEFAULT_SIZE),
            MenuCommand::BackgroundBlack => self.set_background(BackgroundMode::Black),
            MenuCommand::BackgroundCheckerboard => self.set_background(BackgroundMode::Checkerboard),
            MenuCommand::AlphaPremultiplied => self.set_alpha(AlphaView::Premultiplied),
            MenuCommand::AlphaStraight => self.set_alpha(AlphaView::Straight),
            MenuCommand::ToggleAlphaType => {
                let next = match self.clip.as_ref().map(|c| c.alpha) {
                    Some(AlphaView::Premultiplied) => AlphaView::Straight,
                    _ => AlphaView::Premultiplied,
                };
                self.set_alpha(next);
            }
            MenuCommand::PlayPause => self.with_clock(glance_playback::Clock::toggle),
            MenuCommand::PreviousClip | MenuCommand::NextClip => {}
            MenuCommand::Restart => self.with_clock(glance_playback::Clock::restart),
            MenuCommand::VolumeUp => self.adjust_volume(0.1),
            MenuCommand::VolumeDown => self.adjust_volume(-0.1),
            MenuCommand::Mute => self.toggle_mute(),
            MenuCommand::Loop => self.with_clock(|c| {
                let l = c.is_looping();
                c.set_looping(!l);
            }),
        }
        self.publish_now_playing();
    }

    /// Swift's key map (`VideoHostingView.keyDown`): arrows seek 5 s (Shift 0.5 s), up / down
    /// volume, `,` `.` step, `m` mute, `-` `=` volume, `l` loop, `f` full screen, Esc quits while
    /// playing a file outside full screen. Space and A are menu shortcuts: the window never gets them.
    fn keys(&mut self, ui: &egui::Ui, host: &mut Host<'_>) {
        let events = ui.input(|i| i.events.clone());
        for e in events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            // Every press the window gets, so a run can show which keys the menu took instead.
            log(serde_json::json!({"event": "key", "key": key.name()}));
            if modifiers.command {
                continue;
            }
            use egui::Key as K;
            match key {
                K::ArrowLeft => self.seek_by(if modifiers.shift { -0.5 } else { -5.0 }),
                K::ArrowRight => self.seek_by(if modifiers.shift { 0.5 } else { 5.0 }),
                K::ArrowUp => self.adjust_volume(0.1),
                K::ArrowDown => self.adjust_volume(-0.1),
                K::Comma => self.with_clock(|c| c.step(false)),
                K::Period => self.with_clock(|c| c.step(true)),
                K::M => self.toggle_mute(),
                K::Minus => self.adjust_volume(-0.1),
                K::Equals | K::Plus => self.adjust_volume(0.1),
                K::L => self.with_clock(|c| {
                    let l = c.is_looping();
                    c.set_looping(!l);
                }),
                K::F => host.toggle_fullscreen(),
                K::Escape => {
                    let playing = self.clip.as_ref().is_some_and(|c| !c.player.clock().is_paused());
                    if !host.is_fullscreen() && self.message.is_none() && playing {
                        host.quit();
                    }
                }
                _ => {}
            }
        }
    }

    fn bar_action(&mut self, a: BarAction, host: &mut Host<'_>) {
        match a {
            BarAction::PreviousClip | BarAction::NextClip => {}
            BarAction::TogglePlay => self.with_clock(glance_playback::Clock::toggle),
            BarAction::Restart => self.with_clock(glance_playback::Clock::restart),
            BarAction::SeekBy(s) => self.seek_by(s),
            BarAction::Step { forward } => self.with_clock(|c| c.step(forward)),
            BarAction::ToggleLoop => self.with_clock(|c| {
                let l = c.is_looping();
                c.set_looping(!l);
            }),
            BarAction::ToggleMute => self.toggle_mute(),
            BarAction::SetVolume(v) => {
                self.volume = v.clamp(0.0, 1.0);
                let v = self.volume;
                self.with_clock(|c| c.set_volume(v));
            }
            BarAction::SetAlpha(v) => self.set_alpha(v),
            BarAction::ToggleFullscreen => host.toggle_fullscreen(),
        }
        self.publish_now_playing();
    }
}

impl App for Sample {
    fn menu(&self) -> Option<&dyn glance_ui::menu::MenuBar> {
        Some(&self.menu)
    }

    fn event(&mut self, event: ShellEvent, host: &mut Host<'_>) {
        match event {
            ShellEvent::Menu(c) => self.menu_command(c, host),
            ShellEvent::Dropped(p) => self.open(&p, "drop"),
            ShellEvent::Platform(PlatformEvent::Open(paths)) => {
                // Several files: the last one wins (Swift's `application(_:open:)`).
                log(serde_json::json!({"event": "os-open", "paths": paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()}));
                if let Some(p) = paths.last() {
                    self.open(p, "os");
                }
            }
            ShellEvent::Platform(PlatformEvent::Media(m)) => {
                log(serde_json::json!({"event": "media", "command": format!("{m:?}")}));
                match m {
                    MediaCommand::TogglePlayPause => self.with_clock(glance_playback::Clock::toggle),
                    MediaCommand::Play => self.with_clock(glance_playback::Clock::play),
                    MediaCommand::Pause => self.with_clock(glance_playback::Clock::pause),
                    MediaCommand::Previous | MediaCommand::Next => {}
                    MediaCommand::Seek(t) => {
                        if let Some(c) = self.clip.as_mut() {
                            let clock = c.player.clock_mut();
                            let f = clock.clip().nearest_frame(glance_media::Tick::from_units((t * 1e9) as i64, 1_000_000_000));
                            clock.seek(f);
                        }
                    }
                }
                self.publish_now_playing();
            }
            // The sample adds no menu items of its own and never says it has unsaved work.
            ShellEvent::MenuItem(_) | ShellEvent::Platform(PlatformEvent::QuitRequested) => {}
            ShellEvent::Platform(PlatformEvent::DefaultHandlerSet(result)) => {
                log(serde_json::json!({"event": "default-handler", "result": format!("{result:?}")}));
                match result {
                    Ok(()) => dialog::inform(&format!("{APP} is now the default for its sample files."), ""),
                    Err(why) => {
                        let (t, body) = dialog::default_handler::failed(APP, &why);
                        dialog::warn(&t, &body);
                    }
                }
            }
        }
    }

    fn frame(&mut self, ui: &mut egui::Ui, host: &mut Host<'_>) {
        if !self.media_tried {
            self.media_tried = true;
            self.media = register_media(host).map_err(|e| log(serde_json::json!({"event": "media-keys", "error": e.to_string()}))).ok();
            log(serde_json::json!({"event": "media-keys", "registered": self.media.is_some()}));
            self.publish_now_playing();
        }
        self.keys(ui, host);
        if let Some(video) = self.fit.take() {
            host.fit_width(video, glance_ui::window::GLANCE_BOTTOM_CHROME);
        }
        // The picture for the moment this refresh reaches the screen.
        let shown_at = Instant::now() + host.refresh_millihertz.map_or(Duration::from_millis(8), |mhz| Duration::from_secs_f64(1000.0 / f64::from(mhz.max(1))));
        if let Some(c) = self.clip.as_mut() {
            match c.player.frame_at(shown_at) {
                Ok(shown) => {
                    if c.shown != Some((shown.index, c.alpha)) {
                        match self.view.show(host.gpu, &self.presenter, &shown.frame, c.alpha) {
                            Ok(t) => {
                                let t = t.clone();
                                self.drawn = Some(host.picture(&mut self.picture, &t));
                                c.shown = Some((shown.index, c.alpha));
                                if !c.logged_first_frame {
                                    c.logged_first_frame = true;
                                    log(serde_json::json!({"event": "shown", "path": c.path.display().to_string(), "frame": shown.index}));
                                }
                            }
                            Err(e) => self.message = Some(e.to_string()),
                        }
                    }
                }
                Err(e) => self.message = Some(e.to_string()),
            }
        }
        // Title and subtitle.
        let suffix = self.clip.as_ref().map(|c| bar::title_suffix(Some(&c.codec), c.player.clock().is_looping())).unwrap_or_default();
        let title = bar::window_title(APP, self.clip.as_ref().map(|c| file_name(&c.path)).as_deref(), &suffix);
        if title != self.title {
            host.set_title(&title);
            host.set_subtitle(self.clip.as_ref().map(|c| c.readout.as_str()).unwrap_or(""));
            self.title = title;
        }
        // The video area, then the scrub row and the bar.
        let chrome = look::SCRUB_HEIGHT + look::BAR_HEIGHT;
        let area = egui::vec2(ui.available_width(), (ui.available_height() - chrome).max(1.0));
        let (rect, _) = ui.allocate_exact_size(area, egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);
        if let Some(drawn) = self.drawn {
            let checker = self.checker.get_or_insert_with(|| Checker::new(ui.ctx())).clone();
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::centered_and_justified(egui::Direction::TopDown)));
            let bg = if self.background == BackgroundMode::Checkerboard { Background::Checkerboard } else { Background::Black };
            glance_ui::video(&mut child, drawn, bg, &checker);
        }
        if let Some(m) = &self.message {
            bar::not_supported(ui, rect, m, look::NOT_SUPPORTED_CAPTION);
        }
        let (scrub, bar_state) = match self.clip.as_ref() {
            Some(c) => {
                let clock = c.player.clock();
                let time = glance_playback::seconds(glance_playback::ticks(clock.time_of_frame(clock.current())));
                let info = c.player.info();
                let alpha = match c.rule {
                    AlphaRule::Opaque => AlphaControl::None,
                    AlphaRule::Choosable => AlphaControl::Choosable {
                        current: c.alpha,
                        hint: c.hint.as_ref().map(|h| h.0.clone()),
                        samples: c.hint.as_ref().map_or(0, |h| h.1),
                        share: c.hint.as_ref().map_or(0.0, |h| h.2),
                    },
                    AlphaRule::Stated(v) => AlphaControl::Stated { view: v, by: info.codec_name.clone() },
                };
                (
                    ScrubState { time, duration: f64::from(info.frames) / info.rate.as_f64().max(1e-9), frame: clock.current() },
                    BarState {
                        paused: clock.is_paused(),
                        looping: clock.is_looping(),
                        muted: self.muted,
                        volume: self.volume,
                        has_siblings: false,
                        alpha,
                        readout: c.readout.clone(),
                    },
                )
            }
            None => (
                ScrubState { time: 0.0, duration: 0.0, frame: 0 },
                BarState {
                    paused: true,
                    looping: true,
                    muted: self.muted,
                    volume: self.volume,
                    has_siblings: false,
                    alpha: AlphaControl::None,
                    readout: String::new(),
                },
            ),
        };
        for a in bar::scrub_row(ui, &scrub, &mut self.scrub) {
            if let Some(c) = self.clip.as_mut() {
                let clock = c.player.clock_mut();
                match a {
                    ScrubAction::Begin => clock.begin_scrub(),
                    ScrubAction::SeekTo(t) => {
                        let f = clock.clip().nearest_frame(glance_media::Tick::from_units((t * 1e9) as i64, 1_000_000_000));
                        clock.scrub_to(f);
                    }
                    ScrubAction::End => clock.end_scrub(),
                }
            }
        }
        for a in bar::transport_bar(ui, &bar_state) {
            self.bar_action(a, host);
        }
    }
}

/// The media keys: macOS needs nothing; Windows binds them to the window.
#[cfg(target_os = "macos")]
fn register_media(_host: &Host<'_>) -> Result<glance_platform::MediaKeys, glance_platform::PlatformError> {
    glance_platform::MediaKeys::register()
}

#[cfg(windows)]
fn register_media(host: &Host<'_>) -> Result<glance_platform::MediaKeys, glance_platform::PlatformError> {
    match host.raw_window_handle() {
        Some(glance_ui::RawWindowHandle::Win32(h)) => glance_platform::MediaKeys::register(h.hwnd.get()),
        _ => Err(glance_platform::PlatformError::Unsupported("media keys without a window")),
    }
}

pub fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = |r: Result<(), String>| match r {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("glance-sample: {e}");
            std::process::ExitCode::FAILURE
        }
    };
    if args.first().map(String::as_str) == Some("--toggle-background") {
        return code(glance_settings::app_store(BUNDLE_ID, "Amritus", APP).map(|store| {
            let s = GlanceSettings::new(store);
            let next = if s.background() == BackgroundMode::Black { BackgroundMode::Checkerboard } else { BackgroundMode::Black };
            s.set_background(next);
            println!("{}", next.stored_name());
        }));
    }
    #[cfg(target_os = "macos")]
    if args.first().map(String::as_str) == Some("--set-default-test") {
        return code(set_default_test());
    }
    let first = args.iter().find(|a| !a.starts_with('-')).map(PathBuf::from);
    // Windows starts a process per double-click: hand the file to the running one and leave.
    #[cfg(windows)]
    let _lock = match glance_platform::SingleInstance::acquire(BUNDLE_ID, first.as_slice()) {
        Ok(glance_platform::SingleInstance::Handed) => return std::process::ExitCode::SUCCESS,
        Ok(glance_platform::SingleInstance::Primary(lock)) => lock,
        Err(e) => return code(Err(e.to_string())),
    };
    code(glance_ui::run_shell(
        ShellConfig { title: APP.into(), size: look::DEFAULT_SIZE, min_size: Some(look::MIN_SIZE), open_files: true, frame_autosave: None },
        move |gpu| Sample::new(gpu, first),
    ))
}

#[cfg(target_os = "macos")]
/// Ask for the default handler of the sample's own type and wait (up to 30 s) for the answer.
fn set_default_test() -> Result<(), String> {
    let before = glance_platform::is_default_for(SAMPLE_TYPE).map_err(|e| e.to_string())?;
    glance_platform::set_default_for(SAMPLE_TYPE).map_err(|e| e.to_string())?;
    let started = Instant::now();
    // The completion handler arrives on a background queue; the main thread only waits.
    while started.elapsed() < Duration::from_secs(30) {
        for e in glance_platform::take_events() {
            if let PlatformEvent::DefaultHandlerSet(r) = e {
                let after = glance_platform::is_default_for(SAMPLE_TYPE).map_err(|e| e.to_string())?;
                let line = serde_json::json!({"type": SAMPLE_TYPE, "was_default": before, "answer": format!("{r:?}"), "is_default": after});
                println!("{line}");
                log(serde_json::json!({"event": "set-default-test", "result": line}));
                return r;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("no answer from the OS in 30 s".into())
}
