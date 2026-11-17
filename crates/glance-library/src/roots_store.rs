//! The library-root list and the two bookmark-backed folder sets as Swift MaCrate keeps them in Application Support
//! (CrateCore/LibraryRootStore.swift, RecursionPreference.swift, ExpansionStore.swift, BookmarkBlobMigration.swift):
//! loading with bookmark resolution (an unresolvable root stays in the list as unavailable), stale bookmarks
//! refreshed and saved, same-folder duplicates collapsed, add / remove / reorder / sort, volume unmount and remount,
//! and the one-time migration from the legacy `UserDefaults` values (the app reads those from the three domains
//! Swift read — its own, `com.amritus.crate` and `Crate` — and hands the raw values in). Every write is the whole
//! file through a temporary file and a rename, the bytes [`crate::roots`] encodes.
//!
//! Bookmarks are Apple's opaque blobs: resolving and creating them is the platform's (the app passes an adapter over
//! `glance_platform::bookmark_resolve` / `bookmark_create` as [`Bookmarks`]).

use crate::roots::{
    AddRootOutcome, LibraryRoot, RootSortMode, canonical_path, decode_bookmark_blobs, decode_library_roots, decode_root_sort, decode_show_sizes,
    encode_bookmark_blobs, encode_library_roots, encode_root_sort, encode_show_sizes, is_contained, is_contained_canonical, last_path_component, merge_dedup,
    move_roots, sort_roots,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// A resolved bookmark: the folder's path now, and whether the blob should be recreated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBookmark {
    pub path: PathBuf,
    pub stale: bool,
}

/// Bookmark resolution and creation, injected (`URL(resolvingBookmarkData:options: [] …)`,
/// `url.bookmarkData(options: [] …)`).
pub trait Bookmarks: Send + Sync {
    /// `None` when the bookmark does not resolve (an unmounted volume, a deleted folder).
    fn resolve(&self, blob: &[u8]) -> Option<ResolvedBookmark>;
    /// `None` when no bookmark can be made.
    fn create(&self, path: &Path) -> Option<Vec<u8>>;
}

/// [`Bookmarks`] from two closures.
pub struct FnBookmarks<R, C> {
    pub resolve: R,
    pub create: C,
}

impl<R, C> Bookmarks for FnBookmarks<R, C>
where
    R: Fn(&[u8]) -> Option<ResolvedBookmark> + Send + Sync,
    C: Fn(&Path) -> Option<Vec<u8>> + Send + Sync,
{
    fn resolve(&self, blob: &[u8]) -> Option<ResolvedBookmark> {
        (self.resolve)(blob)
    }
    fn create(&self, path: &Path) -> Option<Vec<u8>> {
        (self.create)(path)
    }
}

/// `Data.write(to:options: .atomic)` after creating the folder: a temporary file beside the target, then a rename.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("/"));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = dir.join(format!(".{name}.{}.tmp", Uuid::new_v4().simple()));
    std::fs::write(&temp, bytes)?;
    if let Err(e) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    Ok(())
}

/// Seconds since 2001 now (Foundation's `Date()` as `JSONEncoder` stores it).
fn now_since_2001() -> f64 {
    crate::store::now_seconds() - crate::enumerate::SECONDS_1970_TO_2001
}

fn uuid_string(id: Uuid) -> String {
    id.hyphenated().encode_upper(&mut Uuid::encode_buffer()).to_string()
}

/// What adding a folder did (`LibraryRootStore.AddRootResult`).
#[derive(Clone, Debug, PartialEq)]
pub enum AddRootResult {
    Added { root: LibraryRoot, overlaps_existing: bool },
    AlreadyAttached,
}

/// Why a folder could not be added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddRootError {
    /// No bookmark could be made for it.
    NoBookmark,
}

impl std::fmt::Display for AddRootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("could not make a bookmark for the folder")
    }
}

impl std::error::Error for AddRootError {}

/// `LibraryRootStore`.
pub struct LibraryRootStore {
    roots: Vec<LibraryRoot>,
    resolved: HashMap<Uuid, PathBuf>,
    sort_mode: RootSortMode,
    store_path: PathBuf,
    sort_path: PathBuf,
    bookmarks: Arc<dyn Bookmarks>,
}

