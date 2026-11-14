//! `glance-proof verify`: the 0-difference render harness over any files, without a window. Every
//! `--every`-th frame of each clip goes through the core's pipeline and is compared at 1:1 with the
//! CPU reference, for each Alpha Type; one frame per clip is also drawn window-scaled (1.5x and
//! 0.5x) through the egui widget and compared with the software bilinear model.

use glance_gpu::verify::{Reference, Row, clip_rows};
use glance_gpu::{BlockPath, ClipView, Gpu, Presenter, expected_picture};
use glance_media::{AlphaView, Route};
use glance_ui::Background;
use glance_ui::headless::{Stage, distance, on_screen};
use std::path::{Path, PathBuf};

/// One frame of a clip on a scaled screen against the model.
#[derive(Debug, serde::Serialize)]
pub struct Scaled {
    pub alpha: &'static str,
    pub scale: f32,
    pub frame: u32,
    /// The largest distance from the model after rounding, in 8-bit code values.
    pub worst: f32,
    /// Pixels more than 1 away.
    pub beyond_1: u64,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct FileReport {
    pub file: String,
    /// `dxv`, `hap`, `native`, `lmbd`, or `none` with the line the viewer shows.
    pub route: &'static str,
    pub message: Option<String>,
    pub codec: String,
    /// The DXV variant, from the middle frame's planes (a DXV track's FourCC does not say).
    pub variant: Option<&'static str>,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub every: u32,
    pub one_to_one: Vec<Row>,
    pub scaled: Vec<Scaled>,
    pub error: Option<String>,
    pub seconds: f64,
}

#[derive(Debug, serde::Serialize)]
pub struct Report {
    pub build: &'static str,
    pub adapter: String,
    pub block_path: &'static str,
    pub files: Vec<FileReport>,
}

/// The movie files of `paths` (folders are walked), sorted.
pub fn movies(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(p: &Path, out: &mut Vec<PathBuf>) {
        if p.is_dir() {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(p).map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
            entries.sort();
            entries.iter().for_each(|e| walk(e, out));
        } else if p.extension().and_then(|e| e.to_str()).is_some_and(|e| ["mov", "mp4", "mkv", "webm", "avi"].contains(&e.to_lowercase().as_str())) {
            out.push(p.to_path_buf());
        }
    }
    let mut out = Vec::new();
    paths.iter().for_each(|p| if p.is_dir() { walk(p, &mut out) } else { out.push(p.clone()) });
    out
}

fn file_report(gpu: &Gpu, presenter: &Presenter, stage: &mut Stage, path: &Path, name: String, every: u32) -> FileReport {
    let started = std::time::Instant::now();
    let mut report = FileReport { file: name, every, route: "none", ..Default::default() };
    let clip = match glance_media::route(path) {
        Ok(clip) => clip,
        Err(e) => {
            report.message = Some(e.message);
            return report;
        }
    };
    report.route = match clip.route {
        Route::Dxv => "dxv",
        Route::Hap => "hap",
        Route::Native => "native",
        Route::Lmbd => "lmbd",
    };
    (report.codec, report.width, report.height, report.frames) = (glance_media::codec_label(&clip.info), clip.info.width, clip.info.height, clip.info.frames);
    match clip_rows(gpu, presenter, &clip, BlockPath::Exact, every) {
        Ok(rows) => report.one_to_one = rows,
        Err(e) => report.error = Some(e),
    }
    // The middle frame, window-scaled, for each Alpha Type the clip is read with.
    let middle = clip.info.frames / 2;
    if let Ok(frame) = clip.open().map_err(|e| e.to_string()).and_then(|mut s| s.decode(middle).map_err(|e| e.to_string())) {
        report.variant = if clip.route == Route::Dxv { glance_media::dxv_variant_label(&frame) } else { None };
        let mut view = ClipView::new(&gpu.device);
        for alpha in glance_gpu::verify::views(&clip) {
            let Ok(reference) = expected_picture(&frame, alpha) else { continue };
            for scale in [1.5f32, 0.5] {
                let screen = [((clip.info.width as f32 * scale).round() as u32).max(1), ((clip.info.height as f32 * scale).round() as u32).max(1)];
                let shown = view.show(gpu, presenter, &frame, alpha).cloned().and_then(|t| stage.show(&t, screen, 1.0, Background::Black));
                match shown {
                    Ok((pixels, rect)) => {
                        let model = on_screen(&reference, (clip.info.width, clip.info.height), rect, screen, 1.0, Background::Black);
                        let (worst, beyond_1) = distance(&pixels, &model, 1.0);
                        report.scaled.push(Scaled {
                            alpha: if alpha == AlphaView::Straight { "straight" } else { "premultiplied" },
                            scale,
                            frame: middle,
                            worst,
                            beyond_1,
                        });
                    }
                    Err(e) => report.error = Some(e.to_string()),
                }
            }
        }
    }
    report.seconds = started.elapsed().as_secs_f64();
    report
}

/// Run the harness over `paths`; file names in the report are relative to `root` when given.
pub fn run(paths: &[PathBuf], root: Option<&Path>, every: u32) -> Result<Report, String> {
    let gpu = glance_gpu::open_gpu().map_err(|e| e.to_string())?;
    let presenter = Presenter::new(&gpu.device);
    let mut stage = Stage::new(&gpu);
    let info = gpu.adapter.get_info();
    let mut files = Vec::new();
    for path in movies(paths) {
        let name = root.and_then(|r| path.strip_prefix(r).ok()).unwrap_or(&path).display().to_string();
        let report = file_report(&gpu, &presenter, &mut stage, &path, name, every);
        let exact = report.one_to_one.iter().all(Row::exact) && !report.one_to_one.is_empty();
        let worst_scaled = report.scaled.iter().map(|s| s.worst).fold(0f32, f32::max);
        eprintln!(
            "{}: {} {}x{} frames {} | 1:1 {} | scaled max {} | {:.1} s{}",
            report.file,
            report.route,
            report.width,
            report.height,
            report.frames,
            if report.route == "none" {
                report.message.clone().unwrap_or_default()
            } else if exact {
                "max 0".into()
            } else if !report.one_to_one.is_empty() && report.one_to_one.iter().all(|r| r.reference == Reference::None && r.nonblank && r.frames > 0) {
                "shown (the OS decoder: no CPU reference)".into()
            } else {
                format!("{:?}", report.one_to_one.iter().map(|r| (r.reference, r.worst, r.differing, r.nonblank)).collect::<Vec<_>>())
            },
            worst_scaled,
            report.seconds,
            report.error.as_ref().map(|e| format!(" | ERROR {e}")).unwrap_or_default()
        );
        files.push(report);
    }
    Ok(Report {
        build: if cfg!(debug_assertions) { "debug" } else { "release" },
        adapter: format!("{} ({:?})", info.name, info.backend),
        block_path: "exact",
        files,
    })
}
