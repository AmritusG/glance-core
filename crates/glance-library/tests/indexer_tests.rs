//! The indexer, ported from Swift MaCrate's LibraryIndexerTests, IndexerQuietEventTests, the indexer cases of
//! ExclusionEngineTests and VideoSizeTests, RetireOrphansTests — same inputs, same expectations — plus the paths
//! Swift's tests could not reach from a temp dir (`indexRoot` refuses `/private` and `/var`; these run under the
//! target directory instead) and the events.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_library::clip_id::clip_id;
use glance_library::indexer::{Indexer, IndexerEvent, Phase, Progress, SharedStore, indexing_status_text, settle};
use glance_library::sidecar::{ClipMetadata, sidecar_path, write_sidecar};
use glance_library::stash::{Stash, StashEntry, write_stash_file};
use glance_library::store::{ClipRow, Location, MetadataStore};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct Dir(PathBuf);

impl Dir {
    /// Swift's tests: a temp dir in its kernel-canonical form (`realpath`, `/private/var/…`).
    fn temp() -> Dir {
        let raw = std::env::temp_dir().join(format!("crate-indexer-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&raw).unwrap();
        Dir(std::fs::canonicalize(&raw).unwrap())
    }
    /// A folder `indexRoot` accepts (not under `/private`, `/tmp` or `/var`).
    fn rootable() -> Dir {
        let raw = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("crate-indexer-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&raw).unwrap();
        Dir(std::fs::canonicalize(&raw).unwrap())
    }
    fn p(&self) -> &Path {
        &self.0
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn default_contents() -> Vec<u8> {
    (0..128u8).collect()
}

fn store() -> SharedStore {
    Arc::new(Mutex::new(MetadataStore::open_with(Location::InMemory, Box::new(Default::default)).unwrap()))
}

fn indexer() -> (Indexer, SharedStore, Arc<Mutex<Vec<IndexerEvent>>>) {
    let s = store();
    let events = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    let ix = Indexer::with(s.clone(), Arc::new(glance_library::hashing::HashingService::new()), Some(Arc::new(move |ev| e.lock().unwrap().push(ev))));
    (ix, s, events)
}

/// Swift's `indexViaFSEvent`: the folder as both the changed dir and the root.
fn index_via_fs_event(ix: &Indexer, dir: &Path, excluded: &HashSet<String>) {
    ix.handle_fs_event(&[dir.to_path_buf()], &[dir.to_path_buf()], excluded);
}

fn none() -> HashSet<String> {
    HashSet::new()
}

fn meta(id: &str, rating: i64, tags: &[&str], created: f64, updated: f64) -> ClipMetadata {
    let mut m = ClipMetadata::new(id, created);
    m.rating = rating;
    m.tags = tags.iter().map(|t| t.to_string()).collect();
    m.updated_at = updated;
    m
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

// MARK: - The three sub-cases

#[test]
fn case_b_clip_without_sidecar_creates_default_row() {
    let d = Dir::temp();
    d.file("fresh.mov", &default_contents());
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    let st = st.lock().unwrap();
    let rows = st.fetch_all().unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert!(Path::new(&row.current_path).exists());
    assert!(row.current_path.ends_with("/fresh.mov"));
    assert_eq!(row.rating, 0);
    assert_eq!(row.bpm, None);
    assert_eq!(row.mood, None);
    assert!(!row.is_orphan);
    assert!(st.tags_for_clip_id(&row.clip_id).unwrap().is_empty());
}

#[test]
fn case_a_clip_with_sidecar_loads_metadata() {
    let d = Dir::temp();
    let clip = d.file("tagged.mov", &default_contents());
    let id = clip_id(&clip).unwrap();
    let mut m = meta(&id, 5, &["intro", "fast"], 1_700_000_000.0, 1_700_000_000.0);
    m.bpm = Some(140.0);
    m.mood = Some("warm".into());
    m.genre = Some("house".into());
    m.energy = Some(8);
    write_sidecar(&m, &clip, None).unwrap();
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    let st = st.lock().unwrap();
    let row = st.fetch_by_clip_id(&id).unwrap().unwrap();
    assert_eq!(
        (row.rating, row.bpm, row.mood.as_deref(), row.genre.as_deref(), row.energy, row.is_orphan),
        (5, Some(140.0), Some("warm"), Some("house"), Some(8), false)
    );
    assert_eq!(st.tags_for_clip_id(&id).unwrap(), vec!["fast", "intro"]);
}

#[test]
fn case_c_sidecar_with_missing_clip_becomes_orphan() {
    let d = Dir::temp();
    let phantom = d.p().join("ghost.mov");
    let id = "deadbeef1234567890123456789012345678901234567890123456789012345678:9999";
    write_sidecar(&meta(id, 3, &["abandoned"], 1_700_000_000.0, 1_700_000_000.0), &phantom, None).unwrap();
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    let st = st.lock().unwrap();
    let row = st.fetch_by_clip_id(id).unwrap().unwrap();
    assert!(row.is_orphan);
    assert_eq!(row.rating, 3);
    assert_eq!(row.file_size, 9999, "the orphan's size comes from the clip id");
    assert_eq!(row.file_mtime, 0.0);
    assert_eq!(st.tags_for_clip_id(id).unwrap(), vec!["abandoned"]);
}

// MARK: - Cache hit

#[test]
fn cache_hit_skips_rehash_on_second_index() {
    let d = Dir::temp();
    let clip = d.file("stable.mov", &default_contents());
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    let row1 = st.lock().unwrap().fetch_all().unwrap().remove(0);
    let meta = std::fs::symlink_metadata(&clip).unwrap();
    assert_eq!(row1.current_path, s(&clip));
    assert_eq!(row1.file_size, meta.len() as i64);
    assert!((row1.file_mtime - glance_library::enumerate::foundation_mtime(&meta).unwrap()).abs() < 0.001);
    let mut poisoned = row1.clone();
    poisoned.updated_at = 0.0;
    st.lock().unwrap().upsert(&poisoned, &[]).unwrap();
    index_via_fs_event(&ix, d.p(), &none());
    assert_eq!(st.lock().unwrap().fetch_by_clip_id(&row1.clip_id).unwrap().unwrap().updated_at, 0.0, "cache hit preserves the row exactly");
}

#[test]
fn mtime_change_invalidates_cache_and_rehashes() {
    let d = Dir::temp();
    let clip = d.file("touched.mov", &default_contents());
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    let row1 = st.lock().unwrap().fetch_all().unwrap().remove(0);
    let mut poisoned = row1.clone();
    poisoned.updated_at = 0.0;
    st.lock().unwrap().upsert(&poisoned, &[]).unwrap();
    let future = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    std::fs::File::options().write(true).open(&clip).unwrap().set_modified(future).unwrap();
    index_via_fs_event(&ix, d.p(), &none());
    assert_ne!(st.lock().unwrap().fetch_by_clip_id(&row1.clip_id).unwrap().unwrap().updated_at, 0.0);
}

#[test]
fn cache_hit_applies_newer_sidecar() {
    let d = Dir::temp();
    let clip = d.file("reconcile.mov", &default_contents());
    let id = clip_id(&clip).unwrap();
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    let row1 = st.lock().unwrap().fetch_by_clip_id(&id).unwrap().unwrap();
    assert_eq!((row1.brief.as_deref(), row1.rating), (None, 0));
    let newer = row1.updated_at + 10.0;
    let mut m = meta(&id, 4, &["abstract", "neon"], row1.created_at, newer);
    m.bpm = Some(128.0);
    m.mood = Some("warm".into());
    m.genre = Some("house".into());
    m.energy = Some(7);
    m.brief = Some("portrait, sunset, neon, dreamy".into());
    write_sidecar(&m, &clip, None).unwrap();
    index_via_fs_event(&ix, d.p(), &none());
    let st = st.lock().unwrap();
    let row2 = st.fetch_by_clip_id(&id).unwrap().unwrap();
    assert_eq!(row2.brief.as_deref(), Some("portrait, sunset, neon, dreamy"));
    assert_eq!((row2.rating, row2.bpm, row2.mood.as_deref(), row2.genre.as_deref(), row2.energy), (4, Some(128.0), Some("warm"), Some("house"), Some(7)));
    assert_eq!(row2.updated_at, newer);
    assert_eq!((&row2.clip_id, row2.file_size, row2.file_mtime, row2.created_at), (&row1.clip_id, row1.file_size, row1.file_mtime, row1.created_at));
    assert_eq!(st.tags_for_clip_id(&id).unwrap(), vec!["abstract", "neon"]);
}

#[test]
fn cache_hit_ignores_older_sidecar() {
    let d = Dir::temp();
    let clip = d.file("stale-sidecar.mov", &default_contents());
    let id = clip_id(&clip).unwrap();
    let base = 1_700_000_000.0;
    let mut seed = meta(&id, 5, &["seed"], base, base);
    seed.brief = Some("original brief".into());
    write_sidecar(&seed, &clip, None).unwrap();
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    let mut row1 = st.lock().unwrap().fetch_by_clip_id(&id).unwrap().unwrap();
    row1.updated_at = base + 100.0;
    row1.brief = Some("row brief (fresher)".into());
    row1.rating = 2;
    st.lock().unwrap().upsert(&row1, &["seed".to_string()]).unwrap();
    let mut stale = meta(&id, 1, &["stale-tag"], base, base + 50.0);
    stale.brief = Some("stale brief (older)".into());
    write_sidecar(&stale, &clip, None).unwrap();
    index_via_fs_event(&ix, d.p(), &none());
    let st = st.lock().unwrap();
    let row2 = st.fetch_by_clip_id(&id).unwrap().unwrap();
    assert_eq!((row2.brief.as_deref(), row2.rating, row2.updated_at), (Some("row brief (fresher)"), 2, base + 100.0));
    assert_eq!(st.tags_for_clip_id(&id).unwrap(), vec!["seed"]);
}

// MARK: - Rename recovery, orphan re-attach

#[test]
fn renamed_clip_recovers_sidecar_by_clip_id() {
    let d = Dir::temp();
    let original = d.file("original.mov", &default_contents());
    let id = clip_id(&original).unwrap();
    write_sidecar(&meta(&id, 4, &["renamed"], 1.0, 1.0), &original, None).unwrap();
    std::fs::rename(&original, d.p().join("renamed.mov")).unwrap();
    assert!(sidecar_path(&original).exists());
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    let st = st.lock().unwrap();
    let row = st.fetch_by_clip_id(&id).unwrap().unwrap();
    assert!(row.current_path.ends_with("/renamed.mov"));
    assert!(Path::new(&row.current_path).exists());
    assert_eq!((row.rating, row.is_orphan), (4, false));
    assert_eq!(st.tags_for_clip_id(&id).unwrap(), vec!["renamed"]);
}

#[test]
fn orphan_flag_clears_when_clip_reappears() {
    let d = Dir::temp();
    let clip = d.file("vanishing.mov", &default_contents());
    let id = clip_id(&clip).unwrap();
    write_sidecar(&meta(&id, 2, &[], 1.0, 1.0), &clip, None).unwrap();
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    assert!(!st.lock().unwrap().fetch_by_clip_id(&id).unwrap().unwrap().is_orphan);
    let away = Dir::temp();
    let stashed = away.p().join("stash.mov");
    std::fs::rename(&clip, &stashed).unwrap();
    index_via_fs_event(&ix, d.p(), &none());
    assert!(st.lock().unwrap().fetch_by_clip_id(&id).unwrap().unwrap().is_orphan);
    std::fs::rename(&stashed, &clip).unwrap();
    index_via_fs_event(&ix, d.p(), &none());
    let row = st.lock().unwrap().fetch_by_clip_id(&id).unwrap().unwrap();
    assert_eq!((row.is_orphan, row.rating), (false, 2));
}

// MARK: - Progress and events

#[test]
fn progress_reaches_total_when_finished() {
    let d = Dir::temp();
    d.file("a.mov", &[1]);
    d.file("b.mov", &[2]);
    d.file("c.mov", &[3]);
    let (ix, _, events) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    let p = ix.progress();
    assert_eq!(p.phase, Phase::Finished);
    assert_eq!(p.total, 3);
    assert!(p.completed >= 3);
    let phases: Vec<Phase> = events.lock().unwrap().iter().filter_map(|e| if let IndexerEvent::Progress(p) = e { Some(p.phase) } else { None }).collect();
    assert_eq!(phases.first(), Some(&Phase::Enumerating));
    assert!(phases.contains(&Phase::Hashing));
    assert_eq!(phases.last(), Some(&Phase::Finished));
}

/// IndexerQuietEventTests: an event in a watched folder with no clips or sidecars publishes nothing once settled.
#[test]
fn event_in_folder_without_clips_does_not_publish() {
    let base = Dir::temp();
    let renders = base.p().join("renders");
    std::fs::create_dir_all(&renders).unwrap();
    std::fs::write(renders.join("frame0001.png"), [1]).unwrap();
    let (ix, _, events) = indexer();
    ix.handle_fs_event(std::slice::from_ref(&renders), &[base.p().to_path_buf()], &none());
    let count = |ev: &Arc<Mutex<Vec<IndexerEvent>>>| ev.lock().unwrap().iter().filter(|e| matches!(e, IndexerEvent::Progress(_))).count();
    let settled = count(&events);
    for i in 2..=6 {
        std::fs::write(renders.join(format!("frame{i:04}.png")), [1]).unwrap();
        ix.handle_fs_event(std::slice::from_ref(&renders), &[base.p().to_path_buf()], &none());
    }
    assert_eq!(count(&events), settled);
    assert_eq!(ix.progress().phase, Phase::Finished);
}

#[test]
fn external_rename_is_reported_once_per_old_path() {
    let d = Dir::temp();
    let a = d.file("a.mov", &default_contents());
    let (ix, st, events) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    std::fs::rename(&a, d.p().join("b.mov")).unwrap();
    events.lock().unwrap().clear();
    index_via_fs_event(&ix, d.p(), &none());
    let renames: Vec<_> = events.lock().unwrap().iter().filter_map(|e| if let IndexerEvent::ExternalRename(c) = e { Some(c.clone()) } else { None }).collect();
    assert_eq!(renames.len(), 1);
    assert_eq!(renames[0].old_path, s(&a));
    assert_eq!(renames[0].new_path, s(&d.p().join("b.mov")));
    // The old row is flagged orphan; settle retires it (its content is live at the new path).
    let rep = settle(&mut st.lock().unwrap(), &none());
    assert_eq!(rep.retired, vec![s(&a)]);
    assert_eq!(st.lock().unwrap().fetch_all().unwrap().len(), 1);
}

#[test]
fn external_sidecar_updates_the_row_without_timestamp_compare() {
    let d = Dir::temp();
    let clip = d.file("x.mov", &default_contents());
    let id = clip_id(&clip).unwrap();
    let (ix, st, events) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    // An older timestamp still wins: the external path does not compare.
    let mut m = meta(&id, 5, &["ext"], 1.0, 2.0);
    m.brief = Some("from another host".into());
    let sc = write_sidecar(&m, &clip, None).unwrap();
    ix.handle_external_sidecar(&sc);
    let row = st.lock().unwrap().fetch_at(&s(&clip)).unwrap().unwrap();
    assert_eq!((row.rating, row.brief.as_deref(), row.updated_at, row.is_orphan), (5, Some("from another host"), 2.0, false));
    assert!(events.lock().unwrap().iter().any(|e| *e == IndexerEvent::ExternalSidecarChanged(clip.clone())));
    // No row at the sidecar's clip path: nothing.
    let other = write_sidecar(&meta("ff:1", 1, &[], 1.0, 1.0), &d.p().join("nothing.mov"), None).unwrap();
    ix.handle_external_sidecar(&other);
    assert!(st.lock().unwrap().fetch_at(&s(&d.p().join("nothing.mov"))).unwrap().is_none());
}

// MARK: - Exclusions, refusals, roots

/// ExclusionEngineTests.testIndexerDoesNotOrphanExcludedSubtreeSidecar.
#[test]
fn indexer_does_not_orphan_excluded_subtree_sidecar() {
    let base = Dir::temp();
    let parent = base.p().join("parent");
    let keep = parent.join("keep");
    let excluded = parent.join("excluded");
    std::fs::create_dir_all(&keep).unwrap();
    std::fs::create_dir_all(&excluded).unwrap();
    write_sidecar(&ClipMetadata::new("EX1", 1.0), &excluded.join("x.mov"), None).unwrap();
    write_sidecar(&ClipMetadata::new("KP1", 1.0), &keep.join("k.mov"), None).unwrap();
    // Swift derives the key from the enumerated sidecar's clip path (the `/private/var` form): a missing clip's
    // path keeps `/private` when standardized, so that is the form step 7 compares.
    let ex_sidecar = glance_library::sidecar::scan_sidecars(&parent).into_keys().find(|p| s(p).contains("/excluded/")).unwrap();
    let ex_key = s(glance_library::sidecar::clip_path_from_sidecar(&ex_sidecar).unwrap().parent().unwrap());
    let (ix, st, _) = indexer();
    ix.handle_fs_event(std::slice::from_ref(&parent), std::slice::from_ref(&parent), &[ex_key].into_iter().collect());
    let rows = st.lock().unwrap().fetch_all().unwrap();
    let under_keep: Vec<&ClipRow> = rows.iter().filter(|r| r.current_path.contains("/keep/")).collect();
    assert_eq!(under_keep.len(), 1);
    assert!(under_keep[0].is_orphan);
    assert!(!rows.iter().any(|r| r.current_path.contains("/excluded/")));
}

#[test]
fn index_root_refuses_dangerous_and_excluded_roots() {
    let (ix, st, events) = indexer();
    for root in ["/", "/Users", "/Volumes", "/private/var/folders", "/tmp", "/System/Library"] {
        ix.index_root(Path::new(root), &none());
    }
    assert!(st.lock().unwrap().fetch_all().unwrap().is_empty());
    let dropped =
        events.lock().unwrap().iter().filter(|e| matches!(e, IndexerEvent::Diag(d) if d.starts_with("v0.6.29 indexRoot: DROPPED (dangerous root)"))).count();
    assert_eq!(dropped, 6);
    let d = Dir::rootable();
    d.file("a.mov", &[9]);
    let key = glance_library::roots::standardized_path(&s(d.p()));
    ix.index_root(d.p(), &[key].into_iter().collect());
    assert!(st.lock().unwrap().fetch_all().unwrap().is_empty());
}

#[test]
fn fs_events_outside_roots_are_dropped() {
    let d = Dir::temp();
    d.file("a.mov", &[1]);
    let other = Dir::temp();
    let (ix, st, _) = indexer();
    ix.handle_fs_event(&[d.p().to_path_buf()], &[other.p().to_path_buf()], &none());
    assert!(st.lock().unwrap().fetch_all().unwrap().is_empty());
    // A folder that no longer exists is skipped silently.
    ix.handle_fs_event(&[d.p().join("gone")], &[d.p().to_path_buf()], &none());
    assert!(st.lock().unwrap().fetch_all().unwrap().is_empty());
}

#[test]
fn known_folder_is_scanned_one_level_new_folder_recursively() {
    let d = Dir::temp();
    d.file("top.mov", &[1]);
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    assert_eq!(st.lock().unwrap().fetch_all().unwrap().len(), 1);
    // A subtree moved in: an event for the known root scans one level, so its clips stay unseen …
    d.file("moved/deep/inner.mov", &[2]);
    index_via_fs_event(&ix, d.p(), &none());
    assert_eq!(st.lock().unwrap().fetch_all().unwrap().len(), 1);
    // … and the event for the moved folder itself (no rows under it) walks it recursively.
    ix.handle_fs_event(&[d.p().join("moved")], &[d.p().to_path_buf()], &none());
    assert_eq!(st.lock().unwrap().fetch_all().unwrap().len(), 2);
}

#[test]
fn index_root_ingests_stashes_and_rebuild_finishes() {
    let d = Dir::rootable();
    let a = d.file("a.mov", &default_contents());
    let id = clip_id(&a).unwrap();
    let stash = Stash {
        id: "8F2C0D6E-1B7A-4C3D-9E8F-0A1B2C3D4E5F".into(),
        name: "Set".into(),
        created_at: 1.0,
        updated_at: 2.0,
        clips: vec![StashEntry { clip_id: id, relative_path_hint: "a.mov".into() }],
    };
    write_stash_file(std::slice::from_ref(&stash), d.p()).unwrap();
    let (ix, st, events) = indexer();
    ix.rebuild(vec![d.p().to_path_buf()], none());
    ix.wait_rebuild();
    assert_eq!(st.lock().unwrap().list_stashes().unwrap(), vec![stash]);
    assert_eq!(st.lock().unwrap().fetch_all().unwrap().len(), 1);
    let last = events.lock().unwrap().iter().rev().find_map(|e| if let IndexerEvent::Progress(p) = e { Some(p.clone()) } else { None }).unwrap();
    assert_eq!((last.root_name, last.phase), (None, Phase::Finished));
}

#[test]
fn a_new_rebuild_cancels_the_previous_one() {
    let d = Dir::rootable();
    for i in 0..40 {
        d.file(&format!("c{i}.mov"), &vec![i as u8; 1 << 16]);
    }
    let (ix, st, _) = indexer();
    ix.rebuild(vec![d.p().to_path_buf(); 5], none());
    ix.rebuild(vec![d.p().to_path_buf()], none());
    ix.wait_rebuild();
    // Whatever the first pass did, the second indexed everything.
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(st.lock().unwrap().fetch_all().unwrap().len(), 40);
}

// MARK: - Video size and backfill (VideoSizeTests, with glance-core's own fixtures)

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(name)
}

#[test]
fn indexer_stores_size_on_cache_miss() {
    let d = Dir::temp();
    let clip = d.p().join("tiles.mp4");
    std::fs::copy(fixture("tiles_h264.mp4"), &clip).unwrap();
    let want = glance_library::header::extract_info(&clip);
    let (ix, st, _) = indexer();
    index_via_fs_event(&ix, d.p(), &none());
    let row = st.lock().unwrap().fetch_at(&s(&clip)).unwrap().unwrap();
    let (w, h) = want.size.unwrap();
    assert_eq!((row.video_width, row.video_height), (Some(i64::from(w)), Some(i64::from(h))));
    assert_eq!(row.duration_sec, want.duration);
    assert_eq!(row.frame_rate, want.fps);
}

#[test]
fn indexer_backfills_null_sizes() {
    let d = Dir::temp();
    let video = d.p().join("p.mov");
    std::fs::copy(fixture("tiles_prores4444.mov"), &video).unwrap();
    let junk = d.file("junk.mov", &(0..=255u8).collect::<Vec<_>>());
    let (ix, st, _) = indexer();
    for p in [&video, &junk] {
        let m = std::fs::symlink_metadata(p).unwrap();
        let mut row = ClipRow::new(clip_id(p).unwrap(), s(p), m.len() as i64, glance_library::enumerate::foundation_mtime(&m).unwrap(), 1.0, 1.0);
        row.duration_sec = Some(1.0);
        row.frame_rate = Some(1.0);
        st.lock().unwrap().upsert(&row, &[]).unwrap();
    }
    index_via_fs_event(&ix, d.p(), &none());
    let want = glance_library::header::extract_info(&video).size.unwrap();
    let st = st.lock().unwrap();
    let v = st.fetch_at(&s(&video)).unwrap().unwrap();
    assert_eq!((v.video_width, v.video_height), (Some(i64::from(want.0)), Some(i64::from(want.1))));
    assert_eq!(v.frame_rate, Some(1.0), "a known rate is not re-read");
    let j = st.fetch_at(&s(&junk)).unwrap().unwrap();
    assert_eq!((j.video_width, j.video_height), (Some(0), Some(0)));
}

// MARK: - Settle (RetireOrphansTests and the excluded purge, through `settle`)

fn row(path: &str, id: &str, orphan: bool) -> ClipRow {
    let mut r = ClipRow::new(id, path, 1, 1.0, 1.0, 1.0);
    r.is_orphan = orphan;
    r
}

#[test]
fn settle_retires_moved_from_orphans_and_purges_excluded() {
    let st = store();
    let mut db = st.lock().unwrap();
    db.upsert(&row("/new/x.mov", "C1", false), &["livetag".into()]).unwrap();
    db.upsert(&row("/old/x.mov", "C1", true), &["orphantag".into()]).unwrap();
    db.upsert(&row("/gone/y.mov", "C2", true), &["losstag".into()]).unwrap();
    db.upsert(&row("/a/z.mov", "C3", false), &[]).unwrap();
    db.upsert(&row("/b/z.mov", "C3", false), &[]).unwrap();
    db.upsert(&row("/lib/Movies/Excluded/glenc.mov", "EX1", true), &["exTag".into()]).unwrap();
    db.upsert(&row("/lib/Movies/ExcludedX/keep.mov", "K1", true), &[]).unwrap();
    let rep = settle(&mut db, &["/lib/Movies/Excluded".to_string()].into_iter().collect());
    assert_eq!(rep.retired, vec!["/old/x.mov".to_string()]);
    assert_eq!(rep.purged, 1);
    assert!(db.fetch_by_path("/gone/y.mov").unwrap().is_some());
    assert!(db.fetch_by_path("/a/z.mov").unwrap().is_some() && db.fetch_by_path("/b/z.mov").unwrap().is_some());
    assert!(db.fetch_by_path("/lib/Movies/ExcludedX/keep.mov").unwrap().is_some());
    let tags = db.all_tags().unwrap();
    assert!(!tags.contains(&"orphantag".to_string()) && !tags.contains(&"exTag".to_string()));
    assert!(tags.contains(&"livetag".to_string()) && tags.contains(&"losstag".to_string()));
}

#[test]
fn status_text() {
    let p = |phase, completed, total| Progress { root_name: Some("Clips".into()), total, completed, phase };
    assert_eq!(indexing_status_text(&p(Phase::Enumerating, 0, 9), false), "Indexing Clips… enumerating");
    assert_eq!(indexing_status_text(&p(Phase::Hashing, 3, 9), false), "Indexing Clips… 3 of 9");
    assert_eq!(indexing_status_text(&p(Phase::Hashing, 3, 9), true), "Re-indexing for v0.6.33… Indexing Clips… 3 of 9");
    assert_eq!(indexing_status_text(&p(Phase::Finished, 9, 9), true), "");
}
