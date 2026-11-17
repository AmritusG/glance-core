//! Thumbnails: Swift MaCrate's ThumbnailCacheTests (same steps), VideoSizeTests' HQ tiny / odd sizes (with
//! glance-core's own DXV streams), and the service rules — memory before disk, one extraction per path in flight,
//! Alpha Type regeneration, the native walk through the app's frame grabber.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_library::thumbnails::{
    MigrationOutcome, NATIVE_MAX_SIZE, NativeThumbnailSource, Rgba, ThumbnailCache, ThumbnailExtractor, ThumbnailService, cache_key, is_migration_done,
    run_migration,
};
use glance_media::AlphaView;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("crate-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> String {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p.to_string_lossy().into_owned()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn red(side: u32) -> Rgba {
    Rgba { width: side, height: side, pixels: [255, 0, 0, 255].repeat((side * side) as usize) }
}

fn fixture(rel: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(rel).to_string_lossy().into_owned()
}

// MARK: - ThumbnailCacheTests

#[test]
fn store_and_retrieve() {
    let (cache_dir, files) = (Dir::new("cache"), Dir::new("cachefiles"));
    let cache = ThumbnailCache::new(&cache_dir.0);
    let file = files.file("a.mp4", &[0, 1, 2]);
    cache.store(&red(16), &file);
    let got = cache.image(&file).expect("cache hit after store");
    assert_eq!((got.width, got.height), (16, 16));
    // The file is `<key>.jpg`, the key Swift's.
    assert!(cache_dir.0.join(format!("{}.jpg", cache_key(&file).unwrap())).exists());
}

#[test]
fn mtime_change_forces_re_extraction() {
    let (cache_dir, files) = (Dir::new("cache"), Dir::new("cachefiles"));
    let cache = ThumbnailCache::new(&cache_dir.0);
    let file = files.file("b.mp4", &[0, 1, 2]);
    cache.store(&red(16), &file);
    assert!(cache.image(&file).is_some());
    std::fs::write(&file, [0x10, 0x11, 0x12, 0x13]).unwrap();
    assert!(cache.image(&file).is_none(), "the new key does not find the old entry");
}

#[test]
fn evict_removes_key() {
    let (cache_dir, files) = (Dir::new("cache"), Dir::new("cachefiles"));
    let cache = ThumbnailCache::new(&cache_dir.0);
    let file = files.file("c.mp4", &[0]);
    cache.store(&red(16), &file);
    assert!(cache.image(&file).is_some());
    cache.evict(&file);
    assert!(cache.image(&file).is_none());
    assert!(cache.cached_jpeg_data(&file).is_none());
}

// MARK: - Extraction

/// VideoSizeTests.testHQThumbnailsAtTinyAndOddHeights, on glance-core's own DXV streams: the picture keeps the
/// display size, however small or odd.
#[test]
fn hq_thumbnails_at_tiny_and_odd_sizes() {
    let ex = ThumbnailExtractor::new(None);
    for (name, w, h) in [("ycg6", 18, 10), ("yg10", 18, 10), ("ycg6", 100, 36), ("yg10", 100, 36), ("dxt1", 18, 10), ("dxt5", 100, 36)] {
        let p = fixture(&format!("dxvkit/streams/glenc/glenc_{name}_pat{w}x{h}.mov"));
        let img = ex.extract(Path::new(&p), AlphaView::Premultiplied).unwrap_or_else(|e| panic!("{name} {w}x{h}: {e}"));
        assert_eq!((img.width, img.height), (w, h), "{name}");
        assert!(img.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255), "baked pictures are opaque");
    }
}

#[test]
fn alpha_type_changes_a_dxv_alpha_thumbnail_only() {
    let ex = ThumbnailExtractor::new(None);
    let yg10 = fixture("tiles_yg10.mov");
    let pre = ex.extract(Path::new(&yg10), AlphaView::Premultiplied).unwrap();
    let straight = ex.extract(Path::new(&yg10), AlphaView::Straight).unwrap();
    assert_ne!(pre.pixels, straight.pixels);
    let ycg6 = fixture("tiles_ycg6.mov");
    assert_eq!(ex.extract(Path::new(&ycg6), AlphaView::Premultiplied).unwrap(), ex.extract(Path::new(&ycg6), AlphaView::Straight).unwrap());
    // HAP is straight whatever the setting says.
    let hap5 = fixture("tiles_hap5.mov");
    assert_eq!(ex.extract(Path::new(&hap5), AlphaView::Premultiplied).unwrap(), ex.extract(Path::new(&hap5), AlphaView::Straight).unwrap());
}

