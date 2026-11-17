// SPDX-License-Identifier: MIT
//! Folder expansion on disk, with SendEhrKit's own tree and expected orders.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use glance_arena::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Tree {
    root: PathBuf,
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn t0() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

impl Tree {
    fn new() -> Tree {
        Tree { root: common::temp_dir("media") }
    }

    fn file(&self, rel: &str, bytes: usize, modified: Option<SystemTime>) -> String {
        let p = self.root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, vec![0u8; bytes]).unwrap();
        if let Some(m) = modified {
            fs::File::options().write(true).open(&p).unwrap().set_modified(m).unwrap();
        }
        p.to_string_lossy().into_owned()
    }

    fn f(&self, rel: &str) -> String {
        self.file(rel, 1, None)
    }

    fn path(&self, rel: &str) -> String {
        self.root.join(rel).to_string_lossy().into_owned()
    }

    fn rel(&self, paths: &[String]) -> Vec<String> {
        let base = format!("{}/", self.root.to_string_lossy());
        paths.iter().map(|p| p.replacen(&base, "", 1)).collect()
    }

    /// Set/clip 10.mov, Set/clip 2.mov, Set/.hidden.mov, Set/notes.txt, Set/App.app/ (package),
    /// Set/Sub/b.mov, Set/Sub/a.png, Set/Sub/Deeper/z.mov, Set/Sub/.DS_Store
    fn make(&self) {
        for r in [
            "Set/clip 10.mov",
            "Set/clip 2.mov",
            "Set/.hidden.mov",
            "Set/notes.txt",
            "Set/App.app/Contents/Info.plist",
            "Set/Sub/b.mov",
            "Set/Sub/a.png",
            "Set/Sub/Deeper/z.mov",
            "Set/Sub/.DS_Store",
        ] {
            self.f(r);
        }
    }
}

fn collect(paths: &[String], sub: bool, key: SortKey, asc: bool) -> MediaCollection {
    MediaCollector::collect(paths, sub, key, asc)
}

#[test]
fn folder_without_subfolders() {
    let t = Tree::new();
    t.make();
    let c = collect(&[t.path("Set")], false, SortKey::Name, true);
    assert_eq!(t.rel(&c.files), ["Set/clip 2.mov", "Set/clip 10.mov"]);
    assert_eq!(c.skipped_hidden, 1);
    assert_eq!(c.skipped_non_media, 2); // notes.txt + App.app
    assert_eq!(c.subfolders_not_included, 1);
    assert_eq!(c.from_folders, 2);
    assert_eq!(c.summary(), "2 files (2 from 1 folder), skipped 1 hidden, 2 not a type Arena opens, 1 subfolder (Include Subfolders is off)");
}

#[test]
fn nested_folder_with_subfolders() {
    let t = Tree::new();
    t.make();
    let c = collect(&[t.path("Set")], true, SortKey::Name, true);
    assert_eq!(t.rel(&c.files), ["Set/clip 2.mov", "Set/clip 10.mov", "Set/Sub/a.png", "Set/Sub/b.mov", "Set/Sub/Deeper/z.mov"]);
    assert_eq!(c.skipped_hidden, 2); // .hidden.mov + Sub/.DS_Store
    assert_eq!(c.skipped_non_media, 2);
    assert_eq!(c.summary(), "5 files (5 from 1 folder), skipped 2 hidden, 2 not a type Arena opens");
}

#[test]
fn mixed_files_and_folders_follow_the_sort_setting() {
    let t = Tree::new();
    t.make();
    let loose = t.file("A loose.mov", 1, Some(t0() + Duration::from_secs(100)));
    let set = t.path("Set");
    // Name ascending: "A loose.mov" before "Set".
    let c = collect(&[set.clone(), loose.clone()], false, SortKey::Name, true);
    assert_eq!(t.rel(&c.files), ["A loose.mov", "Set/clip 2.mov", "Set/clip 10.mov"]);
    assert_eq!(c.folders.len(), 1);
    assert_eq!(c.from_folders, 2);
    // Name descending: the folder first, its contents descending too.
    let c = collect(&[loose, set], false, SortKey::Name, false);
    assert_eq!(t.rel(&c.files), ["Set/clip 10.mov", "Set/clip 2.mov", "A loose.mov"]);
}

