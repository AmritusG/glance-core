//! The 0-difference render harness: a clip through the core's own pipeline (route, open, decode,
//! [`ClipView::show`], read back) against the CPU reference ([`expected_picture`]), frame by frame,
//! for each Alpha Type the clip can be read with. The tests run it over the committed fixtures and
//! over generated movies of every variant and size; `glance-proof verify` runs it over any file.

use crate::{ClipView, Gpu, GpuError, Presenter, expected_picture, present_on_cpu, read_texture_rgba};
use dxvkit_plus_wgpu::{BlockPath, FrameRenderer};
use glance_media::{AlphaRule, AlphaView, Clip, Decoded, alpha_rule};

/// The device for a GPU test: `None`, with a line saying why, on a machine without a GPU or when
/// `GLANCE_SKIP_GPU=1` (a CI runner whose GPU is not the product's); an error instead when
/// `GLANCE_REQUIRE_GPU=1` and there is none, so a gate cannot pass by skipping.
pub fn test_gpu() -> Result<Option<Gpu>, GpuError> {
    let set = |name: &str| std::env::var(name).as_deref() == Ok("1");
    if set("GLANCE_SKIP_GPU") {
        eprintln!("SKIP: GLANCE_SKIP_GPU=1");
        return Ok(None);
    }
    match crate::open_gpu() {
        Ok(gpu) => Ok(Some(gpu)),
        Err(e) if set("GLANCE_REQUIRE_GPU") => Err(e),
        Err(e) => {
            eprintln!("SKIP: no GPU ({e})");
            Ok(None)
        }
    }
}

/// What a picture was compared with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reference {
    /// DXVKit Plus's CPU decode (`Decoded::to_rgba`), read with the row's Alpha Type on the CPU.
    CpuDecode,
    /// DXVKit Plus's own GPU conversion: HAP HDR, which has no CPU decode.
    DxvkitPlusGpu,
    /// Nothing to compare with (frames of the OS decoder): the row only says the frames were shown.
    None,
}

/// One clip read with one Alpha Type.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Row {
    pub alpha: &'static str,
    pub reference: Reference,
    /// Frames shown and compared.
    pub frames: u32,
    /// Frames that did not decode (expected only for test vectors made to fail).
    pub undecodable: u32,
    pub texels: u64,
    /// The largest difference of any channel of any texel, in 8-bit code values.
    pub worst: u8,
    /// Texels with any difference.
    pub differing: u64,
    /// Whether any shown picture had a non-zero byte (a blank output is not a pass).
    pub nonblank: bool,
}

impl Row {
    /// Equal to the reference in every texel of every frame, and something was shown.
    pub fn exact(&self) -> bool {
        self.reference != Reference::None && self.frames > 0 && self.worst == 0 && self.differing == 0 && self.nonblank
    }
}

/// (largest channel difference, texels that differ).
pub fn diff(a: &[u8], b: &[u8]) -> (u8, u64) {
    if a == b {
        return (0, 0);
    }
    if a.len() != b.len() {
        return (255, (a.len().max(b.len()) / 4) as u64);
    }
    let worst = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
    (worst, a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0).filter(|(p, q)| p != q).count() as u64)
}

/// The Alpha Types a clip is checked with: both for a clip whose alpha is the user's choice and for
/// an opaque clip (which must not change), the stated one otherwise.
pub fn views(clip: &Clip) -> Vec<AlphaView> {
    match alpha_rule(&clip.info) {
        AlphaRule::Stated(view) => vec![view],
        AlphaRule::Choosable | AlphaRule::Opaque => vec![AlphaView::Premultiplied, AlphaView::Straight],
    }
}

/// Show every `step`-th frame of `clip` on `path` with each of its Alpha Types and compare.
pub fn clip_rows(gpu: &Gpu, presenter: &Presenter, clip: &Clip, path: BlockPath, step: u32) -> Result<Vec<Row>, String> {
    let mut source = clip.open().map_err(|e| format!("{e} ({})", e.cause))?;
    let views = views(clip);
    let mut view = ClipView::with_path(&gpu.device, path);
    let mut theirs: Option<FrameRenderer> = None;
    let name = |v: &AlphaView| if *v == AlphaView::Straight { "straight" } else { "premultiplied" };
    let mut rows: Vec<Row> = views
        .iter()
        .map(|v| Row { alpha: name(v), reference: Reference::None, frames: 0, undecodable: 0, texels: 0, worst: 0, differing: 0, nonblank: false })
        .collect();
    let gpu_error = |e: GpuError| e.to_string();
    for index in (0..clip.info.frames).step_by(step.max(1) as usize) {
        let Ok(frame) = source.decode(index) else {
            rows.iter_mut().for_each(|r| r.undecodable = r.undecodable.saturating_add(1));
            continue;
        };
        for (i, (&alpha, row)) in views.iter().zip(&mut rows).enumerate() {
            let picture = if i == 0 { view.show(gpu, presenter, &frame, alpha) } else { view.relook(gpu, presenter, alpha) }.map_err(gpu_error)?.clone();
            let got = read_texture_rgba(&gpu.device, &gpu.queue, &picture).map_err(gpu_error)?;
            let want = match (expected_picture(&frame, alpha), &frame) {
                (Ok(want), _) => Some((want, Reference::CpuDecode)),
                (Err(_), Decoded::Planes { .. }) => {
                    let renderer = theirs.get_or_insert_with(|| FrameRenderer::new(&gpu.device));
                    let stored = renderer.render(&gpu.device, &gpu.queue, &frame).map_err(gpu_error)?.clone();
                    let mut want = read_texture_rgba(&gpu.device, &gpu.queue, &stored).map_err(gpu_error)?;
                    present_on_cpu(&mut want, &frame, alpha);
                    Some((want, Reference::DxvkitPlusGpu))
                }
                (Err(_), Decoded::Native(_)) => None,
            };
            if let Some((want, reference)) = want {
                let (worst, differing) = diff(&want, &got);
                row.reference = reference;
                row.worst = row.worst.max(worst);
                row.differing = row.differing.saturating_add(differing);
            }
            row.frames = row.frames.saturating_add(1);
            row.texels = row.texels.saturating_add((got.len() / 4) as u64);
            row.nonblank |= got.iter().any(|&b| b != 0);
        }
    }
    Ok(rows)
}
