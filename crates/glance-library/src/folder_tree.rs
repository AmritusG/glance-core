//! The sidebar's folder tree (CrateCore/FolderTree.swift): a folder's immediate subfolders, read lazily and cached
//! per folder; an FSEvent invalidates a folder, and only a real change of its subfolder list bumps the change
//! counter (0.16.4: a render writing frames into a watched folder must not redraw the sidebar).

use crate::enumerate::{is_hidden, localized_case_insensitive_cmp};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One subfolder (`FolderNode`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderNode {
    /// The parent's path as asked for, joined with the entry's on-disk name.
    pub path: PathBuf,
    /// `url.lastPathComponent`.
    pub display_name: String,
}

#[derive(Debug, Default)]
pub struct FolderTree {
    cache: HashMap<PathBuf, Vec<FolderNode>>,
    change_counter: u64,
}

impl FolderTree {
    pub fn new() -> FolderTree {
        FolderTree::default()
    }

    /// Bumped (wrapping) by every real change and by [`FolderTree::clear_cache`].
    pub fn change_counter(&self) -> u64 {
        self.change_counter
    }

    /// `children(of:)`: the cached list, read on first ask.
    pub fn children(&mut self, folder: &Path) -> Vec<FolderNode> {
        if let Some(c) = self.cache.get(folder) {
            return c.clone();
        }
        let fresh = list_subfolders(folder);
        self.cache.insert(folder.to_path_buf(), fresh.clone());
        fresh
    }

    /// `invalidate(_:)`: re-read a cached folder; returns whether its list changed (and bumps the counter then).
    /// A folder never listed is not read.
    pub fn invalidate(&mut self, folder: &Path) -> bool {
        let Some(cached) = self.cache.get(folder) else { return false };
        let fresh = list_subfolders(folder);
        if &fresh == cached {
            return false;
        }
        self.cache.insert(folder.to_path_buf(), fresh);
        self.change_counter = self.change_counter.wrapping_add(1);
        true
    }

    /// `clearCache()`.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.change_counter = self.change_counter.wrapping_add(1);
    }
}

/// `FolderTree.enumerate`: `contentsOfDirectory(…, options: .skipsHiddenFiles)` keeping entries whose
/// `isDirectory` is true (a symbolic link is not a directory; packages are), sorted by
/// [`localized_case_insensitive_cmp`] of the name. An unreadable folder has none.
pub fn list_subfolders(folder: &Path) -> Vec<FolderNode> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut out: Vec<FolderNode> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let meta = std::fs::symlink_metadata(e.path()).ok()?;
            (!is_hidden(&name, &meta) && meta.is_dir()).then(|| FolderNode { path: folder.join(&name), display_name: name })
        })
        .collect();
    out.sort_by(|a, b| localized_case_insensitive_cmp(&a.display_name, &b.display_name));
    out
}

#[cfg(test)]
mod tests {
    // FolderTreeInvalidateTests, same steps; plus what a listing keeps.
    use super::*;

    struct Base(PathBuf);

    impl Base {
        fn new() -> Base {
            let b = std::env::temp_dir().join(format!("crate.foldertree.{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(b.join("A")).ok();
            Base(b)
        }
    }

    impl Drop for Base {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn names(v: &[FolderNode]) -> Vec<&str> {
        v.iter().map(|n| n.display_name.as_str()).collect()
    }

    #[test]
    fn new_file_in_listed_folder_does_not_publish() {
        let b = Base::new();
        let mut tree = FolderTree::new();
        assert_eq!(names(&tree.children(&b.0)), vec!["A"]);
        let before = tree.change_counter();
        for i in 0..5 {
            std::fs::write(b.0.join(format!("frame{i}.png")), [1]).ok();
            tree.invalidate(&b.0);
        }
        assert_eq!(tree.change_counter(), before);
    }

    #[test]
    fn unlisted_folder_does_not_publish() {
        let b = Base::new();
        let mut tree = FolderTree::new();
        let before = tree.change_counter();
        std::fs::create_dir_all(b.0.join("A/New")).ok();
        tree.invalidate(&b.0.join("A"));
        assert_eq!(tree.change_counter(), before);
        assert_eq!(names(&tree.children(&b.0.join("A"))), vec!["New"]);
    }

    #[test]
    fn subfolder_added_and_removed_publishes_and_refreshes() {
        let b = Base::new();
        let mut tree = FolderTree::new();
        let _ = tree.children(&b.0);
        let before = tree.change_counter();
        std::fs::create_dir_all(b.0.join("B")).ok();
        tree.invalidate(&b.0);
        assert_eq!(tree.change_counter(), before.wrapping_add(1));
        assert_eq!(names(&tree.children(&b.0)), vec!["A", "B"]);
        std::fs::remove_dir_all(b.0.join("B")).ok();
        tree.invalidate(&b.0);
        assert_eq!(tree.change_counter(), before.wrapping_add(2));
        assert_eq!(names(&tree.children(&b.0)), vec!["A"]);
    }

    #[test]
    fn directories_only_hidden_and_links_skipped_sorted() {
        let b = Base::new();
        for n in ["beta", ".hidden", "clip10", "clip2", "Thing.app"] {
            std::fs::create_dir_all(b.0.join(n)).ok();
        }
        std::fs::write(b.0.join("file.mov"), [0]).ok();
        #[cfg(unix)]
        std::os::unix::fs::symlink(b.0.join("beta"), b.0.join("link")).ok();
        assert_eq!(names(&list_subfolders(&b.0)), vec!["A", "beta", "clip10", "clip2", "Thing.app"]);
    }
}