#[test]
fn date_modified_descending_on_disk() {
    let t = Tree::new();
    for (n, s) in [("a", 1), ("b", 3), ("c", 2), ("d", 4)] {
        t.file(&format!("F/{n}.mov"), 1, Some(t0() + Duration::from_secs(s)));
    }
    let c = collect(&[t.path("F")], false, SortKey::DateModified, false);
    assert_eq!(t.rel(&c.files), ["F/d.mov", "F/b.mov", "F/c.mov", "F/a.mov"]);
}

#[test]
fn size_on_disk() {
    let t = Tree::new();
    t.file("S/big.mov", 3000, None);
    t.file("S/small.mov", 10, None);
    t.file("S/mid.mov", 500, None);
    let c = collect(&[t.path("S")], false, SortKey::Size, true);
    assert_eq!(t.rel(&c.files), ["S/small.mov", "S/mid.mov", "S/big.mov"]);
}

#[test]
fn date_added_comes_from_the_probe() {
    let t = Tree::new();
    for n in ["x", "y", "z"] {
        t.f(&format!("A/{n}.mov"));
    }
    let probe = |p: &Path| -> Option<SystemTime> {
        let order = ["y", "z", "x"];
        let stem = p.file_stem()?.to_string_lossy().into_owned();
        order.iter().position(|o| *o == stem).map(|i| t0() + Duration::from_secs(i as u64))
    };
    let c = MediaCollector::collect_with(&[t.path("A")], false, SortKey::DateAdded, true, Some(&probe));
    assert_eq!(t.rel(&c.files), ["A/y.mov", "A/z.mov", "A/x.mov"]);
}

#[test]
fn threshold_ask() {
    let t = Tree::new();
    for i in 1..=6 {
        t.f(&format!("Big/c{i}.mov"));
    }
    let c = collect(&[t.path("Big")], false, SortKey::Name, true);
    assert!(c.needs_confirmation(5, false));
    assert!(!c.needs_confirmation(6, false));
    // Loose files never ask, however many.
    let loose: Vec<String> = (1..=6).map(|i| t.path(&format!("Big/c{i}.mov"))).collect();
    assert!(!collect(&loose, false, SortKey::Name, true).needs_confirmation(5, false));
}

#[test]
fn selection_splits_folders_files_and_packages() {
    let t = Tree::new();
    t.make();
    let sel = MediaSelection::new(&[t.path("Set"), t.path("Set/clip 2.mov"), t.path("Set/notes.txt"), t.path("Set/App.app")]);
    assert_eq!(t.rel(&sel.folders), ["Set"]);
    assert_eq!(t.rel(&sel.files), ["Set/clip 2.mov"]);
    assert_eq!(sel.unsupported, 2);
}

