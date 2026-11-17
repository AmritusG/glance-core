//! What Swift MaCrate's `FSEventsWatcher` (CrateCore/FSEventsWatcher.swift) makes of one FSEvents callback: the
//! folders to re-index and the sidecars another process wrote. The stream itself is `glance-platform`'s
//! `FolderWatch` (file events, no defer, latency 0.5 s, since now); its batches go through [`classify`], and a
//! non-empty [`FsBatch`] goes to the indexer ([`crate::indexer::Indexer::handle_fs_event`], then
//! [`crate::indexer::Indexer::handle_external_sidecar`] per sidecar) and to the folder tree
//! ([`crate::folder_tree::FolderTree::invalidate`] per folder).

use crate::roots::standardized_path;
use crate::sidecar::SelfWriteTracker;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// `kFSEventStreamEventFlagRootChanged` (the value of `glance_platform::FS_EVENT_FLAG_ROOT_CHANGED`).
pub const FLAG_ROOT_CHANGED: u32 = 0x0000_0020;
/// `kFSEventStreamEventFlagUnmount` (the value of `glance_platform::FS_EVENT_FLAG_UNMOUNT`).
pub const FLAG_UNMOUNT: u32 = 0x0000_0080;

/// One FSEvents event: a path and its flags (`glance_platform::FsEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchEvent {
    pub path: PathBuf,
    pub flags: u32,
}

/// One classified callback (`FSEventBatch`). Sorted sets: Swift's are unordered, so any order is Swift's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FsBatch {
    /// Folders that changed themselves, parents of changed files, parent and grandparent (within the root) of
    /// vanished paths.
    pub touched_dirs: BTreeSet<PathBuf>,
    /// Sidecars written by another process (not this one within the last 2 s).
    pub external_sidecars: BTreeSet<PathBuf>,
    /// The diag lines Swift writes for dropped volume / root events.
    pub diag: Vec<String>,
}

impl FsBatch {
    /// Swift skips the main-queue hop when nothing survived.
    pub fn is_empty(&self) -> bool {
        self.touched_dirs.is_empty() && self.external_sidecars.is_empty()
    }
}

/// `url.deletingLastPathComponent()` of a file URL, as a path: the parent; `/` stays `/`.
fn parent(p: &Path) -> PathBuf {
    p.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
}

fn last_component(p: &Path) -> String {
    let s = p.to_string_lossy();
    let t = s.trim_end_matches('/');
    t.rsplit('/').next().unwrap_or(t).to_string()
}

/// `isSidecar`: `.<name>.crate-meta.json`.
pub fn is_sidecar_event(basename: &str) -> bool {
    basename.starts_with('.') && basename.ends_with(".crate-meta.json")
}

/// `isAtomicWriteTemp`: the sidecar writer's `.<uuid>.crate-meta.tmp` and Foundation's `.dat.nosync…`.
pub fn is_atomic_write_temp(basename: &str) -> bool {
    (basename.starts_with('.') && basename.ends_with(".crate-meta.tmp")) || basename.starts_with(".dat.nosync")
}

/// `isAlwaysIgnoredDotfile`: `.DS_Store`, `.localized`, `.Trashes`, `.git*`, `._*`.
pub fn is_always_ignored_dotfile(basename: &str) -> bool {
    matches!(basename, ".DS_Store" | ".localized" | ".Trashes") || basename.starts_with(".git") || basename.starts_with("._")
}

