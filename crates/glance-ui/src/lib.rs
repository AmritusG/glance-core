//! # glance-ui — the family's egui host
//!
//! egui on our own winit loop through egui-winit + egui-wgpu (no eframe), the way loombda-rs hosts
//! its editor: the app owns the device ([`Gpu`], with BC textures enabled) and hands it to egui as
//! an existing device, so a clip's picture is drawn by egui straight from its wgpu texture
//! ([`Host::picture`], zero copy).
//!
//! [`video`] draws a clip the way the Swift apps do: the background first (black, or the shared
//! checkerboard — 8 pt squares of grey 0.85 and 0.65, in window points, not clip pixels), then the
//! premultiplied picture blended over it, fitted to the space with its aspect kept.
//!
//! The window redraws continuously, paced by the display (`PresentMode::Fifo`). egui-wgpu's
//! dithering is off: it would add noise to a scaled picture, and a viewer shows what the file holds.
//!
//! The shell (C3): text in the system font ([`fonts`]); Glance's look ([`look`], [`icons`]), its
//! transport bar, scrub row and messages ([`bar`]); its menus ([`menu`], muda) and dialogs
//! ([`dialog`]); its window rules ([`window`]). [`run_shell`] opens the one window with a menu
//! bar, a minimum size and the OS's open-file events, and hands the app every [`ShellEvent`] — a
//! menu command, files sent by the OS, a file dropped on the window, a media key.
#![forbid(unsafe_code)]

pub mod bar;
#[cfg(any(target_os = "macos", windows))]
pub mod dialog;
pub mod fonts;
pub mod headless;
pub mod icons;
pub mod look;
#[cfg(any(target_os = "macos", windows))]
pub mod menu;
pub mod window;

pub use glance_platform::{MediaCommand, NowPlaying, PlatformEvent};
pub use winit::raw_window_handle::RawWindowHandle;

use egui::mutex::RwLock;
use egui_wgpu::{RenderState, Renderer, RendererOptions, ScreenDescriptor, WgpuConfiguration, WgpuSetup, WgpuSetupExisting};
use glance_gpu::Gpu;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};

/// The Glance look in egui ([`look::apply`]).
pub fn apply_look(ctx: &egui::Context) {
    look::apply(ctx);
}

/// What shows behind a clip (the Swift apps' `BackgroundMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Background {
    #[default]
    Black,
    Checkerboard,
}

/// Checkerboard squares, in points.
pub const CHECKER_CELL: f32 = 8.0;
/// The checkerboard's greys, 0.85 and 0.65 of 255, rounded: the lighter square first.
pub const CHECKER_GREYS: [u8; 2] = [217, 166];

/// The checkerboard as a repeating 2x2 texture (made once per context).
#[derive(Clone)]
pub struct Checker(egui::TextureHandle);

impl Checker {
    pub fn new(ctx: &egui::Context) -> Checker {
        let [light, dark] = CHECKER_GREYS.map(egui::Color32::from_gray);
        let image = egui::ColorImage::new([2, 2], vec![light, dark, dark, light]);
        let options = egui::TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..egui::TextureOptions::NEAREST };
        Checker(ctx.load_texture("glance-checker", image, options))
    }
}

/// The largest size with `picture`'s aspect that fits `space`.
pub fn fit(picture: egui::Vec2, space: egui::Vec2) -> egui::Vec2 {
    if picture.x <= 0.0 || picture.y <= 0.0 {
        return egui::Vec2::ZERO;
    }
    picture * (space.x / picture.x).min(space.y / picture.y).max(0.0)
}

/// Draw `picture` (premultiplied, from `glance_gpu::ClipView`) over `background` in the space left.
pub fn video(ui: &mut egui::Ui, picture: egui::load::SizedTexture, background: Background, checker: &Checker) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(fit(picture.size, ui.available_size()), egui::Sense::hover());
    let painter = ui.painter();
    match background {
        Background::Black => {
            painter.rect_filled(rect, 0.0, egui::Color32::BLACK);
        }
        Background::Checkerboard => {
            // One texture repeat is two squares.
            let uv = egui::Rect::from_min_size(egui::Pos2::ZERO, rect.size() / (2.0 * CHECKER_CELL));
            painter.image(checker.0.id(), rect, uv, egui::Color32::WHITE);
        }
    }
    painter.image(picture.id, rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    response
}

