//! The host without a window: one egui frame drawn offscreen and read back, for tests and for
//! `glance-proof verify`. It is the windowed host's own path — the same egui renderer, the same
//! [`crate::video`] widget, the same blend and the same hardware filter — into a texture instead of
//! a surface, so what is measured here is what a window shows.
//!
//! [`on_screen`] is the software model of that picture: the clip's premultiplied picture sampled
//! bilinearly at the screen pixel's centre and blended over the background.

use crate::{Background, CHECKER_CELL, CHECKER_GREYS, Checker, Host, Picture};
use egui::mutex::RwLock;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use glance_gpu::{Gpu, GpuError, read_texture_rgba};

/// The offscreen screen's format: 8-bit, no sRGB conversion on write, like the window's surface.
const SCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// An egui context and renderer on a device, with no window.
pub struct Stage {
    gpu: Gpu,
    ctx: egui::Context,
    renderer: RwLock<Renderer>,
    slot: Option<Picture>,
    checker: Option<Checker>,
}

impl Stage {
    pub fn new(gpu: &Gpu) -> Stage {
        let ctx = egui::Context::default();
        // The system font is always there on the systems the apps run on; without it the stage draws
        // no text (a picture-only screen is unaffected).
        let _ = crate::fonts::install(&ctx);
        crate::look::apply(&ctx);
        let options = RendererOptions { dithering: false, ..Default::default() };
        Stage { gpu: gpu.clone(), ctx, renderer: RwLock::new(Renderer::new(&gpu.device, SCREEN_FORMAT, options)), slot: None, checker: None }
    }

    pub fn gpu(&self) -> &Gpu {
        &self.gpu
    }

    /// A screen holding nothing but `picture` (a clip's picture from `glance_gpu::ClipView`), drawn
    /// by [`crate::video`] over `background` into all the space there is. Returns the screen as
    /// RGBA8 and the rectangle the picture got, `[left, top, right, bottom]` in physical pixels.
    pub fn show(&mut self, picture: &wgpu::Texture, screen: [u32; 2], pixels_per_point: f32, background: Background) -> Result<(Vec<u8>, [f32; 4]), GpuError> {
        let (mut slot, mut checker) = (self.slot.take(), self.checker.take());
        let mut rect = egui::Rect::NOTHING;
        let pixels = self.frame(screen, pixels_per_point, |ui, host| {
            let sized = host.picture(&mut slot, picture);
            let checker = checker.get_or_insert_with(|| Checker::new(ui.ctx()));
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| rect = crate::video(ui, sized, background, checker).rect);
        });
        (self.slot, self.checker) = (slot, checker);
        Ok((pixels?, [rect.left(), rect.top(), rect.right(), rect.bottom()].map(|v| v * pixels_per_point)))
    }

    /// Draw one frame of `ui` on a screen of `size` physical pixels at `pixels_per_point` (cleared
    /// to black) and read it back as RGBA8.
    pub fn frame(&mut self, size: [u32; 2], pixels_per_point: f32, mut ui: impl FnMut(&mut egui::Ui, &mut Host<'_>)) -> Result<Vec<u8>, GpuError> {
        let [width, height] = size.map(|v| v.max(1));
        let points = egui::vec2(width as f32, height as f32) / pixels_per_point;
        let mut raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, points)), ..Default::default() };
        raw.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(pixels_per_point);
        let Stage { gpu, ctx, renderer, .. } = self;
        let mut host = Host {
            gpu,
            renderer,
            last_interval: None,
            refresh_millihertz: None,
            window: None,
            quit: None,
            drop_at: None,
            screens: &[],
            output_screens: Vec::new(),
            output_request: None,
        };
        let mut out = ctx.run_ui(raw, |root| ui(root, &mut host));
        // Applied in full, then cleared: egui 0.36 panics when a `TexturesDelta` is dropped unapplied.
        let mut delta = std::mem::take(&mut out.textures_delta);
        let jobs = ctx.tessellate(out.shapes, out.pixels_per_point);
        let screen = ScreenDescriptor { size_in_pixels: [width, height], pixels_per_point: out.pixels_per_point };
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glance-stage"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCREEN_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("glance-stage") });
        let mut commands = {
            let mut r = renderer.write();
            for (id, deltas) in &delta.set {
                for d in deltas {
                    r.update_texture(&gpu.device, &gpu.queue, *id, d);
                }
            }
            r.update_buffers(&gpu.device, &gpu.queue, &mut enc, &jobs, &screen)
        };
        {
            let pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glance-stage"),
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
            renderer.read().render(&mut pass, &jobs, &screen);
        }
        commands.push(enc.finish());
        gpu.queue.submit(commands);
        let mut r = renderer.write();
        for id in &delta.free {
            r.free_texture(id);
        }
        drop(r);
        delta.clear();
        read_texture_rgba(&gpu.device, &gpu.queue, &target)
    }
}