/// Classify one callback's events against the watched roots' standardized paths (`watch(roots:)` stores
/// [`standardized_path`] of each). Per event, in order: an unmount / root-changed flag → logged and dropped; an
/// atomic-write temp or an always-ignored dotfile → dropped; a sidecar → dropped when `tracker` says this process
/// wrote it, else an external sidecar plus its folder; any other dot-file → dropped; an existing folder → it and
/// its parent; an existing file → its parent; a vanished path → [`ascend_within_root`].
pub fn classify(events: &[WatchEvent], root_paths: &[String], tracker: Option<&SelfWriteTracker>) -> FsBatch {
    let mut batch = FsBatch::default();
    for ev in events {
        let path = &ev.path;
        let name = last_component(path);
        if ev.flags & (FLAG_UNMOUNT | FLAG_ROOT_CHANGED) != 0 {
            batch.diag.push(format!("FSEvents: dropping volume/root event flags=0x{:x} path={}", ev.flags, path.display()));
            continue;
        }
        if is_atomic_write_temp(&name) || is_always_ignored_dotfile(&name) {
            continue;
        }
        if is_sidecar_event(&name) {
            if tracker.is_some_and(|t| t.is_self_write(path)) {
                continue;
            }
            batch.external_sidecars.insert(path.clone());
            batch.touched_dirs.insert(parent(path));
            continue;
        }
        if name.starts_with('.') {
            continue;
        }
        match std::fs::metadata(path) {
            Ok(m) if m.is_dir() => {
                batch.touched_dirs.insert(path.clone());
                batch.touched_dirs.insert(parent(path));
            }
            Ok(_) => {
                batch.touched_dirs.insert(parent(path));
            }
            Err(_) => batch.touched_dirs.extend(ascend_within_root(path, root_paths)),
        }
    }
    batch
}

/// The watched roots' paths as [`classify`] compares them.
pub fn root_paths(roots: &[PathBuf]) -> Vec<String> {
    roots.iter().map(|r| standardized_path(&r.to_string_lossy())).collect()
}

fn within(p: &str, root: &str) -> bool {
    p == root || p.strip_prefix(root).is_some_and(|r| r.starts_with('/'))
}