/// A native source that hands out dark frames until `bright_from`, then bright ones; counts calls.
struct FakeNative {
    bright_from: usize,
    frames: usize,
    calls: AtomicUsize,
    times: Mutex<Vec<f64>>,
}

impl NativeThumbnailSource for FakeNative {
    fn frame_at(&self, _path: &Path, time: f64, max_size: (u32, u32)) -> Result<Rgba, String> {
        assert_eq!(max_size, NATIVE_MAX_SIZE);
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.times.lock().unwrap().push(time);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let i = (time * 15.0).round() as usize;
        if i >= self.frames {
            return Err("past the end".into());
        }
        let v = if i >= self.bright_from { 200 } else { 2 + i as u8 };
        Ok(Rgba { width: 320, height: 180, pixels: [v, v, v, 255].repeat(320 * 180) })
    }
}

#[test]
fn native_walk_samples_fifteenths_and_takes_the_first_contentful_frame() {
    let native = Arc::new(FakeNative { bright_from: 3, frames: 15, calls: AtomicUsize::new(0), times: Mutex::new(Vec::new()) });
    let ex = ThumbnailExtractor::new(Some(native.clone()));
    let (img, walk) = ex.extract_with_walk(Path::new("/nonexistent/clip.mp4"), AlphaView::Premultiplied).unwrap();
    assert_eq!((walk.index, walk.contentful), (3, true));
    assert_eq!(img.pixels[0], 200);
    assert_eq!(*native.times.lock().unwrap(), vec![0.0, 1.0 / 15.0, 2.0 / 15.0, 3.0 / 15.0]);
    // All dark and a short clip: the brightest frame seen, the walk stopped at the first failure.
    let dim = Arc::new(FakeNative { bright_from: 99, frames: 5, calls: AtomicUsize::new(0), times: Mutex::new(Vec::new()) });
    let (_, walk) = ThumbnailExtractor::new(Some(dim.clone())).extract_with_walk(Path::new("/x.mp4"), AlphaView::Premultiplied).unwrap();
    assert_eq!((walk.index, walk.contentful), (4, false));
    assert_eq!(dim.calls.load(Ordering::SeqCst), 6);
    // No grabber: native clips have no thumbnail.
    assert!(ThumbnailExtractor::new(None).extract(Path::new(&fixture("tiles_h264.mp4")), AlphaView::Premultiplied).is_err());
}

// MARK: - The service