impl std::fmt::Debug for LibraryRootStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryRootStore")
            .field("roots", &self.roots)
            .field("resolved", &self.resolved)
            .field("sort_mode", &self.sort_mode)
            .finish_non_exhaustive()
    }
}

impl LibraryRootStore {
    /// `init(defaults:storeURL:sortModeURL:)`: read `store_path` (`library-roots.json`; a file that does not decode
    /// is an empty list, never migrated over), or — when it does not exist — merge the legacy values (`[LibraryRoot]`
    /// JSON from the `com.amritus.crate.libraryRoots` default of each domain, in Swift's order: the app's own,
    /// `com.amritus.crate`, `Crate`), de-duplicated by resolved canonical path, and write the file. Then resolve
    /// every root, save refreshed stale bookmarks, collapse roots resolving to one folder, and read the sort mode
    /// (`root-sort.json`).
    pub fn load(store_path: &Path, sort_path: &Path, legacy: &[Option<Vec<u8>>], bookmarks: Arc<dyn Bookmarks>) -> LibraryRootStore {
        let mut store = LibraryRootStore {
            roots: Vec::new(),
            resolved: HashMap::new(),
            sort_mode: decode_root_sort(std::fs::read(sort_path).ok().as_deref()),
            store_path: store_path.to_path_buf(),
            sort_path: sort_path.to_path_buf(),
            bookmarks,
        };
        let mut decoded = if store_path.exists() {
            std::fs::read(store_path).ok().and_then(|b| decode_library_roots(&b)).unwrap_or_default()
        } else {
            let lists: Vec<Vec<LibraryRoot>> = legacy.iter().map(|v| v.as_deref().and_then(decode_library_roots).unwrap_or_default()).collect();
            let merged = merge_dedup(&lists, |r| store.dedup_key(r));
            store.write(&merged);
            merged
        };
        let mut any_stale = false;
        for root in decoded.iter_mut() {
            let (path, refreshed) = store.resolve_root(root);
            let Some(path) = path else { continue };
            store.resolved.insert(root.id, path);
            if let Some(fresh) = refreshed {
                *root = fresh;
                any_stale = true;
            }
        }
        store.roots = decoded;
        if any_stale {
            store.save();
        }
        store.collapse_resolved_duplicates();
        store
    }

    fn dedup_key(&self, root: &LibraryRoot) -> String {
        match self.bookmarks.resolve(&root.bookmark_data) {
            Some(r) => canonical_path(&r.path.to_string_lossy()),
            None => format!("id:{}", uuid_string(root.id)),
        }
    }

    /// `resolveRoot`: the path (`None` when unresolvable) and, for a stale bookmark that could be recreated, the
    /// root with the fresh bookmark.
    fn resolve_root(&self, root: &LibraryRoot) -> (Option<PathBuf>, Option<LibraryRoot>) {
        let Some(r) = self.bookmarks.resolve(&root.bookmark_data) else { return (None, None) };
        if r.stale
            && let Some(fresh) = self.bookmarks.create(&r.path)
        {
            let refreshed = LibraryRoot { bookmark_data: fresh, ..root.clone() };
            return (Some(r.path), Some(refreshed));
        }
        (Some(r.path), None)
    }

    fn write(&self, list: &[LibraryRoot]) {
        if let Some(json) = encode_library_roots(list) {
            let _ = write_atomically(&self.store_path, json.as_bytes());
        }
    }

    fn save(&self) {
        self.write(&self.roots);
    }

    /// The stored (manual) order.
    pub fn roots(&self) -> &[LibraryRoot] {
        &self.roots
    }

    /// `url(for:)`: the resolved folder, `None` while unavailable.
    pub fn url_for(&self, id: Uuid) -> Option<&Path> {
        self.resolved.get(&id).map(PathBuf::as_path)
    }

    /// `rootStore.roots.compactMap { rootStore.url(for: $0) }`: the available roots' folders, stored order — what
    /// the app indexes, watches and sizes.
    pub fn available_paths(&self) -> Vec<PathBuf> {
        self.roots.iter().filter_map(|r| self.resolved.get(&r.id).cloned()).collect()
    }