#[test]
fn groups_on_disk_mixed_loose_empty_and_subfolders() {
    let t = Tree::new();
    t.make(); // Set: 2 own files, Sub: 2, Sub/Deeper: 1
    t.f("Other/o 1.mov");
    t.f("Other/o 2.mp4");
    t.f("Empty/readme.txt"); // no media
    let loose = t.f("loose.mov");
    let mut paths: Vec<String> = ["Set", "Other", "Empty"].iter().map(|p| t.path(p)).collect();
    paths.push(loose);
    let c = collect(&paths, false, SortKey::Name, true);
    assert_eq!(c.groups.iter().map(FolderGroup::name).collect::<Vec<_>>(), ["Empty", "Other", "Set"]);
    assert_eq!(
        c.groups.iter().map(|g| t.rel(&g.files)).collect::<Vec<_>>(),
        vec![vec![], vec!["Other/o 1.mov".to_string(), "Other/o 2.mp4".to_string()], vec!["Set/clip 2.mov".to_string(), "Set/clip 10.mov".to_string()]]
    );
    assert_eq!(c.loose_files(), 1);
    assert_eq!(c.from_folders, 4);
    // Subfolders on: they join their parent's group, own files first.
    let c = collect(&paths, true, SortKey::Name, true);
    assert_eq!(c.groups.len(), 3);
    assert_eq!(t.rel(&c.groups[2].files), ["Set/clip 2.mov", "Set/clip 10.mov", "Set/Sub/a.png", "Set/Sub/b.mov", "Set/Sub/Deeper/z.mov"]);
    let snap = CompositionSnapshot::new(
        "C",
        "D",
        vec![LayerInfo::new(1, 11, "Layer #", None, (1..=4).map(|c| SlotInfo::new(c, c as i64, None, "Empty")).collect())],
        (1..=4).map(|c| ColumnInfo::new(c, 100 + c as i64, "Column #", None)).collect(),
    );
    let plan = LoadPlanner::plan_layers(&c.groups, &snap).unwrap();
    assert_eq!(plan.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Other", "Set"]);
    assert_eq!(plan.layers.iter().map(|l| l.layer_index).collect::<Vec<_>>(), [3, 2]);
    assert_eq!(plan.add_columns, 1); // Set has 5 files, comp has 4 columns
    assert_eq!(plan.empty_folders.iter().map(|f| glance_arena::media::last_path_component(f)).collect::<Vec<_>>(), ["Empty"]);
}

#[test]
fn stacking_follows_sort_ascending_and_descending() {
    let t = Tree::new();
    for n in ["B", "A", "C"] {
        t.f(&format!("{n}/x.mov"));
    }
    let paths: Vec<String> = ["B", "A", "C"].iter().map(|p| t.path(p)).collect();
    let snap = CompositionSnapshot::new("C", "D", vec![LayerInfo::new(1, 11, "Layer #", None, vec![])], vec![]);
    for (asc, expected) in [(true, ["A", "B", "C"]), (false, ["C", "B", "A"])] {
        let c = collect(&paths, false, SortKey::Name, asc);
        let plan = LoadPlanner::plan_layers(&c.groups, &snap).unwrap();
        assert_eq!(plan.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), expected);
        // Top-most (highest index) is the first folder in sort order.
        let mut by_index = plan.layers.clone();
        by_index.sort_by_key(|l| std::cmp::Reverse(l.layer_index));
        assert_eq!(by_index.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), expected);
    }
}

#[test]
fn total_count_ask_across_folders() {
    let t = Tree::new();
    for i in 1..=3 {
        t.f(&format!("P/p{i}.mov"));
        t.f(&format!("Q/q{i}.mov"));
    }
    for i in 1..=4 {
        t.f(&format!("l{i}.mov"));
    }
    let mut paths: Vec<String> = ["P", "Q"].iter().map(|p| t.path(p)).collect();
    paths.extend((1..=4).map(|i| t.path(&format!("l{i}.mov"))));
    let c = collect(&paths, false, SortKey::Name, true);
    assert_eq!(c.from_folders, 6);
    assert_eq!(c.loose_files(), 4);
    // 3 + 3 = 6 > 5: asked once on the total, though no single folder is over.
    assert!(c.needs_confirmation(5, true));
    // Loose files (ignored per folder) don't count: 6 is not over 6.
    assert!(!c.needs_confirmation(6, true));
    assert!(c.needs_confirmation(6, false)); // consecutive-column loads count everything
}

#[cfg(unix)]
#[test]
fn symlinks_to_files_followed_to_folders_not() {
    let t = Tree::new();
    t.f("Real/a.mov");
    t.f("Src/b.mov");
    fs::create_dir_all(t.root.join("L")).unwrap();
    std::os::unix::fs::symlink(t.root.join("Src/b.mov"), t.root.join("L/link.mov")).unwrap();
    std::os::unix::fs::symlink(t.root.join("Real"), t.root.join("L/folderlink")).unwrap();
    std::os::unix::fs::symlink(t.root.join("Src/b.mov"), t.root.join("L/link.txt")).unwrap();
    let c = collect(&[t.path("L")], true, SortKey::Name, true);
    assert_eq!(t.rel(&c.files), ["L/link.mov"]);
    assert_eq!(c.skipped_non_media, 1);
    assert_eq!(c.subfolders_not_included, 0);
}