/// `FSEventsWatcher.ascendWithinRoot(missing:rootPaths:)`: for a vanished path, its parent and grandparent — each
/// only when still at or under the smallest root that contains the path; nothing for a path under no root or for a
/// root itself (a volume going away never walks `/Volumes` or `/`).
pub fn ascend_within_root(missing: &Path, root_paths: &[String]) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    let mp = standardized_path(&missing.to_string_lossy());
    // Longest containing root; Swift's `max(by:)` keeps the first of equal lengths.
    let mut containing: Option<&String> = None;
    for rp in root_paths.iter().filter(|rp| within(&mp, rp)) {
        if containing.is_none_or(|c| rp.chars().count() > c.chars().count()) {
            containing = Some(rp);
        }
    }
    let Some(root) = containing else { return out };
    if mp == *root {
        return out;
    }
    let p = parent(missing);
    if within(&standardized_path(&p.to_string_lossy()), root) {
        out.insert(p.clone());
    }
    let g = parent(&p);
    if within(&standardized_path(&g.to_string_lossy()), root) {
        out.insert(g);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!("glance-watch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).ok();
        std::fs::canonicalize(&d).unwrap_or(d)
    }

    #[test]
    fn noise_is_dropped() {
        for n in [".DS_Store", ".localized", ".Trashes", ".git", ".gitignore", "._clip.mov", ".dat.nosync1a2b.x9", ".1234-ABCD.crate-meta.tmp", ".hidden"] {
            let b = classify(&[WatchEvent { path: PathBuf::from(format!("/Volumes/X/lib/{n}")), flags: 0 }], &["/Volumes/X/lib".into()], None);
            assert!(b.is_empty(), "{n}");
        }
    }

    #[test]
    fn volume_events_are_logged_and_dropped() {
        let b = classify(&[WatchEvent { path: PathBuf::from("/Volumes/X"), flags: FLAG_UNMOUNT }], &["/Volumes/X".into()], None);
        assert!(b.is_empty());
        assert_eq!(b.diag, vec!["FSEvents: dropping volume/root event flags=0x80 path=/Volumes/X".to_string()]);
        let b = classify(&[WatchEvent { path: PathBuf::from("/Volumes/X/a"), flags: FLAG_ROOT_CHANGED | 0x800 }], &["/Volumes/X".into()], None);
        assert!(b.is_empty() && b.diag.len() == 1);
    }

    #[test]
    fn sidecars_external_unless_self_written() {
        let d = scratch();
        let roots = vec![d.to_string_lossy().into_owned()];
        let sc = d.join(".a.mov.crate-meta.json");
        let b = classify(&[WatchEvent { path: sc.clone(), flags: 0 }], &roots, None);
        assert_eq!(b.external_sidecars.iter().collect::<Vec<_>>(), vec![&sc]);
        assert_eq!(b.touched_dirs.iter().collect::<Vec<_>>(), vec![&d]);
        let t = SelfWriteTracker::new();
        t.record(&sc);
        assert!(classify(&[WatchEvent { path: sc, flags: 0 }], &roots, Some(&t)).is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn dirs_files_and_vanished_paths() {
        let d = scratch();
        let root = d.join("root");
        std::fs::create_dir_all(root.join("sub/deep")).ok();
        std::fs::write(root.join("sub/a.mov"), b"x").ok();
        let roots = vec![standardized_path(&root.to_string_lossy())];
        let b = classify(&[WatchEvent { path: root.join("sub/deep"), flags: 0 }], &roots, None);
        assert_eq!(b.touched_dirs, [root.join("sub"), root.join("sub/deep")].into_iter().collect());
        let b = classify(&[WatchEvent { path: root.join("sub/a.mov"), flags: 0 }], &roots, None);
        assert_eq!(b.touched_dirs, [root.join("sub")].into_iter().collect());
        // Vanished paths (a volume that is not mounted here, so nothing exists): parent and grandparent, clamped.
        let vol = vec!["/Volumes/NoSuchVolume/lib".to_string()];
        let b = classify(&[WatchEvent { path: PathBuf::from("/Volumes/NoSuchVolume/lib/sub/deep/gone.mov"), flags: 0 }], &vol, None);
        assert_eq!(b.touched_dirs, [PathBuf::from("/Volumes/NoSuchVolume/lib/sub"), PathBuf::from("/Volumes/NoSuchVolume/lib/sub/deep")].into_iter().collect());
        // Parent is the root: the grandparent (above the root) is clamped away.
        let b = classify(&[WatchEvent { path: PathBuf::from("/Volumes/NoSuchVolume/lib/gone.mov"), flags: 0 }], &vol, None);
        assert_eq!(b.touched_dirs, [PathBuf::from("/Volumes/NoSuchVolume/lib")].into_iter().collect());
        // The root itself vanishing, and a path under no root: nothing.
        assert!(ascend_within_root(&root, &roots).is_empty());
        assert!(ascend_within_root(Path::new("/Volumes/Other/x.mov"), &roots).is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn ascent_never_leaves_a_volume_root() {
        // An unmounted top-level root must never produce /Volumes or /.
        let roots = vec!["/Volumes/ClipDrive".to_string()];
        assert!(ascend_within_root(Path::new("/Volumes/ClipDrive"), &roots).is_empty());
        let out = ascend_within_root(Path::new("/Volumes/ClipDrive/Clips/a.mov"), &roots);
        assert_eq!(out, [PathBuf::from("/Volumes/ClipDrive"), PathBuf::from("/Volumes/ClipDrive/Clips")].into_iter().collect());
        // Nested roots: the smallest containing root clamps.
        let nested = vec!["/Volumes/ClipDrive".to_string(), "/Volumes/ClipDrive/Clips".to_string()];
        let out = ascend_within_root(Path::new("/Volumes/ClipDrive/Clips/a.mov"), &nested);
        assert_eq!(out, [PathBuf::from("/Volumes/ClipDrive/Clips")].into_iter().collect());
        // A sibling named like the root is not under it.
        assert!(ascend_within_root(Path::new("/Volumes/ClipDriveX/a.mov"), &roots).is_empty());
    }
}
