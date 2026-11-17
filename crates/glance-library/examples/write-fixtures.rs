//! Writes the Rust side of the library cross-test into a directory, for the Swift oracle to open
//! (`tools/swift-library-oracle/run.sh verify …`):
//!
//! ```sh
//! cargo run -p glance-library --example write-fixtures -- <dir>
//! cargo run -p glance-library --example write-fixtures -- --check-swift-writes <report-dir>
//! ```
//!
//! The first form writes `empty/`, `populated/` and `legacy-v7-upgraded/` libraries made by the Rust store, the
//! sidecar and stash byte cases under `json/`, and a `sidecar-tree/` written through the Rust sidecar writer with
//! the Rust scan of it (`sidecar-tree-scan.json`). The second re-opens the library Swift changed
//! (`<report-dir>/after-swift-writes`) and checks Swift's writes from the Rust side. Synthetic data only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../tests/common/mod.rs"]
mod common;

use common::*;
use glance_library::filter::ClipSort;
use glance_library::sidecar::{ClipMetadata, scan_sidecars, write_sidecar};
use glance_library::stash::write_stash_file;
use glance_library::store::Location;
use serde_json::json;
use std::path::Path;

fn write_libraries(out: &Path) {
    let _ = std::fs::remove_dir_all(out);
    std::fs::create_dir_all(out).unwrap();
    drop(open(Location::File(out.join("empty/library.sqlite"))));
    build(&mut open(Location::File(out.join("populated/library.sqlite"))));
    let legacy = out.join("legacy-v7-upgraded/library.sqlite");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::copy(fixtures().join("legacy-v7/library.sqlite"), &legacy).unwrap();
    let store = open(Location::File(legacy));
    assert!(store.open_report().migrations.needs_full_reindex);

    let json_dir = out.join("json");
    std::fs::create_dir_all(&json_dir).unwrap();
    for (name, meta) in sidecar_cases() {
        std::fs::write(json_dir.join(format!("sidecar-{name}.json")), meta.encode().unwrap()).unwrap();
    }
    for (name, stashes) in [("stash-full", stash_cases()), ("stash-empty", vec![])] {
        std::fs::create_dir_all(json_dir.join(name)).unwrap();
        write_stash_file(&stashes, &json_dir.join(name)).unwrap();
    }

    // A small folder tree: sidecars beside (absent) clips, in a hidden folder, inside a package (not scanned), behind
    // a symbolic link (not followed), a non-ASCII name, and one undecodable sidecar.
    let tree = out.join("sidecar-tree");
    for dir in ["sub", ".hiddendir", "Pack.app", "sub/deeper", "sub/other", "after", "more/a", "more/b"] {
        std::fs::create_dir_all(tree.join(dir)).unwrap();
    }
    for (i, clip) in [
        "clip.mov",
        "sub/x.mov",
        ".hiddendir/y.mov",
        "Pack.app/z.mov",
        "sub/caf\u{E9} n\u{FC}.mov",
        "sub/deeper/w.mov",
        "sub/other/v.mov",
        "after/u.mov",
        "more/a/t.mov",
        "more/b/s.mov",
        "more/r.mov",
    ]
    .iter()
    .enumerate()
    {
        let mut meta = ClipMetadata::new(id('a', 100 + i as i64), 1_731_600_000.5);
        meta.tags = strs(&["tree"]);
        write_sidecar(&meta, &tree.join(clip), None).unwrap();
    }
    std::fs::write(tree.join("sub/.broken.mov.crate-meta.json"), b"{not json").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(tree.join("sub"), tree.join("link")).unwrap();
        std::os::unix::fs::symlink(tree.join(".clip.mov.crate-meta.json"), tree.join("sub/.linked.mov.crate-meta.json")).unwrap();
        std::os::unix::fs::symlink(tree.join("more/a"), tree.join("more/lnk")).unwrap();
    }
    let scan: Vec<serde_json::Value> =
        scan_sidecars(&tree).into_iter().map(|(p, m)| json!([p.strip_prefix(&tree).unwrap().to_string_lossy(), m.clip_id])).collect();
    std::fs::write(out.join("sidecar-tree-scan.json"), serde_json::to_vec_pretty(&scan).unwrap()).unwrap();
    println!("wrote {}", out.display());
}

/// Re-open the populated library Swift changed and check its writes from the Rust side.
fn check_swift_writes(report: &Path) {
    let lib = report.join("after-swift-writes/library.sqlite");
    let store = open(Location::File(lib.clone()));
    assert!(store.open_report().migrations.applied.is_empty(), "no migration re-ran");
    assert_eq!(store.open_report().search_backfilled, None);
    let new_path = format!("{ROOT}/from swift.mov");
    let row = store.fetch_at(&new_path).unwrap().expect("Swift's new row");
    assert_eq!((row.rating, row.mood.as_deref(), row.created_at, row.updated_at), (4, Some("swift"), 1_800_000_000.0, 1_800_000_000.5));
    assert_eq!(store.tags_for_path(&new_path).unwrap(), strs(&["blue", "swifttag"]));
    let renamed = format!("{ROOT}/blue_intro renamed by swift.mov");
    assert_eq!(store.tags_for_path(&renamed).unwrap(), strs(&["blue", "intro", "loop"]), "ON UPDATE CASCADE moved the tags");
    assert!(store.tags_for_path(&Fx::p_a()).unwrap().is_empty());
    assert_eq!(store.search_paths("renamed swift").unwrap(), vec![renamed.clone()]);
    assert!(store.search_paths("swifttag").unwrap().contains(&new_path));
    assert!(!store.has_any_row(&format!("{ROOT}/edge")).unwrap(), "deleteRows(underPathPrefix:) removed the edge folder");
    assert!(store.search_paths("hidden").unwrap().is_empty(), "and its FTS rows");
    let preset = store.list_presets().unwrap().into_iter().find(|p| p.name == "From Swift").expect("Swift's preset");
    assert_eq!(preset.sort, Some(ClipSort::FilenameZA));
    assert_eq!(preset.filter.tags, tags(&["swifttag"]));
    let checks = glance_library::dump::checks(store.connection()).unwrap();
    assert_eq!(checks["integrity_check"], json!(["ok"]));
    assert_eq!(checks["foreign_key_check"], json!(0));
    assert_eq!(checks["clips"], checks["clip_search"]);
    store.connection().execute_batch("INSERT INTO clip_search(clip_search) VALUES('integrity-check')").unwrap();
    println!("Swift's writes read back from Rust: ok ({} clips)", checks["clips"]);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, dir] if flag == "--check-swift-writes" => check_swift_writes(Path::new(dir)),
        [dir] => write_libraries(Path::new(dir)),
        _ => {
            eprintln!("usage: write-fixtures <dir> | write-fixtures --check-swift-writes <report-dir>");
            std::process::exit(2);
        }
    }
}
