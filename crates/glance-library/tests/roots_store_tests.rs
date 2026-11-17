//! The root list and the bookmark folder sets, ported from Swift MaCrate's LibraryRootStoreTests,
//! RecursionPreferenceTests and ExpansionStoreTests (same steps, same expectations). Bookmarks are a stand-in
//! (the path, resolvable while the folder exists — what makes a real bookmark of a deleted folder fail); one test
//! runs the real macOS bookmarks through glance-platform.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_library::roots::{LibraryRoot, RootSortMode, decode_bookmark_blobs, decode_library_roots, encode_library_roots, sort_roots};
use glance_library::roots_store::{AddRootResult, Bookmarks, ExpansionStore, LibraryRootStore, RecursionPreference, ResolvedBookmark};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// `bm:<path>` resolves while the folder exists; `stale:<path>` resolves stale.
struct FakeBookmarks;

impl Bookmarks for FakeBookmarks {
    fn resolve(&self, blob: &[u8]) -> Option<ResolvedBookmark> {
        let s = std::str::from_utf8(blob).ok()?;
        let (stale, path) = if let Some(p) = s.strip_prefix("stale:") { (true, p) } else { (false, s.strip_prefix("bm:")?) };
        Path::new(path).is_dir().then(|| ResolvedBookmark { path: PathBuf::from(path), stale })
    }
    fn create(&self, path: &Path) -> Option<Vec<u8>> {
        path.is_dir().then(|| format!("bm:{}", path.display()).into_bytes())
    }
}

struct Env {
    dir: PathBuf,
    store: PathBuf,
    sort: PathBuf,
}