/// What is behind the picture at a point of the picture's rectangle (in points from its top-left
/// corner): black, or the checkerboard — 8-point squares of grey 0.85 and 0.65, the lighter one first.
pub fn background_at(background: Background, x_points: f32, y_points: f32) -> f32 {
    match background {
        Background::Black => 0.0,
        Background::Checkerboard => {
            let cell = (x_points / CHECKER_CELL).floor() as i64 + (y_points / CHECKER_CELL).floor() as i64;
            f32::from(CHECKER_GREYS[usize::from(cell.rem_euclid(2) == 1)])
        }
    }
}

/// The software model of a clip on screen: for every screen pixel, R, G and B as numbers (not yet
/// rounded). `picture` is the clip's premultiplied RGBA8 picture of `size`; `rect` is where
/// [`crate::video`] put it, `[left, top, right, bottom]` in physical pixels. Inside the rectangle a
/// pixel is the picture sampled bilinearly at the pixel's centre (texel centres at half integers,
/// edges clamped, the sample position kept to 1/256 of a texel as GPUs do), blended over the
/// background (`colour + background * (1 - alpha)`); outside it the screen is black.
pub fn on_screen(picture: &[u8], size: (u32, u32), rect: [f32; 4], screen: [u32; 2], pixels_per_point: f32, background: Background) -> Vec<[f32; 3]> {
    let (w, h) = (size.0.max(1) as usize, size.1.max(1) as usize);
    let texel = |x: usize, y: usize| -> [f32; 4] {
        let at = (y.min(h - 1) * w + x.min(w - 1)) * 4;
        let mut t = [0.0; 4];
        for (t, v) in t.iter_mut().zip(picture.get(at..at + 4).unwrap_or(&[0; 4])) {
            *t = f32::from(*v);
        }
        t
    };
    let [left, top, right, bottom] = rect;
    // The sample position on one axis: the texel below it and the weight of the next one.
    let axis = |centre: f32, from: f32, to: f32, n: usize| -> (usize, f32) {
        let at = ((centre - from) / (to - from) * n as f32 - 0.5).clamp(0.0, n as f32 - 1.0);
        let at = (at * 256.0).round() / 256.0;
        (at.floor() as usize, at - at.floor())
    };
    let mut out = Vec::with_capacity(screen[0] as usize * screen[1] as usize);
    for py in 0..screen[1] {
        for px in 0..screen[0] {
            let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
            if cx < left || cx >= right || cy < top || cy >= bottom {
                out.push([0.0; 3]);
                continue;
            }
            let ((x, fx), (y, fy)) = (axis(cx, left, right, w), axis(cy, top, bottom, h));
            let (a, b, c, d) = (texel(x, y), texel(x + 1, y), texel(x, y + 1), texel(x + 1, y + 1));
            let mix = |i: usize| (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy;
            let behind = background_at(background, (cx - left) / pixels_per_point, (cy - top) / pixels_per_point) * (1.0 - mix(3) / 255.0);
            out.push([mix(0) + behind, mix(1) + behind, mix(2) + behind]);
        }
    }
    out
}

/// (largest difference between a screen and its model after rounding, pixels more than `allowed`
/// away), over R, G and B.
pub fn distance(screen: &[u8], model: &[[f32; 3]], allowed: f32) -> (f32, u64) {
    let (mut worst, mut beyond) = (0f32, 0u64);
    for (got, want) in screen.as_chunks::<4>().0.iter().zip(model) {
        let d = got.iter().zip(want).map(|(g, w)| (f32::from(*g) - w.round().clamp(0.0, 255.0)).abs()).fold(0f32, f32::max);
        worst = worst.max(d);
        beyond += u64::from(d > allowed);
    }
    (worst, beyond)
}
