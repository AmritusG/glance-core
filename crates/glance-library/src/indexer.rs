//! Swift MaCrate's `LibraryIndexer` (CrateCore/LibraryIndexer.swift, 0.16.4): walks library roots, hashes the clips
//! whose `(path, size, mtime)` no row already describes, reads sidecars, and writes `library.sqlite` rows — the
//! same rows, in the same cases, as Swift (cross-tested: docs/data/m3/indexer-crosstest.txt).
//!
//! Threads, not tasks: [`Indexer::rebuild`] runs on its own thread; [`Indexer::index_root`],
//! [`Indexer::handle_fs_event`] and [`Indexer::handle_external_sidecar`] block the calling thread (call them off
//! the UI thread). Several passes may run at once, as Swift's tasks interleave; the store is locked per operation.
//! What the UI observes arrives through the event sink ([`IndexerEvent`]) and [`Indexer::progress`].
//!
//! Swift compares path strings by canonical equivalence (Swift `String` `==`), URLs literally; both are kept: the
//! sets and maps Swift keys by `String` are keyed here by the NFC form, the sidecar map Swift keys by `URL` by the
//! literal path.

use crate::enumerate::{ClipFile, direct_videos, recursive_videos};
use crate::hashing::{HashError, HashOutput, HashingService};
use crate::header::{HeaderInfo, extract_info};
use crate::roots::{is_dangerous_root, is_effectively_excluded, last_path_component, standardized_path};
use crate::sidecar::{ClipMetadata, clip_path_from_sidecar, scan_sidecars, scan_sidecars_direct};
use crate::stash::read_stash_file;
use crate::store::{ClipRow, MetadataStore, now_seconds};
use crate::unicode::file_system_representation;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use unicode_normalization::UnicodeNormalization;

/// The library database shared between the indexer, the UI and the other services.
pub type SharedStore = Arc<Mutex<MetadataStore>>;

/// How many header reads the duration / rate / size backfill runs at once.
pub const BACKFILL_CONCURRENCY: usize = 8;

/// `LibraryIndexer.Phase`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Phase {
    #[default]
    Idle,
    Enumerating,
    Hashing,
    Finished,
}

/// `LibraryIndexer.Progress`: the folder being indexed (its last path component), the clips found there and how
/// many are done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub root_name: Option<String>,
    pub total: usize,
    pub completed: usize,
    pub phase: Phase,
}

/// `LibraryIndexer.ClipPathChange`: a clip found at a new path with the content of a row at an old one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipPathChange {
    pub clip_id: String,
    pub old_path: String,
    pub new_path: String,
}

/// What the indexer tells the app.
#[derive(Clone, Debug, PartialEq)]
pub enum IndexerEvent {
    /// `progress` was assigned (Swift's `@Published` fires on every assignment, equal or not; a folder with
    /// nothing to index assigns nothing once the phase is `Finished`). When the phase becomes `Finished` the app
    /// runs [`settle`] and its refreshes (Swift's `onChange(of: progress.phase)`).
    Progress(Progress),
    /// `externalSidecarChanges`: an external sidecar edit was applied to the row of this clip path.
    ExternalSidecarChanged(PathBuf),
    /// `externalRenames`: a clip whose content a row at another path has was indexed at a new path.
    ExternalRename(ClipPathChange),
    /// A line Swift writes to `diag.log` (`CrateDiag.write`), same text.
    Diag(String),
}

pub type EventSink = Arc<dyn Fn(IndexerEvent) + Send + Sync>;
type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;