    pub fn sort_mode(&self) -> RootSortMode {
        self.sort_mode
    }

    /// Set and save the sort mode (`sortMode`'s `didSet`, which saves on every assignment).
    pub fn set_sort_mode(&mut self, mode: RootSortMode) {
        self.sort_mode = mode;
        let _ = write_atomically(&self.sort_path, encode_root_sort(mode).as_bytes());
    }

    /// `sortedRoots(sizes:)`: the display order, the stored order untouched.
    pub fn sorted_roots(&self, sizes: &HashMap<Uuid, i64>) -> Vec<LibraryRoot> {
        sort_roots(&self.roots, self.sort_mode, sizes)
    }

    /// `moveRoots(fromOffsets:toOffset:)`: drag reorder, saved.
    pub fn move_roots(&mut self, source: &[usize], destination: usize) {
        move_roots(&mut self.roots, source, destination);
        self.save();
    }

    /// `isAttached(_:)`: an available root is the same folder ([`canonical_path`]).
    pub fn is_attached(&self, path: &Path) -> bool {
        let c = canonical_path(&path.to_string_lossy());
        self.roots.iter().any(|r| self.resolved.get(&r.id).is_some_and(|p| canonical_path(&p.to_string_lossy()) == c))
    }

    /// `addRoot(url:)`: refuse a folder already attached (available roots only — an offline one is not compared,
    /// Swift's known limitation); otherwise bookmark it, append it (name = last path component, added now), save,
    /// and say whether it is a strict parent or child of an available root.
    pub fn add_root(&mut self, path: &Path) -> Result<AddRootResult, AddRootError> {
        let available: Vec<String> = self.roots.iter().filter_map(|r| self.resolved.get(&r.id)).map(|p| p.to_string_lossy().into_owned()).collect();
        let overlaps_existing = match crate::roots::add_root_outcome(&path.to_string_lossy(), available.iter().map(String::as_str)) {
            AddRootOutcome::AlreadyAttached => return Ok(AddRootResult::AlreadyAttached),
            AddRootOutcome::Added { overlaps_existing } => overlaps_existing,
        };
        let bookmark = self.bookmarks.create(path).ok_or(AddRootError::NoBookmark)?;
        let root = LibraryRoot {
            id: Uuid::new_v4(),
            display_name: last_path_component(&path.to_string_lossy()),
            bookmark_data: bookmark,
            date_added: now_since_2001(),
        };
        self.resolved.insert(root.id, path.to_path_buf());
        self.roots.push(root.clone());
        self.save();
        Ok(AddRootResult::Added { root, overlaps_existing })
    }

    /// `removeRoot(id:)` (its rows stay in the database).
    pub fn remove_root(&mut self, id: Uuid) {
        self.roots.retain(|r| r.id != id);
        self.resolved.remove(&id);
        self.save();
    }

    /// `reresolve(id:)`: Reconnect — resolve one root's bookmark again; on success it is available (a stale
    /// bookmark refreshed and saved) and the folder is returned for the app to index.
    pub fn reresolve(&mut self, id: Uuid) -> Option<PathBuf> {
        let idx = self.roots.iter().position(|r| r.id == id)?;
        let (path, refreshed) = self.resolve_root(&self.roots[idx]);
        let path = path?;
        self.resolved.insert(id, path.clone());
        if let Some(fresh) = refreshed {
            self.roots[idx] = fresh;
            self.save();
        }
        Some(path)
    }

    /// `markVolumeUnavailable(_:)`: every available root on the ejected volume becomes unavailable (nothing saved).
    /// Returns whether any did.
    pub fn mark_volume_unavailable(&mut self, volume: &Path) -> bool {
        let v = volume.to_string_lossy();
        let affected: Vec<Uuid> =
            self.roots.iter().filter(|r| self.resolved.get(&r.id).is_some_and(|p| is_contained(&p.to_string_lossy(), &v))).map(|r| r.id).collect();
        for id in &affected {
            self.resolved.remove(id);
        }
        !affected.is_empty()
    }

