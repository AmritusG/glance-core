//! The show output: a second, app-owned window on its **own** wgpu surface and pacing (never an egui viewport;
//! loombda-rs law 4), borderless full screen on a chosen screen, cursor hidden over it. The app draws one picture
//! per output refresh ([`crate::App::output`]); this module only puts that picture on the screen.
//!
//! Screens are looked at once a second: when the set changes the app hears [`crate::ShellEvent::Screens`], and when
//! the output's screen goes away the output closes and the app hears [`crate::ShellEvent::OutputClosed`] (the app
//! remembers the screen and opens the output again when it comes back).

use glance_gpu::Gpu;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
#[cfg(not(target_os = "macos"))]
use winit::window::Fullscreen;
use winit::window::{Window, WindowAttributes};

/// A screen the OS reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    /// Name and place: unique among the screens attached now, and the same after a replug in the same arrangement.
    pub id: String,
    /// The OS's name for it ("DELL U2720Q", "Color LCD").
    pub name: String,
    /// Pixels.
    pub size: (u32, u32),
    /// Top-left in the desktop, pixels.
    pub position: (i32, i32),
    pub refresh_millihertz: Option<u32>,
    /// The screen the app's window is on.
    pub has_main_window: bool,
}

pub(crate) fn screen_of(m: &MonitorHandle, main: Option<&MonitorHandle>, names: &[(u32, String)]) -> Screen {
    let native = native_id(m);
    let name = native.and_then(|n| names.iter().find(|(id, _)| *id == n).map(|(_, s)| s.clone())).or_else(|| m.name()).unwrap_or_else(|| "Screen".into());
    let (p, s) = (m.position(), m.size());
    Screen {
        // The OS's id for the screen where it has one (macOS: the display id, kept across a replug of the same
        // screen), else the place; the name in front for people.
        id: match native {
            Some(n) => format!("{name}#{n}"),
            None => format!("{name}@{},{}", p.x, p.y),
        },
        name,
        size: (s.width, s.height),
        position: (p.x, p.y),
        refresh_millihertz: m.refresh_rate_millihertz(),
        has_main_window: main == Some(m),
    }
}

#[cfg(target_os = "macos")]
fn native_id(m: &MonitorHandle) -> Option<u32> {
    use winit::platform::macos::MonitorHandleExtMacOS;
    Some(m.native_id())
}

#[cfg(not(target_os = "macos"))]
fn native_id(_m: &MonitorHandle) -> Option<u32> {
    None
}

/// The OS's names for the screens (macOS: NSScreen's; elsewhere winit's names are the OS's already).
pub(crate) fn screen_names() -> Vec<(u32, String)> {
    #[cfg(target_os = "macos")]
    {
        glance_platform::screen_names().unwrap_or_default()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

/// Which of `screens` an app's remembered `wanted` means: the same id; else the same name and size; else the same
/// name (a screen replugged elsewhere, or after the OS renumbered it).
pub fn find_screen<'a>(screens: &'a [Screen], wanted: &str) -> Option<&'a Screen> {
    let name = wanted.split(['#', '@']).next().unwrap_or(wanted);
    screens.iter().find(|s| s.id == wanted).or_else(|| screens.iter().find(|s| s.name == name))
}

/// What the app gets when it draws an output.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputFrame {
    /// The screen this output is on (its [`Screen::id`]): with several outputs, which one to draw (v0.9.0).
    pub screen: String,
    /// The output's size in pixels: draw a picture this size.
    pub size: (u32, u32),
    pub refresh_millihertz: Option<u32>,
    /// Time between the output's last two presents.
    pub last_interval: Option<Duration>,
    /// When the picture drawn now will be on the screen (about one refresh ahead).
    pub shown_at: Instant,
}

/// Why the output closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputEnd {
    /// Its screen was unplugged (or turned off).
    ScreenGone(String),
    /// The app closed it.
    Closed,
}

const BLIT_WGSL: &str = r#"
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs(@builtin(vertex_index) i: u32) -> V {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var v: V;
    v.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    v.uv = vec2<f32>(p.x, 1.0 - p.y);
    return v;
}
@fragment
fn fs(v: V) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSampleLevel(src, smp, v.uv, 0.0).rgb, 1.0);
}
"#;

pub(crate) struct Output {
    pub(crate) window: Arc<Window>,
    pub(crate) screen: Screen,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    bound: Option<(wgpu::Texture, wgpu::BindGroup)>,
    last_present: Option<Instant>,
    last_interval: Option<Duration>,
    /// Its frames when its screen does not pace them ([`crate::pace`]).
    pub(crate) pace: crate::pace::Pacer,
    /// The OS says it is fully covered.
    pub(crate) occluded: bool,
}