impl Env {
    fn new() -> Env {
        let dir = std::env::temp_dir().join(format!("crate-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::fs::canonicalize(&dir).unwrap();
        let store = dir.join("roots-store.json");
        std::fs::write(&store, "[]").unwrap();
        Env { sort: dir.join("root-sort.json"), dir, store }
    }
    fn open(&self) -> LibraryRootStore {
        LibraryRootStore::load(&self.store, &self.sort, &[], Arc::new(FakeBookmarks))
    }
    fn folder(&self, name: &str) -> PathBuf {
        let p = self.dir.join(name);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn seed(&self, roots: &[LibraryRoot]) {
        std::fs::write(&self.store, encode_library_roots(roots).unwrap()).unwrap();
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn root(name: &str, folder: &Path, date: f64) -> LibraryRoot {
    LibraryRoot { id: Uuid::new_v4(), display_name: name.into(), bookmark_data: format!("bm:{}", folder.display()).into_bytes(), date_added: date }
}

fn names(r: &[LibraryRoot]) -> Vec<&str> {
    r.iter().map(|r| r.display_name.as_str()).collect()
}

#[test]
fn add_root_persists_across_instances() {
    let e = Env::new();
    let mut s1 = e.open();
    assert!(s1.roots().is_empty());
    s1.add_root(&e.dir).unwrap();
    assert_eq!(s1.roots().len(), 1);
    assert_eq!(s1.roots()[0].display_name, e.dir.file_name().unwrap().to_string_lossy());
    let s2 = e.open();
    assert_eq!(s2.roots().len(), 1);
    assert_eq!((s2.roots()[0].id, &s2.roots()[0].display_name), (s1.roots()[0].id, &s1.roots()[0].display_name));
}

#[test]
fn bookmark_resolves_to_original_folder_within_session() {
    let e = Env::new();
    let mut s = e.open();
    s.add_root(&e.dir).unwrap();
    assert_eq!(s.url_for(s.roots()[0].id), Some(e.dir.as_path()));
}

#[test]
fn remove_root_persists() {
    let e = Env::new();
    let mut s1 = e.open();
    s1.add_root(&e.dir).unwrap();
    let id = s1.roots()[0].id;
    s1.remove_root(id);
    assert!(s1.roots().is_empty());
    assert!(e.open().roots().is_empty());
}

#[test]
fn merge_dedup_by_key_first_wins() {
    let r = |n: &str| LibraryRoot { id: Uuid::new_v4(), display_name: n.into(), bookmark_data: Vec::new(), date_added: 0.0 };
    let (a1, b, a2, g) = (r("Alpha"), r("Beta"), r("Alpha"), r("Gamma"));
    let merged = glance_library::roots::merge_dedup(&[vec![a1.clone(), b], vec![a2, g]], |r| r.display_name.clone());
    assert_eq!(names(&merged), vec!["Alpha", "Beta", "Gamma"]);
    assert_eq!(merged[0].id, a1.id);
}

#[test]
fn reresolve_returns_path_for_resolvable_root() {
    let e = Env::new();
    let f = e.folder("root-A");
    let mut s = e.open();
    s.add_root(&f).unwrap();
    let id = s.roots()[0].id;
    assert!(s.url_for(id).is_some());
    assert!(s.reresolve(id).is_some());
    assert!(s.url_for(id).is_some());
}

#[test]
fn reresolve_returns_none_for_unresolvable_root() {
    let e = Env::new();
    let f = e.folder("root-B");
    let mut s1 = e.open();
    s1.add_root(&f).unwrap();
    let id = s1.roots()[0].id;
    std::fs::remove_dir_all(&f).unwrap();
    let mut s2 = e.open();
    assert!(s2.roots().iter().any(|r| r.id == id));
    assert!(s2.url_for(id).is_none(), "offline root loads as unavailable");
    assert!(s2.reresolve(id).is_none());
    assert!(s2.url_for(id).is_none());
}

#[test]
fn add_same_folder_twice_refuses_duplicate() {
    let e = Env::new();
    let f = e.folder("dup");
    let mut s = e.open();
    assert!(matches!(s.add_root(&f).unwrap(), AddRootResult::Added { .. }));
    assert_eq!(s.add_root(&f).unwrap(), AddRootResult::AlreadyAttached);
    // A trailing slash or a `..` spelling is the same folder.
    assert_eq!(s.add_root(&e.dir.join("dup/../dup/")).unwrap(), AddRootResult::AlreadyAttached);
    assert_eq!(s.roots().len(), 1);
}

#[test]
fn add_two_distinct_folders_both_added() {
    let e = Env::new();
    let (a, b) = (e.folder("A"), e.folder("B"));
    let mut s = e.open();
    assert!(matches!(s.add_root(&a).unwrap(), AddRootResult::Added { .. }));
    assert!(matches!(s.add_root(&b).unwrap(), AddRootResult::Added { .. }));
    assert_eq!(s.roots().len(), 2);
}

#[test]
fn offline_root_not_matched_so_re_add_appends_by_design() {
    let e = Env::new();
    let f = e.folder("vol");
    e.open().add_root(&f).unwrap();
    std::fs::remove_dir_all(&f).unwrap();
    let mut s2 = e.open();
    assert_eq!(s2.roots().len(), 1);
    assert!(s2.url_for(s2.roots()[0].id).is_none());
    std::fs::create_dir_all(&f).unwrap();
    assert!(matches!(s2.add_root(&f).unwrap(), AddRootResult::Added { .. }));
    assert_eq!(s2.roots().len(), 2);
}

#[test]
fn mark_volume_unavailable_flips_only_roots_on_that_volume() {
    let e = Env::new();
    let vol = e.folder("vol");
    let on = e.folder("vol/root");
    let off = e.folder("other");
    let mut s = e.open();
    s.add_root(&on).unwrap();
    s.add_root(&off).unwrap();
    let id = |n: &str, s: &LibraryRootStore| s.roots().iter().find(|r| r.display_name == n).unwrap().id;
    let (r_on, r_off) = (id("root", &s), id("other", &s));
    assert!(s.mark_volume_unavailable(&vol));
    assert!(s.url_for(r_on).is_none());
    assert!(s.url_for(r_off).is_some());
}

#[test]
fn reresolve_unavailable_restores_present_roots_only() {
    let e = Env::new();
    let vol = e.folder("vol");
    let present = e.folder("vol/present");
    let gone = e.folder("vol/gone");
    let mut s = e.open();
    s.add_root(&present).unwrap();
    s.add_root(&gone).unwrap();
    let id = |n: &str, s: &LibraryRootStore| s.roots().iter().find(|r| r.display_name == n).unwrap().id;
    let (rp, rg) = (id("present", &s), id("gone", &s));
    s.mark_volume_unavailable(&vol);
    assert!(s.url_for(rp).is_none() && s.url_for(rg).is_none());
    std::fs::remove_dir_all(&gone).unwrap();
    assert!(s.reresolve_unavailable());
    assert!(s.url_for(rp).is_some());
    assert!(s.url_for(rg).is_none());
}

#[test]
fn collapse_keeps_one_for_same_canonical_path() {
    let e = Env::new();
    let f = e.folder("ClipDrive");
    e.seed(&[root("ClipDrive", &f, 1.0), root("ClipDrive", &f, 2.0), root("ClipDrive", &f, 3.0)]);
    let mut s = e.open();
    assert_eq!(s.roots().len(), 1);
    assert_eq!(decode_library_roots(&std::fs::read(&e.store).unwrap()).unwrap().len(), 1, "collapse persisted");
    assert!(s.collapse_resolved_duplicates().is_empty());
}

#[test]
fn collapse_leaves_distinct_paths() {
    let e = Env::new();
    let (a, b) = (e.folder("A"), e.folder("B"));
    e.seed(&[root("A", &a, 1.0), root("B", &b, 2.0)]);
    assert_eq!(e.open().roots().len(), 2);
}

#[test]
fn collapse_leaves_offline_dupes_distinct() {
    let e = Env::new();
    let f = e.folder("Off");
    e.seed(&[root("Off", &f, 1.0), root("Off", &f, 2.0)]);
    std::fs::remove_dir_all(&f).unwrap();
    let s = e.open();
    assert_eq!(s.roots().len(), 2);
    assert!(s.url_for(s.roots()[0].id).is_none() && s.url_for(s.roots()[1].id).is_none());
}

#[test]
fn add_child_and_parent_of_existing_root_add_with_overlap() {
    let e = Env::new();
    let parent = e.folder("parent");
    let child = e.folder("parent/child");
    let mut s = e.open();
    s.add_root(&parent).unwrap();
    assert!(matches!(s.add_root(&child).unwrap(), AddRootResult::Added { overlaps_existing: true, .. }));
    assert_eq!(s.roots().len(), 2);
    let e2 = Env::new();
    let parent = e2.folder("parent");
    let child = e2.folder("parent/child");
    let mut s2 = e2.open();
    s2.add_root(&child).unwrap();
    assert!(matches!(s2.add_root(&parent).unwrap(), AddRootResult::Added { overlaps_existing: true, .. }));
}

#[test]
fn add_unrelated_folder_no_overlap() {
    let e = Env::new();
    let (a, b) = (e.folder("A"), e.folder("B"));
    let mut s = e.open();
    s.add_root(&a).unwrap();
    assert!(matches!(s.add_root(&b).unwrap(), AddRootResult::Added { overlaps_existing: false, .. }));
}

fn seeded_three(e: &Env) -> LibraryRootStore {
    let (fa, fb, fc) = (e.folder("dirA"), e.folder("dirB"), e.folder("dirC"));
    let t0 = 1_000.0 - glance_library::enumerate::SECONDS_1970_TO_2001;
    e.seed(&[root("Charlie", &fc, t0 + 300.0), root("alpha", &fa, t0), root("Bravo", &fb, t0 + 150.0)]);
    e.open()
}

#[test]
fn sorted_roots_name_and_date() {
    let e = Env::new();
    let mut s = seeded_three(&e);
    let manual = vec!["Charlie", "alpha", "Bravo"];
    assert_eq!(names(s.roots()), manual);
    let none = HashMap::new();
    let check = |s: &mut LibraryRootStore, m: RootSortMode, want: Vec<&str>| {
        s.set_sort_mode(m);
        assert_eq!(names(&s.sorted_roots(&none)), want, "{m:?}");
    };
    check(&mut s, RootSortMode::Manual, manual.clone());
    check(&mut s, RootSortMode::NameAsc, vec!["alpha", "Bravo", "Charlie"]);
    check(&mut s, RootSortMode::NameDesc, vec!["Charlie", "Bravo", "alpha"]);
    check(&mut s, RootSortMode::DateAddedAsc, vec!["alpha", "Bravo", "Charlie"]);
    check(&mut s, RootSortMode::DateAddedDesc, vec!["Charlie", "Bravo", "alpha"]);
    assert_eq!(names(s.roots()), manual, "stored order untouched by sorting");
}

#[test]
fn move_roots_reorders_and_persists() {
    let e = Env::new();
    let mut s = seeded_three(&e);
    s.move_roots(&[0], 3);
    assert_eq!(names(s.roots()), vec!["alpha", "Bravo", "Charlie"]);
    assert_eq!(names(e.open().roots()), vec!["alpha", "Bravo", "Charlie"]);
}

#[test]
fn sort_mode_persists() {
    let e = Env::new();
    let mut s1 = seeded_three(&e);
    assert_eq!(s1.sort_mode(), RootSortMode::Manual);
    s1.set_sort_mode(RootSortMode::NameDesc);
    assert_eq!(std::fs::read_to_string(&e.sort).unwrap(), "\"nameDesc\"");
    assert_eq!(e.open().sort_mode(), RootSortMode::NameDesc);
}

#[test]
fn root_sorting_size_mode_nil_last() {
    let r = |n: &str| LibraryRoot { id: Uuid::new_v4(), display_name: n.into(), bookmark_data: Vec::new(), date_added: 0.0 };
    let (a, b, c) = (r("A"), r("B"), r("C"));
    let roots = vec![a.clone(), b.clone(), c];
    let sizes: HashMap<Uuid, i64> = [(a.id, 300), (b.id, 100)].into_iter().collect();
    assert_eq!(names(&sort_roots(&roots, RootSortMode::SizeAsc, &sizes)), vec!["B", "A", "C"]);
    assert_eq!(names(&sort_roots(&roots, RootSortMode::SizeDesc, &sizes)), vec!["A", "B", "C"]);
    assert_eq!(names(&sort_roots(&roots, RootSortMode::NameAsc, &sizes)), vec!["A", "B", "C"]);
    assert_eq!(names(&sort_roots(&roots, RootSortMode::Manual, &sizes)), vec!["A", "B", "C"]);
}

#[test]
fn legacy_migration_merges_domains_and_seeds_the_file() {
    let e = Env::new();
    std::fs::remove_file(&e.store).unwrap();
    let (a, b) = (e.folder("A"), e.folder("B"));
    let gone = e.dir.join("gone");
    let ra = root("A", &a, 1.0);
    let rb = root("B", &b, 2.0);
    let ra_again = root("A", &a, 3.0); // the same folder from another domain: dropped
    let off1 = root("gone", &gone, 4.0);
    let off2 = root("gone", &gone, 5.0); // unresolvable: kept distinct (keyed by id)
    let legacy = [
        Some(encode_library_roots(&[ra.clone(), off1.clone()]).unwrap().into_bytes()),
        None,
        Some(encode_library_roots(&[ra_again, rb.clone(), off2.clone()]).unwrap().into_bytes()),
    ];
    let s = LibraryRootStore::load(&e.store, &e.sort, &legacy, Arc::new(FakeBookmarks));
    let ids: Vec<Uuid> = s.roots().iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![ra.id, off1.id, rb.id, off2.id]);
    assert_eq!(decode_library_roots(&std::fs::read(&e.store).unwrap()).unwrap().len(), 4, "file seeded");
    // A present file is never migrated over, even when it does not decode.
    std::fs::write(&e.store, "not json").unwrap();
    assert!(LibraryRootStore::load(&e.store, &e.sort, &legacy, Arc::new(FakeBookmarks)).roots().is_empty());
}

#[test]
fn stale_bookmarks_are_refreshed_and_saved() {
    let e = Env::new();
    let f = e.folder("Stale");
    let mut r = root("Stale", &f, 1.0);
    r.bookmark_data = format!("stale:{}", f.display()).into_bytes();
    e.seed(std::slice::from_ref(&r));
    let s = e.open();
    assert_eq!(s.roots()[0].bookmark_data, format!("bm:{}", f.display()).into_bytes());
    let on_disk = decode_library_roots(&std::fs::read(&e.store).unwrap()).unwrap();
    assert_eq!(on_disk[0].bookmark_data, s.roots()[0].bookmark_data);
}

// MARK: - RecursionPreference / ExpansionStore

fn prefs_file(e: &Env, name: &str) -> PathBuf {
    let p = e.dir.join(name);
    std::fs::write(&p, "[]").unwrap();
    p
}

#[test]
fn recursion_default_is_recursive_and_persists() {
    let e = Env::new();
    let file = prefs_file(&e, "recursion-store.json");
    let p = RecursionPreference::load(&file, &[], Arc::new(FakeBookmarks));
    assert!(p.is_recursive(&e.dir));
    let mut p1 = RecursionPreference::load(&file, &[], Arc::new(FakeBookmarks));
    p1.set_recursive(false, &e.dir);
    assert!(!p1.is_recursive(&e.dir));
    let p2 = RecursionPreference::load(&file, &[], Arc::new(FakeBookmarks));
    assert!(!p2.is_recursive(&e.dir), "direct-only persists via the store file");
}

#[test]
fn recursion_toggle_round_trip() {
    let e = Env::new();
    let file = prefs_file(&e, "recursion-store.json");
    let mut p = RecursionPreference::load(&file, &[], Arc::new(FakeBookmarks));
    p.toggle(&e.dir);
    assert!(!p.is_recursive(&e.dir));
    p.toggle(&e.dir);
    assert!(p.is_recursive(&e.dir));
}

#[test]
fn expansion_default_collapsed_persists_and_collapse_removes() {
    let e = Env::new();
    let file = prefs_file(&e, "expansion-store.json");
    assert!(!ExpansionStore::load(&file, &[], Arc::new(FakeBookmarks)).is_expanded(&e.dir));
    let mut s1 = ExpansionStore::load(&file, &[], Arc::new(FakeBookmarks));
    s1.set_expanded(true, &e.dir);
    assert!(s1.is_expanded(&e.dir));
    assert!(ExpansionStore::load(&file, &[], Arc::new(FakeBookmarks)).is_expanded(&e.dir));
    s1.set_expanded(false, &e.dir);
    assert!(!ExpansionStore::load(&file, &[], Arc::new(FakeBookmarks)).is_expanded(&e.dir));
    assert_eq!(decode_bookmark_blobs(&std::fs::read(&file).unwrap()).unwrap().len(), 0);
}

#[test]
fn folder_set_migration_prunes_and_dedups() {
    let e = Env::new();
    let a = e.folder("A");
    let file = e.dir.join("expanded-folders.json");
    let blob = |p: &Path| format!("bm:{}", p.display()).into_bytes();
    let enc = |v: &[Vec<u8>]| Some(glance_library::roots::encode_bookmark_blobs(v).into_bytes());
    let legacy = [enc(&[blob(&a), blob(&e.dir.join("gone"))]), enc(&[blob(&a)]), enc(&[format!("stale:{}", a.display()).into_bytes()])];
    let s = ExpansionStore::load(&file, &legacy, Arc::new(FakeBookmarks));
    assert!(s.is_expanded(&a));
    // The later (stale) blob for the same folder wins and is recreated from the folder's canonical path.
    let fresh = blob(Path::new(&glance_library::roots::canonical_path(&a.to_string_lossy())));
    assert_eq!(decode_bookmark_blobs(&std::fs::read(&file).unwrap()).unwrap(), vec![fresh], "unresolvable dropped, one per folder, stale refreshed");
}

/// The real thing: macOS bookmarks through glance-platform, as the app wires them.
#[cfg(target_os = "macos")]
#[test]
fn real_bookmarks_resolve_and_go_unavailable() {
    struct Real;
    impl Bookmarks for Real {
        fn resolve(&self, blob: &[u8]) -> Option<ResolvedBookmark> {
            glance_platform::bookmark_resolve(blob).ok().map(|r| ResolvedBookmark { path: r.path, stale: r.stale })
        }
        fn create(&self, path: &Path) -> Option<Vec<u8>> {
            glance_platform::bookmark_create(path).ok()
        }
    }
    let e = Env::new();
    let f = e.folder("Real Root");
    let mut s = LibraryRootStore::load(&e.store, &e.sort, &[], Arc::new(Real));
    s.add_root(&f).unwrap();
    let id = s.roots()[0].id;
    let s2 = LibraryRootStore::load(&e.store, &e.sort, &[], Arc::new(Real));
    assert_eq!(glance_library::roots::canonical_path(&s2.url_for(id).unwrap().to_string_lossy()), glance_library::roots::canonical_path(&f.to_string_lossy()));
    std::fs::remove_dir_all(&f).unwrap();
    let s3 = LibraryRootStore::load(&e.store, &e.sort, &[], Arc::new(Real));
    assert!(s3.url_for(id).is_none(), "a deleted folder's bookmark does not resolve");
}
