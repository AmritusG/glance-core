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
#![forbid(unsafe_code)]

use egui_wgpu::{RenderState, RendererOptions, ScreenDescriptor, WgpuConfiguration, WgpuSetup, WgpuSetupExisting};
use glance_gpu::Gpu;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

/// The Glance look in egui. W0: egui's dark visuals; the Swift apps' colours, spacing and fonts are
/// carried over in the ui-kit wave (docs/00-ROADMAP.md).
pub fn apply_look(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
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
/// The checkerboard's greys: 0.85 and 0.65.
pub const CHECKER_GREYS: [u8; 2] = [217, 166];

/// The checkerboard as a repeating 2x2 texture (made once per context).
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

/// What the host runs: one call per display refresh.
pub trait App {
    fn frame(&mut self, ui: &mut egui::Ui, host: &mut Host<'_>);
    /// True once the app wants its window closed.
    fn finished(&self) -> bool {
        false
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
    render: &'a RenderState,
    /// Time between the last two presents (`None` on the first frames).
    pub last_interval: Option<Duration>,
    /// The display's refresh rate, when the OS reports one.
    pub refresh_millihertz: Option<u32>,
}

impl Host<'_> {
    /// `texture` as something egui draws, registered once per texture and kept in `slot`.
    pub fn picture(&mut self, slot: &mut Option<Picture>, texture: &wgpu::Texture) -> egui::load::SizedTexture {
        let size = egui::vec2(texture.width() as f32, texture.height() as f32);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut renderer = self.render.renderer.write();
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
}

impl<A: App> Running<A> {
    fn start(el: &ActiveEventLoop, title: &str, size: (f64, f64), make: impl FnOnce(&Gpu) -> Result<A, String>) -> Result<Running<A>, String> {
        let gpu = Gpu::headless().map_err(|e| e.to_string())?;
        if !gpu.has_bc() {
            return Err("this GPU has no BC texture support (TEXTURE_COMPRESSION_BC)".into());
        }
        let attrs = WindowAttributes::default().with_title(title).with_inner_size(winit::dpi::LogicalSize::new(size.0, size.1));
        let window = Arc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
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
        let config = wgpu::SurfaceConfiguration {
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
        surface.configure(&gpu.device, &config);
        let ctx = egui::Context::default();
        apply_look(&ctx);
        let max_side = usize::try_from(gpu.device.limits().max_texture_dimension_2d).ok();
        let input = egui_winit::State::new(ctx.clone(), egui::ViewportId::ROOT, &window, Some(window.scale_factor() as f32), None, max_side);
        let app = make(&gpu)?;
        window.focus_window();
        Ok(Running { window, gpu, surface, config, ctx, input, render, app, last_present: None, last_interval: None })
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
        let _ = self.input.on_window_event(&self.window, event);
    }

    fn redraw(&mut self) {
        let raw = self.input.take_egui_input(&self.window);
        let Running { window, gpu, render, app, ctx, last_interval, .. } = self;
        let mut host =
            Host { gpu, render, last_interval: *last_interval, refresh_millihertz: window.current_monitor().and_then(|m| m.refresh_rate_millihertz()) };
        let mut out = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| app.frame(ui, &mut host));
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

struct Shell<A, F> {
    title: String,
    size: (f64, f64),
    make: Option<F>,
    running: Option<Running<A>>,
    error: Option<String>,
}

impl<A: App, F: FnOnce(&Gpu) -> Result<A, String>> ApplicationHandler for Shell<A, F> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let Some(make) = self.make.take() else { return };
        match Running::start(el, &self.title, self.size, make) {
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
                if r.app.finished() {
                    el.exit();
                } else {
                    r.window.request_redraw();
                }
            }
            other => r.on_event(&other),
        }
    }
}

/// Open a window titled `title` (`size` in points), build the app with `make` once the GPU exists,
/// and run until the window closes or the app says it has finished. Must be called on the main
/// thread.
pub fn run<A: App>(title: &str, size: (f64, f64), make: impl FnOnce(&Gpu) -> Result<A, String>) -> Result<(), String> {
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    let mut shell = Shell { title: title.to_string(), size, make: Some(make), running: None, error: None };
    el.run_app(&mut shell).map_err(|e| e.to_string())?;
    shell.error.map_or(Ok(()), Err)
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