#[test]
fn one_extraction_per_path_in_flight_then_memory_and_disk() {
    let (cache_dir, files) = (Dir::new("svc"), Dir::new("svcfiles"));
    let clip = files.file("clip.mp4", &[1, 2, 3, 4]);
    let native = Arc::new(FakeNative { bright_from: 0, frames: 15, calls: AtomicUsize::new(0), times: Mutex::new(Vec::new()) });
    let svc = Arc::new(ThumbnailService::new(&cache_dir.0, Some(native.clone())));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let (svc, clip) = (svc.clone(), clip.clone());
            std::thread::spawn(move || svc.thumbnail(&clip))
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(results.iter().all(|r| r.as_ref().is_some_and(|i| (i.width, i.height) == (320, 180))));
    assert_eq!(native.calls.load(Ordering::SeqCst), 1, "one extraction shared by every caller");
    assert_eq!(svc.memory().usage().0, 1);
    // A fresh service finds the JPEG on disk without extracting.
    let svc2 = ThumbnailService::new(&cache_dir.0, Some(native.clone()));
    assert!(svc2.thumbnail(&clip).is_some());
    assert_eq!(native.calls.load(Ordering::SeqCst), 1);
    // Regenerate drops both tiers and extracts again.
    assert!(svc2.regenerate(&clip).is_some());
    assert_eq!(native.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn the_alpha_type_provider_decides_and_regenerate_follows_it() {
    let (cache_dir, files) = (Dir::new("svcalpha"), Dir::new("svcalphafiles"));
    let clip = files.0.join("a.mov");
    std::fs::copy(fixture("tiles_yg10.mov"), &clip).unwrap();
    let clip = clip.to_string_lossy().into_owned();
    let svc = ThumbnailService::new(&cache_dir.0, None);
    let chosen = Arc::new(Mutex::new(AlphaView::Premultiplied));
    let c = chosen.clone();
    svc.set_alpha_type_provider(Some(Arc::new(move |_p: &str| Some(*c.lock().unwrap()))));
    let pre = svc.thumbnail(&clip).unwrap();
    *chosen.lock().unwrap() = AlphaView::Straight;
    assert_eq!(svc.thumbnail(&clip).unwrap(), pre, "cached until regenerated");
    let straight = svc.regenerate(&clip).unwrap();
    assert_ne!(straight, pre);
    let direct = ThumbnailExtractor::new(None).extract(Path::new(&clip), AlphaView::Straight).unwrap();
    assert_eq!(straight.pixels, direct.pixels, "under 1280 px the memory picture is the extracted one");
}

#[test]
fn failures_return_none_and_are_not_remembered() {
    let (cache_dir, files) = (Dir::new("svcfail"), Dir::new("svcfailfiles"));
    let junk = files.file("junk.mov", &[0u8; 64]);
    let svc = ThumbnailService::new(&cache_dir.0, None);
    assert!(svc.thumbnail(&junk).is_none());
    assert!(svc.thumbnail(&files.0.join("missing.mov").to_string_lossy()).is_none());
    assert_eq!(std::fs::read_dir(&cache_dir.0).unwrap().count(), 0);
}

#[test]
fn migration_rebuilds_once_and_respects_the_guards() {
    let base = Dir::new("thumbmig");
    let support = base.0.join("Application Support");
    let thumbs = base.0.join("Library/Caches/com.example.test/thumbs");
    let clip = base.0.join("clip.mov");
    std::fs::copy(fixture("tiles_dxt1.mov"), &clip).unwrap();
    let clip = clip.to_string_lossy().into_owned();
    let svc = ThumbnailService::new(&thumbs, None);
    std::fs::write(thumbs.join("stale.jpg"), b"old").unwrap();
    // Offline: clips listed but none reachable — nothing deleted, not marked.
    assert_eq!(run_migration(&svc, &["/Volumes/NotMounted/x.mov".to_string()], &support, false, |_, _| {}), MigrationOutcome::SkippedOffline);
    assert!(thumbs.join("stale.jpg").exists() && !is_migration_done(&support, false));
    let mut seen = Vec::new();
    assert_eq!(run_migration(&svc, std::slice::from_ref(&clip), &support, false, |c, t| seen.push((c, t))), MigrationOutcome::Finished(1));
    assert!(!thumbs.join("stale.jpg").exists());
    assert_eq!(std::fs::read_dir(&thumbs).unwrap().count(), 1);
    assert_eq!(seen.last(), Some(&(1, 1)));
    assert_eq!(run_migration(&svc, &[clip], &support, false, |_, _| {}), MigrationOutcome::AlreadyDone);
    // A cache folder that is not `…/Library/Caches/…/thumbs` is never deleted.
    let odd = Dir::new("thumbmig-odd");
    let svc_odd = ThumbnailService::new(&odd.0, None);
    assert_eq!(run_migration(&svc_odd, &[], &odd.0.join("support"), false, |_, _| {}), MigrationOutcome::UnexpectedCacheDir);
}

/// The native path through DXVKit Plus's AVFoundation grab: a thumbnail for H.264 and ProRes clips.
#[cfg(target_os = "macos")]
#[test]
fn native_clips_through_avfoundation() {
    let ex = ThumbnailExtractor::new(Some(Arc::new(glance_library::thumbnails::AvFoundationThumbnails)));
    for name in ["tiles_h264.mp4", "tiles_prores4444.mov", "tiles_hevc.mp4"] {
        let (img, walk) = ex.extract_with_walk(Path::new(&fixture(name)), AlphaView::Premultiplied).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((img.width, img.height, walk.index), (128, 72, 0), "{name}");
    }
    // HAP R is not HAP to MaCrate: it goes to AVFoundation, which cannot open it (as in Swift).
    assert!(ex.extract(Path::new(&fixture("synthetic_hap7.mov")), AlphaView::Premultiplied).is_err());
}