/// Something the OS or the window sent the app.
#[derive(Clone, Debug, PartialEq)]
pub enum ShellEvent {
    /// A menu item was chosen.
    #[cfg(any(target_os = "macos", windows))]
    Menu(menu::MenuCommand),
    /// Files sent by the OS (Finder, Open With, the Dock, a second launch on Windows), a media
    /// key, the answer to "Set as Default".
    Platform(PlatformEvent),
    /// The first file of a drop on the window (Swift opens the first dropped URL, any type).
    Dropped(PathBuf),
}

/// What the host runs: one call per display refresh, and one per event before it.
pub trait App {
    fn frame(&mut self, ui: &mut egui::Ui, host: &mut Host<'_>);
    /// True once the app wants its window closed.
    fn finished(&self) -> bool {
        false
    }
    /// An event, delivered before the next frame.
    fn event(&mut self, _event: ShellEvent, _host: &mut Host<'_>) {}
    /// The app's menu bar, attached once the window exists.
    #[cfg(any(target_os = "macos", windows))]
    fn menu(&self) -> Option<&menu::GlanceMenu> {
        None
    }
}

/// A wgpu texture egui can draw.
pub struct Picture {
    id: egui::TextureId,
    texture: wgpu::Texture,
}

/// What an [`App`] sees of the host during a frame.
pub struct Host<'a> {
    pub gpu: &'a Gpu,
    renderer: &'a RwLock<Renderer>,
    /// Time between the last two presents (`None` on the first frames).
    pub last_interval: Option<Duration>,
    /// The display's refresh rate, when the OS reports one.
    pub refresh_millihertz: Option<u32>,
    /// The window (none on the headless stage).
    window: Option<&'a Window>,
    quit: Option<&'a mut bool>,
}

impl Host<'_> {
    /// Close the window and end the app after this frame.
    pub fn quit(&mut self) {
        if let Some(q) = self.quit.as_deref_mut() {
            *q = true;
        }
    }

    /// The window title (`NSWindow.title`).
    pub fn set_title(&self, title: &str) {
        if let Some(w) = self.window {
            w.set_title(title);
        }
    }