    /// `reresolveUnavailable()`: a volume mounted — resolve every unavailable root again (no indexing), save
    /// refreshed bookmarks, collapse duplicates. Returns whether any root became available.
    pub fn reresolve_unavailable(&mut self) -> bool {
        let pending: Vec<usize> = (0..self.roots.len()).filter(|&i| !self.resolved.contains_key(&self.roots[i].id)).collect();
        if pending.is_empty() {
            return false;
        }
        let (mut any, mut any_stale) = (false, false);
        for idx in pending {
            let (path, refreshed) = self.resolve_root(&self.roots[idx]);
            let Some(path) = path else { continue };
            any = true;
            self.resolved.insert(self.roots[idx].id, path);
            if let Some(fresh) = refreshed {
                self.roots[idx] = fresh;
                any_stale = true;
            }
        }
        if any_stale {
            self.save();
        }
        self.collapse_resolved_duplicates();
        any
    }

    /// `collapseResolvedDuplicates()`: drop every available root whose folder ([`canonical_path`]) an earlier one
    /// already has, save once; offline roots stay distinct. Returns the dropped ids.
    pub fn collapse_resolved_duplicates(&mut self) -> Vec<Uuid> {
        let mut seen = HashSet::new();
        let remove: Vec<Uuid> =
            self.roots.iter().filter(|r| self.resolved.get(&r.id).is_some_and(|p| !seen.insert(canonical_path(&p.to_string_lossy())))).map(|r| r.id).collect();
        if remove.is_empty() {
            return remove;
        }
        self.roots.retain(|r| !remove.contains(&r.id));
        for id in &remove {
            self.resolved.remove(id);
        }
        self.save();
        remove
    }

    /// `LibraryRootStore.isContained` for the app's selection checks: `candidate` is `root` or under it.
    pub fn contains(root: &Path, candidate: &Path) -> bool {
        is_contained_canonical(&canonical_path(&candidate.to_string_lossy()), &canonical_path(&root.to_string_lossy()))
    }
}

/// Read `root-size-display.json` (`true` when absent or unreadable).
pub fn load_show_sizes(path: &Path) -> bool {
    decode_show_sizes(std::fs::read(path).ok().as_deref())
}

/// Write `root-size-display.json`.
pub fn save_show_sizes(path: &Path, show: bool) -> std::io::Result<()> {
    write_atomically(path, encode_show_sizes(show).as_bytes())
}

// MARK: - Bookmark-backed folder sets

/// The folders of `RecursionPreference` (`recursion-prefs.json`, the direct-only folders) and `ExpansionStore`
/// (`expanded-folders.json`, the expanded sidebar rows): plain bookmarks, keyed by the folder's canonical path.
pub struct BookmarkFolderSet {
    entries: BTreeMap<String, Vec<u8>>,
    store_path: PathBuf,
    bookmarks: Arc<dyn Bookmarks>,
}

impl std::fmt::Debug for BookmarkFolderSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BookmarkFolderSet").field("folders", &self.entries.keys().collect::<Vec<_>>()).finish_non_exhaustive()
    }
}

/// `BookmarkBlobMigration.legacyBlobs`: the legacy `[Data]` values (each the JSON a domain holds under the key),
/// concatenated, exact byte duplicates dropped, first kept.
pub fn legacy_blobs(values: &[Option<Vec<u8>>]) -> Vec<Vec<u8>> {
    let mut seen = HashSet::new();
    values.iter().flat_map(|v| v.as_deref().and_then(decode_bookmark_blobs).unwrap_or_default()).filter(|b| seen.insert(b.clone())).collect()
}

/// Swift keys these sets by `url.standardizedFileURL.resolvingSymlinksInPath()`; here the NFC form of that
/// ([`canonical_path`]).
fn folder_key(path: &Path) -> String {
    canonical_path(&path.to_string_lossy())
}