impl Output {
    pub(crate) fn open(el: &ActiveEventLoop, gpu: &Gpu, monitor: MonitorHandle, screen: Screen) -> Result<Output, String> {
        let attrs = WindowAttributes::default().with_title("Output").with_decorations(false).with_position(monitor.position()).with_inner_size(monitor.size());
        // macOS: an undecorated window over the whole screen at the show level (glance_platform::set_show_window),
        // not winit's borderless full screen, which puts its own window level back and leaves the menu bar over the
        // show on a second screen. Elsewhere: borderless full screen on that monitor.
        #[cfg(not(target_os = "macos"))]
        let attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(Some(monitor))));
        #[cfg(target_os = "macos")]
        let _ = monitor;
        let window = Arc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
        window.set_cursor_visible(false);
        #[cfg(target_os = "macos")]
        {
            use winit::raw_window_handle::HasWindowHandle;
            let handle = window.window_handle().map_err(|e| e.to_string())?;
            glance_platform::set_show_window(handle.as_raw()).map_err(|e| e.to_string())?;
        }
        let surface = gpu.instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&gpu.adapter);
        let format =
            caps.formats.iter().copied().find(|f| !f.is_srgb()).or_else(|| caps.formats.first().copied()).ok_or("the output screen has no surface format")?;
        let phys = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: phys.width.max(1),
            height: phys.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&gpu.device, &config);
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glance-output"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glance-output"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader =
            device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("glance-output"), source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()) });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glance-output"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glance-output"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Ok(Output {
            window,
            screen,
            surface,
            config,
            pipeline,
            layout,
            sampler,
            bound: None,
            last_present: None,
            last_interval: None,
            pace: Default::default(),
            occluded: false,
        })
    }

    pub(crate) fn resized(&mut self, gpu: &Gpu, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        if (w, h) != (self.config.width, self.config.height) {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(&gpu.device, &self.config);
        }
    }

    pub(crate) fn frame_info(&self) -> OutputFrame {
        let refresh = self.window.current_monitor().and_then(|m| m.refresh_rate_millihertz());
        let ahead = refresh.map_or(Duration::from_millis(16), |mhz| Duration::from_secs_f64(1000.0 / f64::from(mhz.max(1))));
        OutputFrame {
            screen: self.screen.id.clone(),
            size: (self.config.width, self.config.height),
            refresh_millihertz: refresh,
            last_interval: self.last_interval,
            shown_at: Instant::now() + ahead,
        }
    }

    /// Put `picture` on the screen, stretched to it (the app draws it at the output's size), or black; `false` when no
    /// drawable came (nothing was presented).
    pub(crate) fn present(&mut self, gpu: &Gpu, picture: Option<&wgpu::Texture>) -> bool {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&gpu.device, &self.config);
                return false;
            }
            _ => return false,
        };
        if let Some(t) = picture
            && self.bound.as_ref().is_none_or(|(b, _)| b != t)
        {
            let view = t.create_view(&wgpu::TextureViewDescriptor::default());
            let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glance-output"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            self.bound = Some((t.clone(), group));
        }
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("glance-output") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glance-output"),
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
            if let (Some(_), Some((_, group))) = (picture, &self.bound) {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        gpu.queue.submit([enc.finish()]);
        self.window.pre_present_notify();
        gpu.queue.present(frame);
        let now = Instant::now();
        self.last_interval = self.last_present.map(|t| now.duration_since(t));
        self.last_present = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(id: &str, size: (u32, u32)) -> Screen {
        let name = id.split(['#', '@']).next().unwrap_or(id).to_string();
        Screen { id: id.into(), name, size, position: (0, 0), refresh_millihertz: None, has_main_window: false }
    }

    #[test]
    fn a_remembered_screen_is_found_by_id_then_by_name_after_a_replug() {
        let now = vec![screen("Built-in Display#1", (3456, 2234)), screen("DELL U2720Q#10", (3840, 2160))];
        assert_eq!(find_screen(&now, "DELL U2720Q#10").map(|s| s.id.as_str()), Some("DELL U2720Q#10"));
        // The OS renumbered it after the replug: the name finds it.
        assert_eq!(find_screen(&now, "DELL U2720Q#9").map(|s| s.id.as_str()), Some("DELL U2720Q#10"));
        assert_eq!(find_screen(&now, "DELL U2720Q").map(|s| s.id.as_str()), Some("DELL U2720Q#10"));
        assert!(find_screen(&now, "Projector#3").is_none());
    }
}