    /// The grey line under the title (`NSWindow.subtitle`, macOS); elsewhere nothing (the bar's
    /// readout carries the same text, hard spot 10).
    pub fn set_subtitle(&self, text: &str) {
        #[cfg(target_os = "macos")]
        if let Some(h) = self.window.and_then(|w| w.window_handle().ok()) {
            let _ = glance_platform::set_subtitle(h.as_raw(), text);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = text;
    }

    /// The content area's size in points.
    pub fn content_size(&self) -> Option<(f64, f64)> {
        let w = self.window?;
        let size = w.inner_size().to_logical::<f64>(w.scale_factor());
        Some((size.width, size.height))
    }

    /// The width, in points, of the screen the window is on.
    pub fn screen_width(&self) -> Option<f64> {
        let w = self.window?;
        let m = w.current_monitor()?;
        Some(m.size().to_logical::<f64>(m.scale_factor()).width)
    }

    /// Swift's `fitWindowWidth` for a clip of `video` pixels: the height stays, the width follows
    /// the clip's aspect over the video area (the content less `bottom_chrome` points), clamped to
    /// the minimum and to 95 % of the screen ([`window::fit_width`]). Not in full screen.
    pub fn fit_width(&self, video: (u32, u32), bottom_chrome: f64) {
        let Some(w) = self.window else { return };
        if w.fullscreen().is_some() {
            return;
        }
        let (Some((_, height)), Some(screen)) = (self.content_size(), self.screen_width()) else { return };
        if let Some(width) = window::fit_width(height, 0.0, bottom_chrome, video, look::MIN_SIZE.0, screen) {
            let _ = w.request_inner_size(winit::dpi::LogicalSize::new(width, height));
        }
    }

    /// View ▸ Default Window Size: the default content size, centred on the screen.
    pub fn default_size(&self, size: (f64, f64)) {
        let Some(w) = self.window else { return };
        if w.fullscreen().is_some() {
            return;
        }
        let _ = w.request_inner_size(winit::dpi::LogicalSize::new(size.0, size.1));
        if let Some(m) = w.current_monitor() {
            let scale = m.scale_factor();
            let (mp, ms) = (m.position(), m.size());
            let outer = w.outer_size();
            let inner = w.inner_size();
            let chrome = outer.height.saturating_sub(inner.height);
            let (ow, oh) = ((size.0 * scale) as i32, (size.1 * scale) as i32 + chrome as i32);
            let x = mp.x + (ms.width as i32 - ow) / 2;
            let y = mp.y + (ms.height as i32 - oh) / 2;
            w.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
        }
    }

    /// Full screen on and off (macOS: the system's own full screen, as `toggleFullScreen:`).
    pub fn toggle_fullscreen(&self) {
        if let Some(w) = self.window {
            w.set_fullscreen(if w.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) });
        }
    }

    pub fn is_fullscreen(&self) -> bool {
        self.window.is_some_and(|w| w.fullscreen().is_some())
    }

    /// The window's raw handle, for platform calls (media controls on Windows).
    pub fn raw_window_handle(&self) -> Option<winit::raw_window_handle::RawWindowHandle> {
        self.window.and_then(|w| w.window_handle().ok()).map(|h| h.as_raw())
    }

    /// `texture` as something egui draws, registered once per texture and kept in `slot`.
    pub fn picture(&mut self, slot: &mut Option<Picture>, texture: &wgpu::Texture) -> egui::load::SizedTexture {
        let size = egui::vec2(texture.width() as f32, texture.height() as f32);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut renderer = self.renderer.write();
        let id = match slot {
            Some(p) if p.texture == *texture => p.id,
            Some(p) => {
                renderer.update_egui_texture_from_wgpu_texture(&self.gpu.device, &view, wgpu::FilterMode::Linear, p.id);
                p.texture = texture.clone();
                p.id
            }
            None => renderer.register_native_texture(&self.gpu.device, &view, wgpu::FilterMode::Linear),
        };
        if slot.is_none() {
            *slot = Some(Picture { id, texture: texture.clone() });
        }
        egui::load::SizedTexture { id, size }
    }
}

struct Running<A> {
    window: Arc<Window>,
    gpu: Gpu,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    ctx: egui::Context,
    input: egui_winit::State,
    render: RenderState,
    app: A,
    last_present: Option<Instant>,
    last_interval: Option<Duration>,
    /// Events waiting for the next frame.
    pending: Vec<ShellEvent>,
    /// A drop's first file was taken; the rest of that drop is ignored.
    drop_taken: bool,
    quit: bool,
}