/// A pass's cancellation (Swift's `Task.isCancelled` of the task the pass runs in).
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> CancelToken {
        CancelToken::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

struct Inner {
    store: SharedStore,
    hashing: Arc<HashingService>,
    progress: Mutex<Progress>,
    sink: Option<EventSink>,
    clock: Clock,
    rebuild: Mutex<Option<(CancelToken, std::thread::JoinHandle<()>)>>,
    force_full_reindex: AtomicBool,
}

/// The library indexer. Cheap to clone (shared state); every clone is the same indexer.
#[derive(Clone)]
pub struct Indexer {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Indexer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Indexer").field("progress", &self.progress()).finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Swift `String` equality is canonical equivalence: the key such a set or map compares.
fn skey(s: &str) -> String {
    s.nfc().collect()
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// `(path as NSString).deletingLastPathComponent`.
fn deleting_last_component(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => trimmed.get(..i).unwrap_or("").to_string(),
        None => String::new(),
    }
}

impl Indexer {
    /// An indexer over `store`, with its own hashing service and no event sink. The forced full re-index flag
    /// starts as the store's v8 migration report says ([`crate::schema::MigrationReport::needs_full_reindex`]).
    pub fn new(store: SharedStore) -> Indexer {
        Indexer::with(store, Arc::new(HashingService::new()), None)
    }

    /// An indexer with a shared hashing service and an event sink (called from whichever thread runs the pass).
    pub fn with(store: SharedStore, hashing: Arc<HashingService>, sink: Option<EventSink>) -> Indexer {
        let force = lock(&store).open_report().migrations.needs_full_reindex;
        Indexer {
            inner: Arc::new(Inner {
                store,
                hashing,
                progress: Mutex::new(Progress::default()),
                sink,
                clock: Arc::new(now_seconds),
                rebuild: Mutex::new(None),
                force_full_reindex: AtomicBool::new(force),
            }),
        }
    }

    /// Replace the clock new rows are stamped with (seconds since 1970). Only before the first pass.
    pub fn with_clock(self, clock: impl Fn() -> f64 + Send + Sync + 'static) -> Indexer {
        match Arc::try_unwrap(self.inner) {
            Ok(mut inner) => {
                inner.clock = Arc::new(clock);
                Indexer { inner: Arc::new(inner) }
            }
            Err(inner) => Indexer { inner },
        }
    }

    pub fn store(&self) -> &SharedStore {
        &self.inner.store
    }

    pub fn hashing(&self) -> &Arc<HashingService> {
        &self.inner.hashing
    }

    /// The current progress (`LibraryIndexer.progress`).
    pub fn progress(&self) -> Progress {
        lock(&self.inner.progress).clone()
    }

    /// Swift's `Crate.v0.6.33.forceFullReindex`: set when the v8 migration ran on a library with rows (or by the app
    /// from its own saved flag), cleared by [`Indexer::clear_force_full_reindex`] once a rebuild settles. While set,
    /// the status text starts "Re-indexing for v0.6.33… " ([`indexing_status_text`]).
    pub fn force_full_reindex(&self) -> bool {
        self.inner.force_full_reindex.load(Ordering::SeqCst)
    }

    pub fn set_force_full_reindex(&self, on: bool) {
        self.inner.force_full_reindex.store(on, Ordering::SeqCst);
    }

    /// What ContentView does after a settled pass: clear the flag.
    pub fn clear_force_full_reindex(&self) {
        self.set_force_full_reindex(false);
    }

    fn emit(&self, e: IndexerEvent) {
        if let Some(sink) = &self.inner.sink {
            sink(e);
        }
    }

    fn diag(&self, s: String) {
        self.emit(IndexerEvent::Diag(s));
    }

    fn set_progress(&self, p: Progress) {
        *lock(&self.inner.progress) = p.clone();
        self.emit(IndexerEvent::Progress(p));
    }

    fn update_progress(&self, f: impl FnOnce(&mut Progress)) {
        let p = {
            let mut g = lock(&self.inner.progress);
            f(&mut g);
            g.clone()
        };
        self.emit(IndexerEvent::Progress(p));
    }

    fn db<T>(&self, f: impl FnOnce(&mut MetadataStore) -> crate::Result<T>) -> crate::Result<T> {
        f(&mut lock(&self.inner.store))
    }

    // MARK: - Entry points

    /// `rebuild(roots:excluded:)`: cancel the rebuild in flight and the hashing session, then index every root in
    /// order on a new thread, and finally assign progress `Finished` (root name cleared). Called at launch.
    pub fn rebuild(&self, roots: Vec<PathBuf>, excluded: HashSet<String>) {
        let token = CancelToken::new();
        let previous = lock(&self.inner.rebuild).take();
        if let Some((t, _)) = &previous {
            t.cancel();
        }
        self.inner.hashing.cancel();
        let me = self.clone();
        let t = token.clone();
        let handle = std::thread::spawn(move || {
            for root in &roots {
                if t.is_cancelled() {
                    break;
                }
                me.index_root_in(root, &excluded, &t);
            }
            me.update_progress(|p| {
                p.root_name = None;
                p.phase = Phase::Finished;
            });
        });
        *lock(&self.inner.rebuild) = Some((token, handle));
    }

    /// Block until the rebuild thread (if any) has finished.
    pub fn wait_rebuild(&self) {
        let job = lock(&self.inner.rebuild).take();
        if let Some((_, handle)) = job {
            let _ = handle.join();
        }
    }

    /// `cancel()`: cancel the rebuild and the hashing session in flight.
    pub fn cancel(&self) {
        if let Some((t, _)) = lock(&self.inner.rebuild).as_ref() {
            t.cancel();
        }
        self.inner.hashing.cancel();
    }

    /// `indexRoot(_:excluded:)`: refuse a dangerous root ([`is_dangerous_root`] of its [`standardized_path`]) or an
    /// excluded one; otherwise index it recursively and ingest its `.crate-stashes.json`. For a new root, Refresh,
    /// Reconnect and re-include.
    pub fn index_root(&self, root: &Path, excluded: &HashSet<String>) {
        self.index_root_in(root, excluded, &CancelToken::new());
    }

    fn index_root_in(&self, root: &Path, excluded: &HashSet<String>, token: &CancelToken) {
        let canonical = standardized_path(&path_str(root));
        if is_dangerous_root(&canonical) {
            self.diag(format!("v0.6.29 indexRoot: DROPPED (dangerous root) path={canonical}"));
            return;
        }
        if is_effectively_excluded(&canonical, excluded) {
            self.diag(format!("v0.6.34 indexRoot: SKIPPED (excluded) path={canonical}"));
            return;
        }
        self.diag(format!("v0.6.29 indexRoot: walking root path={canonical}"));
        self.process_folder(root, true, excluded, token);
        self.ingest_stashes(root);
    }

    fn ingest_stashes(&self, root: &Path) {
        let stashes = read_stash_file(root);
        if stashes.is_empty() {
            return;
        }
        for s in &stashes {
            let _ = self.db(|db| db.upsert_stash(s));
        }
        let name = last_path_component(&path_str(root));
        self.diag(format!("v9 STEP 5b: ingested {} stash(es) from {name}/.crate-stashes.json", stashes.len()));
    }

    /// `handleFSEvent(changedDirs:roots:excluded:)`: each changed folder that still exists, lies at or under a root
    /// (standardized paths) and is not excluded is indexed — recursively when the library has no row under it
    /// (a folder moved in), else one level.
    pub fn handle_fs_event(&self, changed_dirs: &[PathBuf], roots: &[PathBuf], excluded: &HashSet<String>) {
        let token = CancelToken::new();
        let root_paths: Vec<String> = roots.iter().map(|r| standardized_path(&path_str(r))).collect();
        for dir in changed_dirs {
            if token.is_cancelled() {
                return;
            }
            if std::fs::metadata(dir).is_err() {
                continue;
            }
            let dp = standardized_path(&path_str(dir));
            let under_root = root_paths.iter().any(|rp| dp == *rp || dp.strip_prefix(rp.as_str()).is_some_and(|r| r.starts_with('/')));
            if !under_root {
                self.diag(format!("v0.6.6 handleFSEvent: DROPPED (outside roots) path={dp}"));
                continue;
            }
            if is_effectively_excluded(&dp, excluded) {
                self.diag(format!("v0.6.34 handleFSEvent: SKIPPED (excluded) path={dp}"));
                continue;
            }
            self.diag(format!("v0.6.6 handleFSEvent: scanning path={dp}"));
            let known = self.db(|db| db.has_any_row(&path_str(dir))).unwrap_or(false);
            self.process_folder(dir, !known, excluded, &token);
        }
    }

    /// `handleExternalSidecar(_:)`: a sidecar another process wrote. When a row exists at the clip path the sidecar
    /// names, its user fields and `updated_at` are taken from the sidecar (no timestamp comparison), its tags
    /// replaced, the orphan flag set from whether the clip file exists; then
    /// [`IndexerEvent::ExternalSidecarChanged`]. No row: nothing (the folder scan of the same batch covers it).
    pub fn handle_external_sidecar(&self, sidecar: &Path) {
        if std::fs::metadata(sidecar).is_err() {
            return;
        }
        let Ok(bytes) = std::fs::read(sidecar) else { return };
        let Ok(meta) = ClipMetadata::decode(&bytes) else { return };
        let Some(expected) = clip_path_from_sidecar(sidecar) else { return };
        let expected_path = path_str(&expected);
        let Ok(Some(existing)) = self.db(|db| db.fetch_at(&expected_path)) else { return };
        let clip_exists = std::fs::metadata(&expected).is_ok();
        let mut updated = existing;
        apply_sidecar_fields(&meta, &mut updated);
        updated.is_orphan = !clip_exists;
        let _ = self.db(|db| db.upsert(&updated, &meta.tags));
        self.emit(IndexerEvent::ExternalSidecarChanged(expected));
    }

    // MARK: - The pipeline

    fn process_folder(&self, folder: &Path, recursive: bool, excluded: &HashSet<String>, token: &CancelToken) {
        // Swift's `defer`: the phase ends `Finished` on every exit, assigned only when it is not already.
        struct Finish<'a>(&'a Indexer);
        impl Drop for Finish<'_> {
            fn drop(&mut self) {
                let finished = lock(&self.0.inner.progress).phase == Phase::Finished;
                if !finished {
                    self.0.update_progress(|p| p.phase = Phase::Finished);
                }
            }
        }
        let _finish = Finish(self);

        let folder_str = path_str(folder);
        if is_effectively_excluded(&standardized_path(&folder_str), excluded) {
            return;
        }

        // 1. Clips and sidecars, silently.
        let (clips, sidecars) = std::thread::scope(|s| {
            let c =
                s.spawn(
                    || if recursive { recursive_videos(folder, excluded, None, &|| token.is_cancelled()) } else { direct_videos(folder).unwrap_or_default() },
                );
            let sc = if recursive { scan_sidecars(folder) } else { scan_sidecars_direct(folder) };
            (c.join().unwrap_or_default(), sc)
        });
        if token.is_cancelled() {
            return;
        }

        // 2. Rows under the folder.
        let fetched = self.db(|db| db.fetch_all_under(&folder_str)).unwrap_or_default();
        if clips.is_empty() && sidecars.is_empty() && fetched.is_empty() {
            return;
        }

        // 3. Real work.
        let root_name = last_path_component(&folder_str);
        self.set_progress(Progress { root_name: Some(root_name.clone()), total: clips.len(), completed: 0, phase: Phase::Enumerating });

        // 4. Sidecar lookups: by the clip path each names (URL-keyed in Swift: literal), by content.
        let mut by_expected: HashMap<String, &ClipMetadata> = HashMap::new();
        let mut by_clip_id: HashMap<&str, &ClipMetadata> = HashMap::new();
        for (path, meta) in &sidecars {
            if let Some(clip) = clip_path_from_sidecar(path) {
                by_expected.insert(path_str(&clip), meta);
            }
            by_clip_id.insert(meta.clip_id.as_str(), meta);
        }

        // 3.5. One row per path.
        let existing = self.deduplicate_rows_by_path(fetched);
        let row_by_path: HashMap<String, &ClipRow> = existing.iter().map(|r| (skey(&r.current_path), r)).collect();
        let mut rows_by_clip_id: HashMap<&str, Vec<&ClipRow>> = HashMap::new();
        for r in &existing {
            rows_by_clip_id.entry(r.clip_id.as_str()).or_default().push(r);
        }

        // 4. Cache hits and misses.
        let mut misses: Vec<&ClipFile> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut completed = 0usize;
        let mut backfill: Vec<BackfillItem> = Vec::new();
        for clip in &clips {
            if let Some(row) = row_by_path.get(&skey(&clip.path)).copied()
                && crate::roots::cache_hit(row.file_size, row.file_mtime, clip.size, clip.mtime)
            {
                if row.is_orphan {
                    self.diag(format!("4.4.b auto-cleared false orphan: {} clip_id={}", clip.display_name, row.clip_id));
                    let _ = self.db(|db| db.set_orphan_at(&row.current_path, false));
                }
                if row.duration_sec.is_none() || row.frame_rate.is_none() || row.video_width.is_none() {
                    backfill.push(BackfillItem {
                        clip_id: row.clip_id.clone(),
                        path: clip.path.clone(),
                        needs_duration: row.duration_sec.is_none(),
                        needs_frame_rate: row.frame_rate.is_none(),
                        needs_video_size: row.video_width.is_none(),
                    });
                }
                let sidecar = by_expected.get(&clip.path).or_else(|| by_clip_id.get(row.clip_id.as_str())).copied();
                if let Some(sc) = sidecar
                    && sc.updated_at > row.updated_at + 0.001
                {
                    let mut refreshed = row.clone();
                    apply_sidecar_fields(sc, &mut refreshed);
                    match self.db(|db| db.upsert(&refreshed, &sc.tags)) {
                        Ok(()) => self.diag(format!(
                            "v0.6.23 sidecar reconcile: {} clip_id={} rowUpdatedAt={} sidecarUpdatedAt={}",
                            clip.display_name,
                            row.clip_id,
                            crate::thumbkey::swift_double(row.updated_at),
                            crate::thumbkey::swift_double(sc.updated_at)
                        )),
                        Err(e) => self.diag(format!("v0.6.23 sidecar reconcile FAILED: {} clip_id={} err={e}", clip.display_name, row.clip_id)),
                    }
                }
                seen.insert(skey(&row.current_path));
                completed += 1;
                continue;
            }
            misses.push(clip);
        }

        self.set_progress(Progress { root_name: Some(root_name), total: clips.len(), completed, phase: Phase::Hashing });

        // 5. Hash the misses.
        if !misses.is_empty() {
            let paths: Vec<String> = misses.iter().map(|c| c.path.clone()).collect();
            let baseline = completed;
            let results = self.inner.hashing.index_folder(&paths, |n| self.update_progress(|p| p.completed = baseline + n));
            if token.is_cancelled() {
                return;
            }
            // 6. Upsert what hashed.
            for clip in &misses {
                let outcome = results.get(&clip.path).cloned().unwrap_or(Err(HashError::FileMissing));
                let hash: HashOutput = match outcome {
                    Ok(h) => h,
                    Err(e) => {
                        self.diag(format!("v0.6.33 indexer: hash-failed path={} error={e}", clip.path));
                        continue;
                    }
                };
                let sidecar = by_expected.get(&clip.path).or_else(|| by_clip_id.get(hash.clip_id.as_str())).copied();
                let info = extract_info(Path::new(&clip.path));
                let row = self.make_row(clip, &hash, sidecar, &info);
                let tags = sidecar.map(|s| s.tags.clone()).unwrap_or_default();
                let _ = self.db(|db| db.upsert(&row, &tags));
                seen.insert(skey(&clip.path));
                if let Some(old_rows) = rows_by_clip_id.get(hash.clip_id.as_str()) {
                    for old in old_rows {
                        if skey(&old.current_path) != skey(&clip.path) {
                            self.emit(IndexerEvent::ExternalRename(ClipPathChange {
                                clip_id: hash.clip_id.clone(),
                                old_path: old.current_path.clone(),
                                new_path: clip.path.clone(),
                            }));
                        }
                    }
                }
            }
        }

        // 7. Sidecars whose clip was not seen: an orphan row from the sidecar, unless the content is live elsewhere.
        for (sidecar_path, meta) in &sidecars {
            let last_known = clip_path_from_sidecar(sidecar_path).map_or_else(|| path_str(sidecar_path), |p| path_str(&p));
            if is_effectively_excluded(&standardized_path(&last_known), excluded) {
                continue;
            }
            if seen.contains(&skey(&last_known)) {
                continue;
            }
            let siblings = self.db(|db| db.fetch_duplicates(&meta.clip_id, &last_known)).unwrap_or_default();
            if let Some(live) = siblings.iter().find(|r| !r.is_orphan) {
                self.diag(format!("v0.6.33 indexer: orphan-skip path={last_known} reason=clip_id-exists-elsewhere existing_path={}", live.current_path));
                continue;
            }
            let row = make_orphan_row(meta, &last_known);
            let _ = self.db(|db| db.upsert(&row, &meta.tags));
        }

        // 8. Rows whose file is gone and that no sidecar covers: flag orphan (one level only when not recursive).
        let folder_key = skey(&folder_str);
        for row in &existing {
            if is_effectively_excluded(&standardized_path(&row.current_path), excluded) {
                continue;
            }
            if seen.contains(&skey(&row.current_path)) {
                continue;
            }
            if by_expected.contains_key(&file_system_representation(&row.current_path)) {
                continue;
            }
            if !recursive && skey(&deleting_last_component(&row.current_path)) != folder_key {
                continue;
            }
            let _ = self.db(|db| db.set_orphan_at(&row.current_path, true));
        }

        // 9. Duration / rate / size backfill for cache hits.
        self.backfill(&backfill, &last_path_component(&folder_str));

        self.update_progress(|p| p.phase = Phase::Finished);
    }

    fn backfill(&self, items: &[BackfillItem], folder_name: &str) {
        if items.is_empty() {
            return;
        }
        let (mut extracted, mut rates_found, mut rates_none, mut sizes_found, mut sizes_none) = (0, 0, 0, 0, 0);
        let next = Mutex::new(0usize);
        let (tx, rx) = std::sync::mpsc::channel::<(usize, HeaderInfo)>();
        std::thread::scope(|s| {
            for _ in 0..BACKFILL_CONCURRENCY.min(items.len()) {
                let tx = tx.clone();
                let next = &next;
                s.spawn(move || {
                    loop {
                        let i = {
                            let mut n = lock(next);
                            let i = *n;
                            *n += 1;
                            i
                        };
                        let Some(item) = items.get(i) else { break };
                        if tx.send((i, extract_info(Path::new(&item.path)))).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            for (i, info) in rx {
                let Some(item) = items.get(i) else { continue };
                if item.needs_duration
                    && let Some(d) = info.duration
                {
                    let _ = self.db(|db| db.set_duration(&item.clip_id, Some(d)));
                    extracted += 1;
                }
                if item.needs_frame_rate {
                    let _ = self.db(|db| db.set_frame_rate(&item.clip_id, info.stored_fps()));
                    if info.fps.is_some() {
                        rates_found += 1;
                    } else {
                        rates_none += 1;
                    }
                }
                if item.needs_video_size {
                    let (w, h) = info.stored_size();
                    let _ = self.db(|db| db.set_video_size(&item.clip_id, w, h));
                    if info.size.is_some() {
                        sizes_found += 1;
                    } else {
                        sizes_none += 1;
                    }
                }
            }
        });
        let n = items.len();
        if extracted > 0 {
            self.diag(format!("5.2.b duration backfill under {folder_name}: extracted {extracted} of {n}"));
        }
        if rates_found + rates_none > 0 {
            self.diag(format!("v10 frame-rate backfill under {folder_name}: rate={rates_found} none={rates_none} of {n}"));
        }
        if sizes_found + sizes_none > 0 {
            self.diag(format!("v11 video-size backfill under {folder_name}: size={sizes_found} none={sizes_none} of {n}"));
        }
    }

    /// `deduplicateRowsByPath`: rows sharing a path (by Swift's string equality — NFC and NFD spellings of one
    /// path) are collapsed to the one whose `clip_id` the file hashes to (the others deleted); when none matches all
    /// are deleted; when the file cannot be hashed the most recently updated is kept.
    fn deduplicate_rows_by_path(&self, rows: Vec<ClipRow>) -> Vec<ClipRow> {
        let mut order: Vec<String> = Vec::new();
        let mut by_path: HashMap<String, Vec<ClipRow>> = HashMap::new();
        for row in rows {
            let k = skey(&row.current_path);
            if !by_path.contains_key(&k) {
                order.push(k.clone());
            }
            by_path.entry(k).or_default().push(row);
        }
        let mut resolved = Vec::with_capacity(order.len());
        for k in order {
            let Some(mut group) = by_path.remove(&k) else { continue };
            if group.len() == 1 {
                resolved.append(&mut group);
                continue;
            }
            let path = group[0].current_path.clone();
            match crate::clip_id::clip_id(Path::new(&path)) {
                Ok(id) => {
                    if let Some(truth) = group.iter().find(|r| r.clip_id == id).cloned() {
                        for stale in group.iter().filter(|r| r.clip_id != truth.clip_id) {
                            let _ = self.db(|db| db.delete_row(&stale.current_path));
                        }
                        resolved.push(truth);
                    } else {
                        for stale in &group {
                            let _ = self.db(|db| db.delete_row(&stale.current_path));
                        }
                    }
                }
                Err(e) => {
                    self.diag(format!("4.4.b dedupe hash failure path={path} err={e}"));
                    // Swift's `max(by:)` keeps the first of equal maxima.
                    let mut pick: Option<ClipRow> = None;
                    for r in group {
                        if pick.as_ref().is_none_or(|p| p.updated_at < r.updated_at) {
                            pick = Some(r);
                        }
                    }
                    resolved.extend(pick);
                }
            }
        }
        resolved
    }

    fn make_row(&self, clip: &ClipFile, hash: &HashOutput, sidecar: Option<&ClipMetadata>, info: &HeaderInfo) -> ClipRow {
        let now = (self.inner.clock)();
        let (w, h) = info.stored_size();
        let mut row = ClipRow::new(hash.clip_id.clone(), clip.path.clone(), hash.file_size, hash.file_mtime, sidecar.map_or(now, |s| s.created_at), now);
        row.duration_sec = info.duration;
        row.frame_rate = Some(info.stored_fps());
        row.video_width = Some(w);
        row.video_height = Some(h);
        if let Some(s) = sidecar {
            apply_sidecar_fields(s, &mut row);
        }
        row
    }
}

struct BackfillItem {
    clip_id: String,
    path: String,
    needs_duration: bool,
    needs_frame_rate: bool,
    needs_video_size: bool,
}

/// `applySidecar(_:onto:)`: the fields the sidecar owns.
fn apply_sidecar_fields(meta: &ClipMetadata, row: &mut ClipRow) {
    row.rating = meta.rating;
    row.bpm = meta.bpm;
    row.mood = meta.mood.clone();
    row.genre = meta.genre.clone();
    row.energy = meta.energy;
    row.brief = meta.brief.clone();
    row.updated_at = meta.updated_at;
}

/// `makeOrphanRow(meta:lastKnownPath:)`.
fn make_orphan_row(meta: &ClipMetadata, last_known: &str) -> ClipRow {
    let mut row = ClipRow::new(meta.clip_id.clone(), last_known, meta.file_size_from_clip_id().unwrap_or(0), 0.0, meta.created_at, meta.updated_at);
    row.rating = meta.rating;
    row.bpm = meta.bpm;
    row.mood = meta.mood.clone();
    row.genre = meta.genre.clone();
    row.energy = meta.energy;
    row.brief = meta.brief.clone();
    row.is_orphan = true;
    row
}

// MARK: - What runs when a pass settles

/// What [`settle`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettleReport {
    /// Orphan rows deleted because their content is live at another path (the move-from litter).
    pub retired: Vec<String>,
    /// Rows deleted under currently excluded folders.
    pub purged: usize,
}

/// The store half of ContentView's settle (`onChange(of: progress.phase)` → `.finished`): retire orphans with a
/// live twin, then delete every row under each excluded folder (the exclusion set's own strings as prefixes, as
/// Swift does). The app then refreshes the orphan count, the [`crate::duplicates::DuplicateRegistry`], the
/// un-briefed count, the stash list and the root sizes, and clears the forced re-index flag.
pub fn settle(store: &mut MetadataStore, excluded: &HashSet<String>) -> SettleReport {
    let retired = store.retire_orphans_with_live_twin().unwrap_or_default();
    let mut purged = 0;
    for prefix in excluded {
        purged += store.delete_rows_under(prefix).map(|v| v.len()).unwrap_or(0);
    }
    SettleReport { retired, purged }
}

/// The Info panel's indexing readout (`InfoPanelView.indexingStatusText`): `"<prefix>Indexing <root>… enumerating"`
/// or `"<prefix>Indexing <root>… <completed> of <total>"`, empty when idle or finished; the prefix is
/// `"Re-indexing for v0.6.33… "` while the forced full re-index flag is set.
pub fn indexing_status_text(p: &Progress, force_full_reindex: bool) -> String {
    let root = p.root_name.as_deref().unwrap_or("");
    let prefix = if force_full_reindex { "Re-indexing for v0.6.33… " } else { "" };
    match p.phase {
        Phase::Enumerating => format!("{prefix}Indexing {root}… enumerating"),
        Phase::Hashing => format!("{prefix}Indexing {root}… {} of {}", p.completed, p.total),
        Phase::Idle | Phase::Finished => String::new(),
    }
}
