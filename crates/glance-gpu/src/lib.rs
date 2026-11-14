//! # glance-gpu — a decoded frame to a picture the viewer can show
//!
//! DXVKit Plus uploads a frame's blocks into BC textures and converts them to RGBA **as stored**
//! (`dxvkit_plus_wgpu::FrameRenderer`). This crate adds the viewer's last step, [`Presenter`]: the
//! clip's colour read as premultiplied or straight ([`AlphaView`]) and written **premultiplied**,
//! ready to be blended over the viewer's background (`ONE, ONE_MINUS_SRC_ALPHA` — egui's blend, and
//! the Swift apps' GL blend: Straight multiplies the colour by its alpha in the shader, Premultiplied
//! passes it through). The per-clip Alpha Type switch is a uniform in this pass; it never re-decodes
//! or re-uploads a frame.
//!
//! One submission per shown frame: block upload, conversion and present pass share an encoder.
#![forbid(unsafe_code)]

use dxvkit_plus_wgpu::FrameRenderer;
use glance_media::{AlphaView, Decoded};

pub use dxvkit_plus_wgpu::{Gpu, GpuError, read_texture_rgba};

/// The present pass's output: premultiplied RGBA, values as shown (no sRGB conversion on write).
pub const VIEW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The clip premultiplied for the blend, texel for texel. `mode` 0: the colour is premultiplied
/// already (as stored); 1: straight (`rgb *= a`).
const PRESENT_WGSL: &str = r#"
struct U { mode: u32, pad0: u32, pad1: u32, pad2: u32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var src: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(src, vec2<i32>(pos.xy), 0);
    if (u.mode == 1u) {
        return vec4<f32>(c.rgb * c.a, c.a);
    }
    return c;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    mode: u32,
    pad: [u32; 3],
}

/// The present pipeline (one per device, shared by every clip).
#[derive(Debug)]
pub struct Presenter {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl Presenter {
    pub fn new(device: &wgpu::Device) -> Presenter {
        let shader =
            device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("glance-present"), source: wgpu::ShaderSource::Wgsl(PRESENT_WGSL.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glance-present"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glance-present"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glance-present"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(VIEW_FORMAT.into())],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Presenter { pipeline, layout }
    }
}

/// One clip's GPU state: DXVKit Plus's renderer (upload textures + as-stored RGBA) and the shown
/// picture. Nothing is reallocated while the clip's size stays the same.
pub struct ClipView {
    renderer: FrameRenderer,
    uniform: wgpu::Buffer,
    /// The as-stored texture the bind group was built for.
    bound: Option<(wgpu::Texture, wgpu::BindGroup)>,
    target: Option<wgpu::Texture>,
    #[cfg(target_os = "macos")]
    importer: Option<dxvkit_plus_apple::surface::TextureCacheImporter>,
}

impl ClipView {
    pub fn new(device: &wgpu::Device) -> ClipView {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glance-present"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ClipView {
            renderer: FrameRenderer::new(device),
            uniform,
            bound: None,
            target: None,
            #[cfg(target_os = "macos")]
            importer: None,
        }
    }

    /// The picture last shown ([`VIEW_FORMAT`], the clip's size).
    pub fn picture(&self) -> Option<&wgpu::Texture> {
        self.target.as_ref()
    }

    /// Upload `frame`, convert it and premultiply it as `alpha` says; returns the picture.
    pub fn show(&mut self, gpu: &Gpu, presenter: &Presenter, frame: &Decoded, alpha: AlphaView) -> Result<&wgpu::Texture, GpuError> {
        let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("glance-show") });
        let stored = match frame {
            Decoded::Planes { .. } => {
                self.renderer.upload(&gpu.device, &gpu.queue, frame)?;
                self.renderer.encode_convert(&gpu.device, &gpu.queue, &mut enc, None)?.clone()
            }
            Decoded::Native(native) => self.render_native(gpu, native)?,
        };
        self.present(gpu, presenter, &mut enc, &stored, alpha);
        gpu.queue.submit([enc.finish()]);
        self.target.as_ref().ok_or_else(|| GpuError::Invalid("no picture".into()))
    }

    /// The frame already on the GPU read with another Alpha Type: no decode, no upload.
    pub fn relook(&mut self, gpu: &Gpu, presenter: &Presenter, alpha: AlphaView) -> Result<&wgpu::Texture, GpuError> {
        let stored = self.renderer.output().ok_or_else(|| GpuError::Invalid("relook before a frame was shown".into()))?.clone();
        let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("glance-relook") });
        self.present(gpu, presenter, &mut enc, &stored, alpha);
        gpu.queue.submit([enc.finish()]);
        self.target.as_ref().ok_or_else(|| GpuError::Invalid("no picture".into()))
    }

    #[cfg(target_os = "macos")]
    fn render_native(&mut self, gpu: &Gpu, native: &glance_media::NativeFrame) -> Result<wgpu::Texture, GpuError> {
        use dxvkit_plus_apple::surface::TextureCacheImporter;
        let platform = |e: glance_media::MediaError| GpuError::Device(e.to_string());
        if self.importer.is_none() {
            self.importer = Some(TextureCacheImporter::new(&gpu.device).map_err(platform)?);
        }
        let importer = self.importer.as_ref().ok_or_else(|| GpuError::Invalid("no importer".into()))?;
        let textures: Vec<wgpu::Texture> = importer.import(&gpu.device, native).map_err(platform)?.into_iter().map(|p| p.texture).collect();
        let planes =
            dxvkit_plus_wgpu::NativePlanes { textures: &textures, layout: native.layout, matrix: native.matrix, width: native.width, height: native.height };
        Ok(self.renderer.render_native(&gpu.device, &gpu.queue, &planes)?.clone())
    }

    #[cfg(not(target_os = "macos"))]
    fn render_native(&mut self, _gpu: &Gpu, _native: &glance_media::NativeFrame) -> Result<wgpu::Texture, GpuError> {
        Err(GpuError::Unsupported("native frames need a platform backend (Windows: DXVKit Plus's Media Foundation backend, not built yet)".into()))
    }

    fn present(&mut self, gpu: &Gpu, presenter: &Presenter, enc: &mut wgpu::CommandEncoder, stored: &wgpu::Texture, alpha: AlphaView) {
        gpu.queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&Uniform { mode: u32::from(alpha == AlphaView::Straight), pad: [0; 3] }));
        if self.bound.as_ref().is_none_or(|(t, _)| t != stored) {
            let view = stored.create_view(&wgpu::TextureViewDescriptor::default());
            let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glance-present"),
                layout: &presenter.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                ],
            });
            self.bound = Some((stored.clone(), group));
        }
        if self.target.as_ref().is_none_or(|t| (t.width(), t.height()) != (stored.width(), stored.height())) {
            self.target = Some(gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("glance-picture"),
                size: wgpu::Extent3d { width: stored.width(), height: stored.height(), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: VIEW_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            }));
        }
        let (Some(target), Some((_, group))) = (&self.target, &self.bound) else { return };
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("glance-present"),
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
        pass.set_pipeline(&presenter.pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}