impl BookmarkFolderSet {
    /// `load()`: the file, or (when it does not exist) the legacy values migrated; every bookmark resolved
    /// (unresolvable ones dropped, stale ones recreated, two for one folder kept once); saved when migrating, when a
    /// bookmark was refreshed, or when one was dropped.
    pub fn load(store_path: &Path, legacy: &[Option<Vec<u8>>], bookmarks: Arc<dyn Bookmarks>) -> BookmarkFolderSet {
        let migrating = !store_path.exists();
        let decoded = if migrating { legacy_blobs(legacy) } else { std::fs::read(store_path).ok().and_then(|b| decode_bookmark_blobs(&b)).unwrap_or_default() };
        let mut entries = BTreeMap::new();
        let mut any_stale = false;
        for blob in &decoded {
            let Some(r) = bookmarks.resolve(blob) else { continue };
            let canon = PathBuf::from(folder_key(&r.path));
            let key = canon.to_string_lossy().into_owned();
            if r.stale
                && let Some(fresh) = bookmarks.create(&canon)
            {
                entries.insert(key, fresh);
                any_stale = true;
            } else {
                entries.insert(key, blob.clone());
            }
        }
        let set = BookmarkFolderSet { entries, store_path: store_path.to_path_buf(), bookmarks };
        if migrating || any_stale || set.entries.len() != decoded.len() {
            set.save();
        }
        set
    }

    fn save(&self) {
        let blobs: Vec<Vec<u8>> = self.entries.values().cloned().collect();
        let _ = write_atomically(&self.store_path, encode_bookmark_blobs(&blobs).as_bytes());
    }

    pub fn contains(&self, folder: &Path) -> bool {
        self.entries.contains_key(&folder_key(folder))
    }

    /// Add `folder` (bookmarked; nothing changes when no bookmark can be made) and save.
    pub fn insert(&mut self, folder: &Path) {
        let canon = PathBuf::from(folder_key(folder));
        let Some(blob) = self.bookmarks.create(&canon) else { return };
        self.entries.insert(canon.to_string_lossy().into_owned(), blob);
        self.save();
    }

    /// Remove `folder` and save.
    pub fn remove(&mut self, folder: &Path) {
        self.entries.remove(&folder_key(folder));
        self.save();
    }

    /// The folders, canonical paths, sorted.
    pub fn folders(&self) -> Vec<PathBuf> {
        self.entries.keys().map(PathBuf::from).collect()
    }
}

/// `RecursionPreference`: folders shown direct-only in the grid (default: recursive). The legacy key is
/// `com.amritus.crate.directOnlyFolders`.
#[derive(Debug)]
pub struct RecursionPreference(pub BookmarkFolderSet);

impl RecursionPreference {
    pub const LEGACY_KEY: &'static str = "com.amritus.crate.directOnlyFolders";

    pub fn load(store_path: &Path, legacy: &[Option<Vec<u8>>], bookmarks: Arc<dyn Bookmarks>) -> RecursionPreference {
        RecursionPreference(BookmarkFolderSet::load(store_path, legacy, bookmarks))
    }

    pub fn is_recursive(&self, folder: &Path) -> bool {
        !self.0.contains(folder)
    }

    pub fn set_recursive(&mut self, recursive: bool, folder: &Path) {
        if recursive { self.0.remove(folder) } else { self.0.insert(folder) }
    }

    pub fn toggle(&mut self, folder: &Path) {
        let r = self.is_recursive(folder);
        self.set_recursive(!r, folder);
    }
}

/// `ExpansionStore`: expanded sidebar folders (default: collapsed). The legacy key is
/// `com.amritus.crate.expandedFolders`.
#[derive(Debug)]
pub struct ExpansionStore(pub BookmarkFolderSet);

impl ExpansionStore {
    pub const LEGACY_KEY: &'static str = "com.amritus.crate.expandedFolders";

    pub fn load(store_path: &Path, legacy: &[Option<Vec<u8>>], bookmarks: Arc<dyn Bookmarks>) -> ExpansionStore {
        ExpansionStore(BookmarkFolderSet::load(store_path, legacy, bookmarks))
    }

    pub fn is_expanded(&self, folder: &Path) -> bool {
        self.0.contains(folder)
    }

    pub fn set_expanded(&mut self, expanded: bool, folder: &Path) {
        if expanded { self.0.insert(folder) } else { self.0.remove(folder) }
    }
}