impl<A: App> Running<A> {
    fn start(el: &ActiveEventLoop, config: &ShellConfig, make: impl FnOnce(&Gpu) -> Result<A, String>) -> Result<Running<A>, String> {
        let gpu = glance_gpu::open_gpu().map_err(|e| e.to_string())?;
        if !gpu.has_bc() {
            return Err("this GPU has no BC texture support (TEXTURE_COMPRESSION_BC)".into());
        }
        let mut attrs =
            WindowAttributes::default().with_title(config.title.as_str()).with_inner_size(winit::dpi::LogicalSize::new(config.size.0, config.size.1));
        if let Some((w, h)) = config.min_size {
            attrs = attrs.with_min_inner_size(winit::dpi::LogicalSize::new(w, h));
        }
        let window = Arc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
        #[cfg(target_os = "macos")]
        {
            let handle = window.window_handle().map_err(|e| e.to_string())?;
            glance_platform::set_single_window(handle.as_raw()).map_err(|e| e.to_string())?;
        }
        let surface = gpu.instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let setup = WgpuConfiguration {
            wgpu_setup: WgpuSetup::Existing(WgpuSetupExisting {
                instance: gpu.instance.clone(),
                adapter: gpu.adapter.clone(),
                device: gpu.device.clone(),
                queue: gpu.queue.clone(),
            }),
            ..Default::default()
        };
        let options = RendererOptions { dithering: false, ..Default::default() };
        let render = pollster::block_on(RenderState::create(&setup, &gpu.instance, Some(&surface), options)).map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&gpu.adapter);
        let phys = window.inner_size();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: render.target_format,
            width: phys.width.max(1),
            height: phys.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&gpu.device, &surface_config);
        let ctx = egui::Context::default();
        fonts::install(&ctx)?;
        look::apply(&ctx);
        let max_side = usize::try_from(gpu.device.limits().max_texture_dimension_2d).ok();
        let input = egui_winit::State::new(ctx.clone(), egui::ViewportId::ROOT, &window, Some(window.scale_factor() as f32), None, max_side);
        let app = make(&gpu)?;
        #[cfg(target_os = "macos")]
        if let Some(menu) = app.menu() {
            menu.attach_to_app()?;
        }
        #[cfg(windows)]
        if let (Some(menu), Ok(handle)) = (app.menu(), window.window_handle()) {
            glance_platform::attach_menu(&menu.menu, handle.as_raw()).map_err(|e| e.to_string())?;
        }
        window.focus_window();
        Ok(Running {
            window,
            gpu,
            surface,
            config: surface_config,
            ctx,
            input,
            render,
            app,
            last_present: None,
            last_interval: None,
            pending: Vec::new(),
            drop_taken: false,
            quit: false,
        })
    }

    fn on_event(&mut self, event: &WindowEvent) {
        if let WindowEvent::Resized(size) = event {
            let (w, h) = (size.width.max(1), size.height.max(1));
            if (w, h) != (self.config.width, self.config.height) {
                self.config.width = w;
                self.config.height = h;
                self.surface.configure(&self.gpu.device, &self.config);
            }
        }
        match event {
            WindowEvent::HoveredFile(_) => self.drop_taken = false,
            WindowEvent::DroppedFile(path) if !self.drop_taken => {
                self.drop_taken = true;
                self.pending.push(ShellEvent::Dropped(path.clone()));
            }
            _ => {}
        }
        let _ = self.input.on_window_event(&self.window, event);
    }

    fn redraw(&mut self) {
        #[cfg(any(target_os = "macos", windows))]
        self.pending.extend(menu::GlanceMenu::take_commands().into_iter().map(ShellEvent::Menu));
        self.pending.extend(glance_platform::take_events().into_iter().map(ShellEvent::Platform));
        let raw = self.input.take_egui_input(&self.window);
        let Running { window, gpu, render, app, ctx, last_interval, pending, quit, .. } = self;
        let refresh_millihertz = window.current_monitor().and_then(|m| m.refresh_rate_millihertz());
        let mut host = Host { gpu, renderer: &render.renderer, last_interval: *last_interval, refresh_millihertz, window: Some(window), quit: Some(quit) };
        for event in pending.drain(..) {
            app.event(event, &mut host);
        }
        let mut out = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::BLACK)).show(ui, |ui| app.frame(ui, &mut host));
        });
        self.input.handle_platform_output(&self.window, out.platform_output);
        let ppp = out.pixels_per_point;
        let jobs = self.ctx.tessellate(out.shapes, ppp);
        let screen = ScreenDescriptor { size_in_pixels: [self.config.width, self.config.height], pixels_per_point: ppp };
        // Texture uploads and frees are applied every frame, presented or not: egui 0.36 panics
        // when a `TexturesDelta` is dropped unapplied (loombda-rs found it live).
        let mut delta = std::mem::take(&mut out.textures_delta);
        {
            let mut r = self.render.renderer.write();
            for (id, deltas) in &delta.set {
                for d in deltas {
                    r.update_texture(&self.gpu.device, &self.gpu.queue, *id, d);
                }
            }
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                None
            }
            _ => None,
        };
        if let Some(frame) = frame {
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut enc = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("glance-ui") });
            let mut commands = self.render.renderer.write().update_buffers(&self.gpu.device, &self.gpu.queue, &mut enc, &jobs, &screen);
            {
                let pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("glance-ui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                let mut pass = pass.forget_lifetime();
                self.render.renderer.read().render(&mut pass, &jobs, &screen);
            }
            commands.push(enc.finish());
            self.gpu.queue.submit(commands);
            self.window.pre_present_notify();
            self.gpu.queue.present(frame);
            let now = Instant::now();
            self.last_interval = self.last_present.map(|t| now.duration_since(t));
            self.last_present = Some(now);
        }
        let _ = self.gpu.device.poll(wgpu::PollType::Poll);
        let mut r = self.render.renderer.write();
        for id in &delta.free {
            r.free_texture(id);
        }
        delta.clear();
    }
}

struct Runner<A, F> {
    config: ShellConfig,
    make: Option<F>,
    running: Option<Running<A>>,
    error: Option<String>,
}

impl<A: App, F: FnOnce(&Gpu) -> Result<A, String>> ApplicationHandler for Runner<A, F> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let Some(make) = self.make.take() else { return };
        match Running::start(el, &self.config, make) {
            Ok(r) => {
                r.window.request_redraw();
                self.running = Some(r);
            }
            Err(e) => {
                self.error = Some(e);
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(r) = self.running.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => {
                r.redraw();
                if r.app.finished() || r.quit {
                    el.exit();
                } else {
                    r.window.request_redraw();
                }
            }
            other => r.on_event(&other),
        }
    }

    /// An OS event was queued ([`glance_platform::set_waker`]): draw, which delivers it.
    fn user_event(&mut self, _el: &ActiveEventLoop, _event: ()) {
        if let Some(r) = self.running.as_ref() {
            r.window.request_redraw();
        }
    }
}

/// The one window of an app.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellConfig {
    pub title: String,
    /// Content size in points.
    pub size: (f64, f64),
    /// Minimum content size in points.
    pub min_size: Option<(f64, f64)>,
    /// Take the files the OS sends ([`glance_platform::install_open_handler`] on macOS).
    pub open_files: bool,
}

/// Open a window titled `title` (`size` in points), build the app with `make` once the GPU exists,
/// and run until the window closes or the app says it has finished. Must be called on the main
/// thread.
pub fn run<A: App>(title: &str, size: (f64, f64), make: impl FnOnce(&Gpu) -> Result<A, String>) -> Result<(), String> {
    run_shell(ShellConfig { title: title.into(), size, min_size: None, open_files: false }, make)
}

/// [`run`] with the shell: a minimum size, and with `open_files` the files the OS sends, delivered
/// as [`ShellEvent::Platform`] — at cold launch too (they wait for the first frame). Closing the
/// window ends the app (Swift Glance: `applicationShouldTerminateAfterLastWindowClosed`). On macOS
/// menu shortcuts without a modifier (Space, A) go to the menu, not the window
/// ([`glance_platform::install_menu_key_monitor`]), and the window is Swift Glance's single
/// window: never tabbed, not listed in the Window menu.
pub fn run_shell<A: App>(config: ShellConfig, make: impl FnOnce(&Gpu) -> Result<A, String>) -> Result<(), String> {
    let el = EventLoop::<()>::with_user_event().build().map_err(|e| e.to_string())?;
    let proxy: EventLoopProxy<()> = el.create_proxy();
    glance_platform::set_waker(move || {
        let _ = proxy.send_event(());
    });
    #[cfg(target_os = "macos")]
    {
        if config.open_files {
            glance_platform::install_open_handler().map_err(|e| e.to_string())?;
        }
        glance_platform::install_menu_key_monitor().map_err(|e| e.to_string())?;
    }
    let mut runner = Runner { config, make: Some(make), running: None, error: None };
    el.run_app(&mut runner).map_err(|e| e.to_string())?;
    runner.error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_fits_the_space_with_its_aspect_kept() {
        // Wide space: height limits. Tall space: width limits.
        assert_eq!(fit(egui::vec2(1920.0, 1080.0), egui::vec2(1000.0, 270.0)), egui::vec2(480.0, 270.0));
        assert_eq!(fit(egui::vec2(1920.0, 1080.0), egui::vec2(960.0, 2000.0)), egui::vec2(960.0, 540.0));
        assert_eq!(fit(egui::vec2(0.0, 1080.0), egui::vec2(960.0, 2000.0)), egui::Vec2::ZERO);
    }
}
