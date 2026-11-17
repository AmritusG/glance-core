// SPDX-License-Identifier: MIT
//! The kit's pure logic, with SendEhrKit's own test inputs and expected values: file URLs, path
//! remaps, the planner, the menu model, recents, sorting, media types, the composition decoder,
//! settings decoding, the reconnect policy.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_arena::decoder;
use glance_arena::*;
use std::time::Instant;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

// MARK: file URLs (Arena's swagger.yaml, FileUrl)

#[test]
fn file_url_spaces_from_spec_example() {
    assert_eq!(ArenaFileURL::encode("/Users/Resolume/My Videos/file 1.mov"), "file:///Users/Resolume/My%20Videos/file%201.mov");
}

#[test]
fn file_url_special_characters() {
    assert_eq!(ArenaFileURL::encode("/a/#1 & 50%?.mov"), "file:///a/%231%20%26%2050%25%3F.mov");
}

#[test]
fn file_url_non_ascii_is_utf8_and_nfc() {
    let nfd = "/Users/x/Mo\u{0308}tley Cru\u{0308}e/Bjo\u{0308}rk.mov"; // Finder-style decomposed
    let nfc = "/Users/x/M\u{00F6}tley Cr\u{00FC}e/Bj\u{00F6}rk.mov";
    assert_eq!(ArenaFileURL::encode(nfd), ArenaFileURL::encode(nfc));
    assert_eq!(ArenaFileURL::encode(nfc), "file:///Users/x/M%C3%B6tley%20Cr%C3%BCe/Bj%C3%B6rk.mov");
}

#[test]
fn file_url_round_trip() {
    use unicode_normalization::UnicodeNormalization;
    let p = "/Volumes/VJ Clips/Resolume Clips/日本 ñ (1).mov";
    let want: String = p.nfc().collect();
    assert_eq!(ArenaFileURL::decode(&ArenaFileURL::encode(p)), Some(want));
    // Relative paths get the leading slash; bad escapes do not decode.
    assert_eq!(ArenaFileURL::encode("a b.mov"), "file:///a%20b.mov");
    assert_eq!(ArenaFileURL::decode("file:///a%2"), None);
    assert_eq!(ArenaFileURL::decode("http://x"), None);
}

// MARK: path remaps

#[test]
fn remap_prefix() {
    let r = vec![PathRemap::new("/Volumes/X", "/Volumes/Y")];
    assert_eq!(remap(&r, "/Volumes/X/clips/a.mov"), "/Volumes/Y/clips/a.mov");
}

#[test]
fn remap_whole_component_only() {
    let r = vec![PathRemap::new("/Volumes/X", "/Volumes/Y")];
    assert_eq!(remap(&r, "/Volumes/XY/a.mov"), "/Volumes/XY/a.mov");
}

#[test]
fn remap_trailing_slash_and_longest_wins() {
    let r = vec![PathRemap::new("/Volumes/X/", "/Volumes/Y/"), PathRemap::new("/Volumes/X/special", "/Z")];
    assert_eq!(remap(&r, "/Volumes/X/special/a.mov"), "/Z/a.mov");
    assert_eq!(remap(&r, "/Volumes/X/other/a.mov"), "/Volumes/Y/other/a.mov");
}

#[test]
fn remap_unmatched_passes_through() {
    assert_eq!(remap(&[], "/a/b.mov"), "/a/b.mov");
}

#[test]
fn remap_matches_nfd_path_against_nfc_prefix() {
    let r = vec![PathRemap::new("/Volumes/M\u{00F6}", "/R")];
    assert_eq!(remap(&r, "/Volumes/Mo\u{0308}/a.mov"), "/R/a.mov");
}

// MARK: planner (2 layers x 6 columns; layer 1, id 10, occupied at columns 2 and 4)

fn planner_snap() -> CompositionSnapshot {
    let slots = |occupied: &[(usize, &str)]| -> Vec<SlotInfo> {
        (1..=6)
            .map(|c| {
                let name = occupied.iter().find(|(k, _)| *k == c).map(|(_, n)| n.to_string());
                let state = if name.is_none() { "Empty" } else { "Disconnected" };
                SlotInfo::new(c, c as i64, name, state)
            })
            .collect()
    };
    CompositionSnapshot::new(
        "Test",
        "Main",
        vec![LayerInfo::new(1, 10, "Layer #", None, slots(&[(2, "B"), (4, "D")])), LayerInfo::new(2, 20, "Lights", None, slots(&[]))],
        (1..=6).map(|c| ColumnInfo::new(c, 100 + c as i64, "Column #", None)).collect(),
    )
}

fn cols(p: &LoadPlan) -> Vec<usize> {
    p.assignments.iter().map(|a| a.column).collect()
}

#[test]
fn plan_consecutive_fill() {
    let p = LoadPlanner::plan(&s(&["a.mov", "b.mov", "c.mov"]), &Destination::Slot { layer_id: 20, column: 3 }, &planner_snap()).unwrap();
    assert_eq!(cols(&p), [3, 4, 5]);
    assert_eq!(p.layer_index, 2);
    assert_eq!(p.add_columns, 0);
    assert!(p.conflicts().is_empty());
}

#[test]
fn plan_consecutive_fill_past_end_adds_columns() {
    let p = LoadPlanner::plan(&s(&["a.mov", "b.mov", "c.mov"]), &Destination::Slot { layer_id: 20, column: 5 }, &planner_snap()).unwrap();
    assert_eq!(cols(&p), [5, 6, 7]);
    assert_eq!(p.add_columns, 1);
}

#[test]
fn plan_occupied_conflicts_and_policies() {
    let p = LoadPlanner::plan(&s(&["a.mov", "b.mov", "c.mov"]), &Destination::Slot { layer_id: 10, column: 2 }, &planner_snap()).unwrap();
    let conflicts = p.conflicts();
    assert_eq!(conflicts.iter().map(|a| a.column).collect::<Vec<_>>(), [2, 4]);
    assert_eq!(conflicts.iter().map(|a| a.replacing.clone()).collect::<Vec<_>>(), [Some("B".to_string()), Some("D".to_string())]);
    assert_eq!(p.resolving(ConflictResolution::Replace).unwrap().assignments.len(), 3);
    assert_eq!(cols(&p.resolving(ConflictResolution::Skip).unwrap()), [3]);
    assert!(p.resolving(ConflictResolution::Cancel).is_none());
}

#[test]
fn plan_next_free_skips_occupied_and_appends() {
    let files: Vec<String> = (1..=5).map(|i| format!("f{i}.mov")).collect();
    let p = LoadPlanner::plan(&files, &Destination::NextFree { layer_id: 10 }, &planner_snap()).unwrap();
    assert_eq!(cols(&p), [1, 3, 5, 6, 7]);
    assert_eq!(p.add_columns, 1);
    assert!(p.conflicts().is_empty());
}

#[test]
fn plan_append_columns() {
    let p = LoadPlanner::plan(&s(&["a.mov", "b.mov"]), &Destination::AppendColumns { layer_id: 10 }, &planner_snap()).unwrap();
    assert_eq!(cols(&p), [7, 8]);
    assert_eq!(p.add_columns, 2);
}

#[test]
fn plan_new_layer() {
    let p = LoadPlanner::plan(&s(&["a.mov"]), &Destination::NewLayer, &planner_snap()).unwrap();
    assert!(p.add_layer);
    assert_eq!(p.layer_index, 3);
    assert_eq!(cols(&p), [1]);
}

#[test]
fn plan_layer_gone() {
    let e = LoadPlanner::plan(&s(&["a.mov"]), &Destination::NextFree { layer_id: 999 }, &planner_snap()).unwrap_err();
    assert_eq!(e, ArenaError::LayerGone { layer_id: 999 });
}

fn titles(nodes: &[MenuNode]) -> Vec<String> {
    nodes.iter().filter_map(|n| n.title().map(str::to_string)).collect()
}

fn submenu<'a>(nodes: &'a [MenuNode], title: &str) -> &'a [MenuNode] {
    match nodes.iter().find(|n| n.title() == Some(title)) {
        Some(MenuNode::Submenu { children, .. }) => children,
        other => panic!("no submenu {title}: {other:?}"),
    }
}

fn connected() -> ArenaStatus {
    ArenaStatus::Connected { product: "Arena".to_string() }
}

#[test]
fn menu_model() {
    let mut input = SendMenuInput::new(Some(planner_snap()), connected(), 1);
    input.settings.extra_empty_columns = 1;
    let nodes = SendMenuModel::build(&input);
    let t = titles(&nodes);
    assert_eq!(t[0], "New Layer at Top (Layer 3)");
    assert!(t.contains(&"Layer 2 \u{201C}Lights\u{201D}".to_string()));
    // highest layer first, like Arena
    let i2 = t.iter().position(|x| x == "Layer 2 \u{201C}Lights\u{201D}").unwrap();
    let i1 = t.iter().position(|x| x == "Layer 1").unwrap();
    assert!(i2 < i1);
    let ct = titles(submenu(&nodes, "Layer 1"));
    assert_eq!(ct[0], "Next Free Column (1)");
    assert_eq!(ct[1], "Append as New Column (7)");
    assert!(ct.contains(&"Column 2 \u{00B7} \u{25CF} B".to_string()));
    assert!(ct.contains(&"Column 5 \u{00B7} (empty)".to_string())); // last occupied 4 + 1 extra
    assert!(!ct.contains(&"Column 6 \u{00B7} (empty)".to_string()));
    assert!(ct.contains(&"Columns 6\u{2013}6".to_string()));
}

#[test]
fn menu_unreachable_is_one_line() {
    let input = SendMenuInput::new(None, ArenaStatus::Failed(ArenaError::WebserverOff { host: "localhost".to_string(), port: 8080 }), 1);
    let nodes = SendMenuModel::build(&input);
    assert_eq!(nodes.len(), 1);
    let MenuNode::Info(text) = &nodes[0] else { panic!() };
    assert!(text.contains("webserver is off"));
    assert_eq!(text, "Arena's webserver is off - Arena > Preferences > Webserver: enable, port 8080 (localhost).");
}

#[test]
fn menu_replace_item_mode() {
    let mut input = SendMenuInput::new(Some(planner_snap()), connected(), 1);
    input.settings.occupied_slot_mode = OccupiedSlotMode::ReplaceItem;
    let nodes = SendMenuModel::build(&input);
    let layer = submenu(&nodes, "Layer 1");
    let sub = submenu(layer, "Column 2 \u{00B7} \u{25CF} B");
    let MenuNode::Action { title, action: MenuAction::Load { replace, .. }, .. } = &sub[0] else { panic!() };
    assert_eq!(title, "Replace \u{201C}B\u{201D}");
    assert!(replace);
}

#[test]
fn menu_status_lines() {
    let one = |st: ArenaStatus| SendMenuModel::build(&SendMenuInput::new(None, st, 1));
    assert_eq!(one(ArenaStatus::Unknown), vec![MenuNode::Info("Connecting to Arena...".to_string())]);
    assert_eq!(SendMenuModel::build(&SendMenuInput::new(None, connected(), 0)), vec![MenuNode::Info("No files Arena can open in the selection.".to_string())]);
    let mut dead = SendMenuInput::new(None, connected(), 1);
    dead.host_alive = false;
    assert_eq!(titles(&SendMenuModel::build(&dead)), ["SendEhr is not running", "Open SendEhr"]);
}

#[test]
fn recents_keep_five_unique() {
    let mut list: Vec<RecentTarget> = Vec::new();
    for c in 1..=7 {
        list = RecentTarget::pushing(RecentTarget::new(Destination::Slot { layer_id: 1, column: c }, c.to_string()), &list);
    }
    list = RecentTarget::pushing(RecentTarget::new(Destination::Slot { layer_id: 1, column: 5 }, "5"), &list);
    assert_eq!(list.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["5", "7", "6", "4", "3"]);
}

#[test]
fn recents_in_menu() {
    let snap = planner_snap();
    let recents = vec![
        RecentTarget::new(Destination::Slot { layer_id: 10, column: 2 }, "L1 C2"),
        RecentTarget::new(Destination::NextFree { layer_id: 20 }, "L2 next"),
        RecentTarget::new(Destination::AppendColumns { layer_id: 10 }, "L1 append"),
        RecentTarget::new(Destination::Slot { layer_id: 999, column: 1 }, "gone"),
        RecentTarget::new(Destination::Slot { layer_id: 10, column: 99 }, "past the end"),
    ];
    let mut input = SendMenuInput::new(Some(snap), connected(), 1);
    input.recents = recents;
    let t = titles(&SendMenuModel::build(&input));
    assert_eq!(
        &t[..5],
        [
            "Recent",
            "Layer 1 \u{203A} Column 2 \u{00B7} \u{25CF} B",
            "Layer 2 \u{201C}Lights\u{201D} \u{203A} Next Free Column",
            "Layer 1 \u{203A} Append as New Column",
            "New Layer at Top (Layer 3)",
        ]
    );
}

#[test]
fn natural_name_order_is_default() {
    let items: Vec<MediaItem> = ["/x/D four.mov", "/y/clip 10.mov", "/x/B two.mov", "/x/clip 2.mov"].iter().map(|p| MediaItem::new(*p)).collect();
    let d = SendEhrSettings::default();
    let sorted: Vec<String> = MediaSorter::sort(&items, d.sort_key, d.sort_ascending).into_iter().map(|i| i.path).collect();
    assert_eq!(sorted, ["/x/B two.mov", "/x/clip 2.mov", "/y/clip 10.mov", "/x/D four.mov"]);
}

#[test]
fn media_types() {
    assert!(ArenaMediaTypes::is_supported_file("/a/b.MOV", false));
    assert!(ArenaMediaTypes::is_supported_file("/a/b.png", false));
    assert!(!ArenaMediaTypes::is_supported_file("/a/b.avc", false));
    assert!(!ArenaMediaTypes::is_supported_file("/a/b.txt", false));
    assert!(!ArenaMediaTypes::is_supported_file("/a/b.mov/", false));
}

// MARK: sort keys (a/b/c differ in every key, each key ordering them differently)

fn sort_items() -> Vec<MediaItem> {
    use std::time::{Duration, UNIX_EPOCH};
    let t = |s: u64| Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000 + s));
    vec![
        MediaItem { path: "/m/clip 10.png".into(), modified: t(30), created: t(10), added: t(20), size: Some(300), ..Default::default() },
        MediaItem { path: "/m/clip 2.wav".into(), modified: t(10), created: t(30), added: t(30), size: Some(100), ..Default::default() },
        MediaItem { path: "/m/clip 1.mov".into(), modified: t(20), created: t(20), added: t(10), size: Some(200), ..Default::default() },
    ]
}

#[test]
fn every_sort_key_both_directions() {
    let expected: [(SortKey, [&str; 3]); 6] = [
        (SortKey::Name, ["clip 1.mov", "clip 2.wav", "clip 10.png"]),
        (SortKey::DateModified, ["clip 2.wav", "clip 1.mov", "clip 10.png"]),
        (SortKey::DateCreated, ["clip 10.png", "clip 1.mov", "clip 2.wav"]),
        (SortKey::DateAdded, ["clip 1.mov", "clip 10.png", "clip 2.wav"]),
        (SortKey::Size, ["clip 2.wav", "clip 1.mov", "clip 10.png"]),
        (SortKey::Kind, ["clip 1.mov", "clip 10.png", "clip 2.wav"]), // mov < png < wav
    ];
    for (key, asc) in expected {
        let names = |a: bool| MediaSorter::sort(&sort_items(), key, a).iter().map(MediaItem::name).collect::<Vec<_>>();
        assert_eq!(names(true), asc, "{key:?} ascending");
        let mut desc = asc.to_vec();
        desc.reverse();
        assert_eq!(names(false), desc, "{key:?} descending");
    }
}

#[test]
fn ties_and_missing_values_fall_back_to_natural_name() {
    let tie = vec![
        MediaItem { path: "/a/x 10.mov".into(), size: Some(5), ..Default::default() },
        MediaItem { path: "/a/x 9.mov".into(), size: Some(5), ..Default::default() },
        MediaItem::new("/a/none.mov"),
    ];
    let names = |a: bool| MediaSorter::sort(&tie, SortKey::Size, a).iter().map(MediaItem::name).collect::<Vec<_>>();
    assert_eq!(names(true), ["none.mov", "x 9.mov", "x 10.mov"]);
    assert_eq!(names(false), ["x 9.mov", "x 10.mov", "none.mov"]);
}

/// Swift's `localizedStandardCompare` on these pairs, measured with Swift 6.4 on macOS 27.
#[test]
fn natural_compare_matches_finder() {
    use glance_arena::media::natural_compare;
    use std::cmp::Ordering::{self, *};
    let table: &[(&str, &str, Ordering)] = &[
        ("clip 2", "clip 10", Less),
        ("Clip 2", "clip 2", Greater),
        ("clip", "Clip", Less),
        ("\u{e9}t\u{e9}", "f", Less),
        ("a 1", "a1", Less),
        ("007", "7", Greater),
        ("b", "B two", Less),
        ("x 9.mov", "x 10.mov", Less),
        ("none.mov", "x 9.mov", Less),
        ("A loose.mov", "Set", Less),
        ("a_b", "a-b", Less),
        ("a-b", "ab", Less),
        ("a.b", "a b", Greater),
        ("\u{e9}", "e", Greater),
        ("e", "f", Less),
        ("\u{c9}", "\u{e9}", Greater),
        ("\u{e4}", "b", Less),
        ("Z", "a", Greater),
        ("1", "a", Less),
        ("_", "a", Less),
        ("_", "1", Less),
        ("(1)", "1", Less),
        ("a10b", "a9c", Greater),
        ("file01", "file1", Greater),
        ("file1", "file01", Less),
        ("clip 2.wav", "clip 10.png", Less),
        ("mov", "png", Less),
        ("", "a", Less),
        ("a", "ab", Less),
        ("\u{df}", "ss", Greater),
        ("\u{f8}", "o", Greater),
        ("\u{65e5}\u{672c}", "a", Greater),
        ("Ab", "aB", Greater),
        ("a1b", "a01b", Less),
        ("1.5", "1.10", Less),
        ("x", "x 1", Less),
        ("v2.0", "v10.0", Less),
        ("\u{1f600}", "a", Less),
        ("a\u{1f600}", "a", Greater),
        ("A", "a", Greater),
        ("aa", "aA", Less),
        ("2 b", "10 a", Less),
        ("02", "2", Greater),
        ("0", "00", Less),
        ("\u{c9}COLE", "ecole", Greater),
        ("Bjo\u{308}rk", "Bj\u{f6}rk", Less),
    ];
    let mut wrong = Vec::new();
    for (a, b, want) in table {
        if natural_compare(a, b) != *want {
            wrong.push(format!("{a:?} vs {b:?}: want {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

// MARK: menu + planner with folders (1 layer x 4 columns)

fn folder_snap() -> CompositionSnapshot {
    CompositionSnapshot::new(
        "C",
        "D",
        vec![LayerInfo::new(1, 11, "Layer #", None, (1..=4).map(|c| SlotInfo::new(c, c as i64, None, "Empty")).collect())],
        (1..=4).map(|c| ColumnInfo::new(c, 100 + c as i64, "Column #", None)).collect(),
    )
}

fn folder_input(folders: &[&str], files: usize) -> SendMenuInput {
    let mut i = SendMenuInput::new(Some(folder_snap()), connected(), files);
    i.folder_names = s(folders);
    i
}

#[test]
fn folder_as_new_layer_naming() {
    let nodes = SendMenuModel::build(&folder_input(&["Warm Up Set"], 0));
    let item = nodes.iter().find(|n| n.title().is_some_and(|t| t.starts_with("Folder as New Layer"))).unwrap();
    let MenuNode::Action { title, action, label } = item else { panic!() };
    assert_eq!(title, "Folder as New Layer \u{201C}Warm Up Set\u{201D} - Layer 2");
    assert_eq!(action, &MenuAction::Load { destination: Destination::NewNamedLayer { name: "Warm Up Set".into() }, replace: false });
    assert_eq!(label, "Folder as New Layer \u{201C}Warm Up Set\u{201D}");
    assert_eq!(Destination::NewNamedLayer { name: "Warm Up Set".into() }.new_layer_name(), Some("Warm Up Set"));
    let plan = LoadPlanner::plan(&s(&["a.mov", "b.mov"]), &Destination::NewNamedLayer { name: "Warm Up Set".into() }, &folder_snap()).unwrap();
    assert!(plan.add_layer);
    assert_eq!(plan.layer_index, 2);
    assert_eq!(cols(&plan), [1, 2]);
    // Two folders: no single-folder item; plain New Layer stays unnamed.
    let two = SendMenuModel::build(&folder_input(&["A", "B"], 0));
    assert!(!titles(&two).iter().any(|t| t.starts_with("Folder as New Layer")));
    assert!(titles(&two).contains(&"New Layer at Top (Layer 2)".to_string()));
    // New named layers are not remembered as Recent targets.
    assert!(RecentTarget::pushing(RecentTarget::new(Destination::NewNamedLayer { name: "X".into() }, "X"), &[]).is_empty());
}

#[test]
fn folder_menu_hides_unknown_ranges_and_shows_settings() {
    let mut input = folder_input(&["Set"], 1);
    input.settings.sort_key = SortKey::DateModified;
    input.settings.sort_ascending = false;
    input.settings.trigger_after_load = true;
    let nodes = SendMenuModel::build(&input);
    assert_eq!(nodes[0].title(), Some("Folder \u{201C}Set\u{201D} + 1 file -> consecutive columns, sorted by Date Modified \u{2193}"));
    let layer = submenu(&nodes, "Layer 1");
    assert_eq!(titles(&layer[..2]), ["Next Free Columns", "Append as New Columns"]);
    let (sort_title, sort) = nodes
        .iter()
        .find_map(|n| match n {
            MenuNode::Submenu { title, children } if title.starts_with("Sort By") => Some((title.clone(), children.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(sort_title, "Sort By (Date Modified \u{2193})");
    let checked: Vec<&str> = sort
        .iter()
        .filter_map(|n| match n {
            MenuNode::Toggle { title, is_on: true, .. } => Some(title.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(checked, ["Date Modified", "Descending"]);
    assert!(nodes.contains(&MenuNode::Toggle {
        title: "Include Subfolders".into(),
        is_on: false,
        action: MenuAction::Setting(SettingChange::IncludeSubfolders(true))
    }));
    assert!(nodes.contains(&MenuNode::Toggle {
        title: "Trigger After Load".into(),
        is_on: true,
        action: MenuAction::Setting(SettingChange::TriggerAfterLoad(false))
    }));
    // Files only: no Include Subfolders, ranges shown.
    let files = SendMenuModel::build(&SendMenuInput::new(Some(folder_snap()), connected(), 2));
    assert!(!titles(&files).contains(&"Include Subfolders".to_string()));
    assert_eq!(submenu(&files, "Layer 1")[0].title(), Some("Next Free Columns (1\u{2013}2)"));
    assert_eq!(files[0].title(), Some("2 files -> consecutive columns from your pick, sorted by Name \u{2191}"));
}

#[test]
fn setting_changes_and_old_settings_file() {
    let mut st = SendEhrSettings::default();
    assert_eq!(st.sort_key, SortKey::Name);
    assert!(st.sort_ascending);
    assert!(!st.include_subfolders);
    assert_eq!(st.large_folder_threshold, 50);
    assert!(!st.trigger_after_load);
    st = SettingChange::SortKey(SortKey::Size).applied(&st);
    st = SettingChange::SortAscending(false).applied(&st);
    st = SettingChange::IncludeSubfolders(true).applied(&st);
    assert_eq!(st.sort_description(), "Size \u{2193}");
    assert!(st.include_subfolders);
    // An early settings file (no new keys) keeps its values.
    let old = r#"{"endpoint":{"host":"10.0.0.5","port":8081},"remaps":[],"occupiedSlotMode":"replaceItem","triggerAfterLoad":true,"extraEmptyColumns":3}"#;
    let d: SendEhrSettings = serde_json::from_str(old).unwrap();
    assert_eq!(d.endpoint.port, 8081);
    assert_eq!(d.occupied_slot_mode, OccupiedSlotMode::ReplaceItem);
    assert!(d.trigger_after_load);
    assert_eq!(d.sort_key, SortKey::Name);
    assert_eq!(d.large_folder_threshold, 50);
    assert_eq!(d.extra_empty_columns, 3);
    // A setting request round-trips through the request file format.
    let req = LoadRequest::setting_change(SettingChange::SortKey(SortKey::DateAdded));
    let back: LoadRequest = serde_json::from_slice(&glance_arena::swift_json::to_vec(&req).unwrap()).unwrap();
    assert_eq!(back.setting, Some(SettingChange::SortKey(SortKey::DateAdded)));
    assert!(back.paths.is_empty());
    assert_eq!(back.label, "Sort by Date Added");
}

#[test]
fn settings_decoding_edge_cases() {
    // Empty object: every default; interval clamped to >= 1; a wrong type fails the whole file.
    let d: SendEhrSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(d, SendEhrSettings::default());
    let d: SendEhrSettings = serde_json::from_str(r#"{"updateInterval":0}"#).unwrap();
    assert_eq!(d.update_interval, 1);
    assert!(serde_json::from_str::<SendEhrSettings>(r#"{"endpoint":{"host":"x"}}"#).is_err());
    assert!(serde_json::from_str::<SendEhrSettings>(r#"{"sortKey":"color"}"#).is_err());
    // Unknown keys are ignored.
    let d: SendEhrSettings = serde_json::from_str(r#"{"future":1,"sortAscending":false}"#).unwrap();
    assert!(!d.sort_ascending);
}

// MARK: Folders as New Layers

fn layers_menu_item(folders: &[&str], files: usize) -> Option<MenuNode> {
    SendMenuModel::build(&folder_input(folders, files)).into_iter().find(|n| n.title().is_some_and(|t| t.starts_with("Folder") && t.contains("New Layer")))
}

#[test]
fn folders_as_new_layers_menu_item() {
    let Some(MenuNode::Action { title, action, .. }) = layers_menu_item(&["One"], 0) else { panic!() };
    assert_eq!(title, "Folder as New Layer \u{201C}One\u{201D} - Layer 2");
    assert_eq!(action, MenuAction::Load { destination: Destination::NewNamedLayer { name: "One".into() }, replace: false });
    for n in [2usize, 3] {
        let names: Vec<String> = (1..=n).map(|i| format!("F{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let Some(MenuNode::Action { title, action, label }) = layers_menu_item(&refs, 0) else { panic!() };
        assert_eq!(title, format!("Folders as New Layers ({n}) - Layers 2\u{2013}{}", 1 + n));
        assert_eq!(label, format!("Folders as New Layers ({n})"));
        assert_eq!(action, MenuAction::Load { destination: Destination::LayerPerFolder, replace: false });
    }
    // Loose files alongside: the item stays available.
    assert!(matches!(layers_menu_item(&["A", "B"], 1), Some(MenuNode::Action { .. })));
    assert_eq!(Destination::LayerPerFolder.layer_id(), None);
    assert!(RecentTarget::pushing(RecentTarget::new(Destination::LayerPerFolder, "x"), &[]).is_empty());
    assert!(LoadPlanner::plan(&s(&["a.mov"]), &Destination::LayerPerFolder, &folder_snap()).is_err());
}

#[test]
fn plan_layers_one_two_three_folders() {
    let one = LoadPlanner::plan_layers(&[FolderGroup::new("/x/A", s(&["/x/A/1.mov", "/x/A/2.mov"]))], &folder_snap()).unwrap();
    assert_eq!(one.layers.iter().map(|l| l.layer_index).collect::<Vec<_>>(), [2]);
    assert_eq!(one.layers[0].name, "A");
    assert_eq!(one.layers[0].assignments.iter().map(|a| a.column).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(one.add_columns, 0);

    let two =
        LoadPlanner::plan_layers(&[FolderGroup::new("/x/A", s(&["a1.mov"])), FolderGroup::new("/x/B", s(&["b1.mov", "b2.mov"]))], &folder_snap()).unwrap();
    assert_eq!(two.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    assert_eq!(two.layers.iter().map(|l| l.layer_index).collect::<Vec<_>>(), [3, 2]); // first folder top-most
    assert_eq!(two.base_layer_count, 1);

    let wide: Vec<String> = (1..=6).map(|i| format!("c{i}.mov")).collect();
    let three = LoadPlanner::plan_layers(
        &[FolderGroup::new("/x/A", s(&["a.mov"])), FolderGroup::new("/x/B", wide.clone()), FolderGroup::new("/x/C", s(&["c.mov", "d.mov"]))],
        &folder_snap(),
    )
    .unwrap();
    assert_eq!(three.layers.iter().map(|l| l.layer_index).collect::<Vec<_>>(), [4, 3, 2]);
    assert_eq!(
        three.layers.iter().map(|l| l.assignments.iter().map(|a| a.column).collect::<Vec<_>>()).collect::<Vec<_>>(),
        vec![vec![1], vec![1, 2, 3, 4, 5, 6], vec![1, 2]]
    );
    assert_eq!(three.layers[1].assignments.iter().map(|a| a.path.clone()).collect::<Vec<_>>(), wide);
    assert_eq!(three.add_columns, 2); // widest folder 6 > 4 columns
    assert!(three.layers.iter().all(|l| l.assignments.iter().all(|a| a.replacing.is_none())));
}

#[test]
fn empty_folder_makes_no_layer() {
    let p = LoadPlanner::plan_layers(
        &[FolderGroup::new("/x/A", s(&["a.mov"])), FolderGroup::new("/x/Empty", vec![]), FolderGroup::new("/x/C", s(&["c.mov"]))],
        &folder_snap(),
    )
    .unwrap();
    assert_eq!(p.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["A", "C"]);
    assert_eq!(p.layers.iter().map(|l| l.layer_index).collect::<Vec<_>>(), [3, 2]);
    assert_eq!(p.empty_folders, ["/x/Empty"]);
    assert_eq!(LoadPlanner::plan_layers(&[FolderGroup::new("/x/E", vec![])], &folder_snap()).unwrap_err(), ArenaError::NoFiles);
}

#[test]
fn layer_per_folder_request_round_trips() {
    let req = LoadRequest::new(s(&["/a", "/b"]), Destination::LayerPerFolder, false, "Folders as New Layers (2)");
    let back: LoadRequest = serde_json::from_slice(&glance_arena::swift_json::to_vec(&req).unwrap()).unwrap();
    assert_eq!(back.destination, Destination::LayerPerFolder);
    assert_eq!(back, req);
}

// MARK: trigger scope and layer ranges in the labels

fn snap_layers(n: usize) -> CompositionSnapshot {
    CompositionSnapshot::new(
        "C",
        "D",
        (1..=n).map(|i| LayerInfo::new(i, 10 + i as i64, "Layer #", None, vec![SlotInfo::new(1, 1, None, "Empty")])).collect(),
        vec![ColumnInfo::new(1, 101, "Column #", None)],
    )
}

#[test]
fn trigger_scope_setting_and_menu_pair() {
    let mut st = SendEhrSettings::default();
    assert_eq!(st.trigger_scope, TriggerScope::AllNewLayers);
    st = SettingChange::TriggerScope(TriggerScope::TopLayerOnly).applied(&st);
    assert_eq!(st.trigger_scope, TriggerScope::TopLayerOnly);
    assert_eq!(SettingChange::TriggerScope(TriggerScope::TopLayerOnly).log_line(), "Trigger Scope Top Layer Only");
    // Older settings files (no key) get the default; the value round-trips.
    let old: SendEhrSettings = serde_json::from_str(r#"{"triggerAfterLoad":true}"#).unwrap();
    assert_eq!(old.trigger_scope, TriggerScope::AllNewLayers);
    let back: SendEhrSettings = serde_json::from_slice(&glance_arena::swift_json::to_vec(&st).unwrap()).unwrap();
    assert_eq!(back.trigger_scope, TriggerScope::TopLayerOnly);

    let nodes = |folders: &[&str], files: usize| {
        let mut i = SendMenuInput::new(Some(snap_layers(1)), connected(), files);
        i.settings = st.clone();
        i.folder_names = s(folders);
        SendMenuModel::build(&i)
    };
    let three = nodes(&["A", "B", "C"], 0);
    let trig = three.iter().position(|n| n.title() == Some("Trigger After Load")).unwrap();
    assert_eq!(
        three[trig + 1..=trig + 2],
        [
            MenuNode::Toggle {
                title: "Trigger: All New Layers".into(),
                is_on: false,
                action: MenuAction::Setting(SettingChange::TriggerScope(TriggerScope::AllNewLayers))
            },
            MenuNode::Toggle {
                title: "Trigger: Top Layer Only".into(),
                is_on: true,
                action: MenuAction::Setting(SettingChange::TriggerScope(TriggerScope::TopLayerOnly))
            },
        ]
    );
    // One folder or plain files make at most one layer: no pair.
    for n in [nodes(&["A"], 0), nodes(&[], 2)] {
        assert!(!titles(&n).iter().any(|t| t.starts_with("Trigger:")));
    }
    // The menu bar always shows it.
    let bar = SendMenuModel::setting_nodes(&st, true, true, false);
    assert_eq!(titles(&bar).iter().filter(|t| t.starts_with("Trigger:")).count(), 2);
}

#[test]
fn label_range_one_and_n_folders() {
    let snap = snap_layers(23);
    let title = |folders: &[&str]| {
        let mut i = SendMenuInput::new(Some(snap.clone()), connected(), 0);
        i.folder_names = s(folders);
        SendMenuModel::build(&i).into_iter().find_map(|n| n.title().filter(|t| t.contains("as New Layer")).map(str::to_string))
    };
    assert_eq!(title(&["Set Alpha"]).unwrap(), "Folder as New Layer \u{201C}Set Alpha\u{201D} - Layer 24");
    assert_eq!(title(&["A", "B", "C"]).unwrap(), "Folders as New Layers (3) - Layers 24\u{2013}26");
    assert_eq!(SendMenuModel::expected_layers(&Destination::LayerPerFolder, Some(&snap), 3), Some(24..=26));
    assert_eq!(SendMenuModel::expected_layers(&Destination::NewLayer, Some(&snap), 0), Some(24..=24));
    assert_eq!(SendMenuModel::expected_layers(&Destination::NextFree { layer_id: 11 }, Some(&snap), 0), None);
    assert_eq!(SendMenuModel::expected_layers(&Destination::LayerPerFolder, None, 3), None);
}

#[test]
fn request_carries_expected_layers() {
    let mut req = LoadRequest::new(s(&["/a"]), Destination::LayerPerFolder, false, "x");
    req.expected_layers = Some(24..=26);
    let bytes = glance_arena::swift_json::to_vec(&req).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("\"expectedLayers\":[24,26]"));
    let back: LoadRequest = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(back.expected_layers, Some(24..=26));
    // An older request file (no key) still decodes.
    let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    v.as_object_mut().unwrap().remove("expectedLayers");
    let old: LoadRequest = serde_json::from_value(v).unwrap();
    assert_eq!(old.expected_layers, None);
}

// MARK: Arena Updates (Live / On Demand / Interval)

#[test]
fn stale_label_text() {
    let now = Timestamp::now();
    let ago = |s: f64| Some(Timestamp(now.0 - s));
    assert_eq!(SendMenuModel::updated_text(None, now), "Not updated yet");
    assert_eq!(SendMenuModel::updated_text(ago(0.4), now), "Updated just now");
    assert_eq!(SendMenuModel::updated_text(ago(12.0), now), "Updated 12 s ago");
    assert_eq!(SendMenuModel::updated_text(ago(119.0), now), "Updated 119 s ago");
    assert_eq!(SendMenuModel::updated_text(ago(185.0), now), "Updated 3 min ago");
    assert_eq!(SendMenuModel::updated_text(ago(7300.0), now), "Updated 2 h ago");

    let snap = CompositionSnapshot::new("C", "D", vec![LayerInfo::new(1, 1, "Layer #", Some(2), vec![])], vec![ColumnInfo::new(1, 9, "Column #", None)]);
    let menu = |mode: ArenaUpdateMode, st: ArenaStatus, snap: Option<CompositionSnapshot>| {
        let mut i = SendMenuInput::new(snap, st, 1);
        i.settings.update_mode = mode;
        i.updated_at = ago(12.0);
        i.now = now;
        SendMenuModel::build(&i)
    };
    let refresh = MenuNode::Action { title: "Refresh".into(), action: MenuAction::Refresh, label: "Refresh".into() };
    let live = menu(ArenaUpdateMode::Live, connected(), Some(snap.clone()));
    assert!(!live.contains(&refresh));
    assert!(!live.contains(&MenuNode::Info("Updated 12 s ago".into())), "nothing extra in Live");
    for mode in [ArenaUpdateMode::OnDemand, ArenaUpdateMode::Interval] {
        let m = menu(mode, connected(), Some(snap.clone()));
        assert_eq!(m[m.len() - 2..], [MenuNode::Info("Updated 12 s ago".into()), refresh.clone()]);
    }
    let nr = ArenaStatus::Failed(ArenaError::NotRunning);
    assert_eq!(
        menu(ArenaUpdateMode::OnDemand, nr.clone(), None),
        [MenuNode::Info(ArenaError::NotRunning.plain_line()), MenuNode::Info("Updated 12 s ago".into()), refresh]
    );
    assert_eq!(menu(ArenaUpdateMode::Live, nr, None), [MenuNode::Info(ArenaError::NotRunning.plain_line())]);
}

#[test]
fn updates_submenu_and_setting() {
    let mut st = SendEhrSettings::default();
    assert_eq!(st.update_mode, ArenaUpdateMode::Live);
    assert_eq!(st.update_interval, 60);
    let MenuNode::Submenu { title, children } = SendMenuModel::update_nodes(&st) else { panic!() };
    assert_eq!(title, "Arena Updates (Live)");
    assert_eq!(titles(&children[..3]), ["Live", "On Demand", "Interval (1 min)"]);
    assert_eq!(
        children[0],
        MenuNode::Toggle { title: "Live".into(), is_on: true, action: MenuAction::Setting(SettingChange::UpdateMode(ArenaUpdateMode::Live)) }
    );
    assert_eq!(titles(&children[4..]), ["Every 15 s", "Every 30 s", "Every 1 min", "Every 2 min", "Every 5 min"]);
    assert!(SendMenuModel::setting_nodes(&st, false, false, false).contains(&SendMenuModel::update_nodes(&st)));

    st = SettingChange::UpdateInterval(15).applied(&st);
    assert_eq!(st.update_mode, ArenaUpdateMode::Interval);
    assert_eq!(st.update_description(), "Every 15 s");
    st = SettingChange::UpdateMode(ArenaUpdateMode::OnDemand).applied(&st);
    assert_eq!(st.update_description(), "On Demand");
    assert_eq!(st.update_interval, 15, "the interval is kept for the next switch");

    // Old settings files (no keys) decode to Live / 60 s; the new keys round-trip.
    let old: SendEhrSettings = serde_json::from_str(r#"{"sortKey":"name"}"#).unwrap();
    assert_eq!(old.update_mode, ArenaUpdateMode::Live);
    assert_eq!(old.update_interval, 60);
    let back: SendEhrSettings = serde_json::from_slice(&glance_arena::swift_json::to_vec(&st).unwrap()).unwrap();
    assert_eq!(back, st);
    // The cache carries the fetch time; an old cache file has none.
    let c = CacheState { status: ArenaStatus::Unknown, snapshot: None, host_pid: 1, written_at: Timestamp(0.0), updated_at: Some(Timestamp(5.0)) };
    let back: CacheState = serde_json::from_slice(&glance_arena::swift_json::to_vec(&c).unwrap()).unwrap();
    assert_eq!(back.updated_at, Some(Timestamp(5.0)));
}

#[test]
fn menu_bar_inline_modes() {
    let st = SendEhrSettings { update_mode: ArenaUpdateMode::OnDemand, ..Default::default() };
    let nodes = SendMenuModel::setting_nodes(&st, true, true, true);
    assert!(!nodes.contains(&SendMenuModel::update_nodes(&st)), "no submenu in the menu bar form");
    let modes: Vec<(String, bool)> = nodes
        .iter()
        .filter_map(|n| match n {
            MenuNode::Toggle { title, is_on, .. } if title.starts_with("Arena Updates: ") => Some((title.clone(), *is_on)),
            _ => None,
        })
        .collect();
    assert_eq!(modes.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), ["Arena Updates: Live", "Arena Updates: On Demand", "Arena Updates: Interval (1 min)"]);
    assert_eq!(modes.iter().map(|m| m.1).collect::<Vec<_>>(), [false, true, false]);
    let Some(MenuNode::Submenu { title, children }) = nodes.last() else { panic!() };
    assert_eq!(title, "Interval (1 min)");
    assert_eq!(children.len(), 5);
}

#[test]
fn host_default_flag_rules() {
    // An interval pick counts as a pick; other settings do not.
    assert!(SettingChange::UpdateInterval(120).applied(&SendEhrSettings::default()).update_mode_chosen);
    assert!(!SettingChange::SortKey(SortKey::Size).applied(&SendEhrSettings::default()).update_mode_chosen);
    // Missing flag: false for Live, true for On Demand / Interval (only a pick could set them).
    let f = |j: &str| serde_json::from_str::<SendEhrSettings>(j).unwrap().update_mode_chosen;
    assert!(!f(r#"{"updateMode":"live"}"#));
    assert!(f(r#"{"updateMode":"onDemand"}"#));
    assert!(f(r#"{"updateMode":"interval","updateInterval":30}"#));
    assert!(!f(r#"{"updateMode":"interval","updateModeChosen":false}"#));
}

// MARK: menu layout de-duplication

fn w5_snap(states: &[&str], names: &[Option<&str>], layer_name: &str) -> CompositionSnapshot {
    let slots = states
        .iter()
        .enumerate()
        .map(|(i, st)| {
            let name = if i < names.len() {
                names[i].map(str::to_string)
            } else if *st == "Empty" {
                None
            } else {
                Some(format!("clip {i}"))
            };
            SlotInfo::new(i + 1, 100 + i as i64, name, *st)
        })
        .collect();
    CompositionSnapshot::new(
        "C",
        "D",
        vec![LayerInfo::new(1, 1, layer_name, Some(2), slots)],
        (1..=states.len()).map(|c| ColumnInfo::new(c, 900 + c as i64, "Column #", None)).collect(),
    )
}

#[test]
fn same_menu_ignores_play_state_only() {
    let a = w5_snap(&["Connected", "Disconnected", "Empty"], &[], "Layer #");
    let play_moved = w5_snap(&["Disconnected", "Connected", "Empty"], &[], "Layer #");
    assert_ne!(a, play_moved);
    assert!(CompositionSnapshot::same_menu(Some(&a), Some(&play_moved)));
    assert!(!CompositionSnapshot::same_menu(Some(&a), Some(&w5_snap(&["Connected", "Disconnected", "Disconnected"], &[], "Layer #"))));
    assert!(!CompositionSnapshot::same_menu(Some(&a), Some(&w5_snap(&["Connected", "Disconnected", "Empty"], &[], "Bass"))));
    assert!(!CompositionSnapshot::same_menu(Some(&a), Some(&w5_snap(&["Connected", "Disconnected", "Empty"], &[Some("x"), Some("clip 1")], "Layer #"))));
    assert!(!CompositionSnapshot::same_menu(Some(&a), None));
    assert!(CompositionSnapshot::same_menu(None, None));
}

// MARK: decoder

#[test]
fn decode_minimal_composition() {
    let json = r#"{"name":{"value":"Comp","id":1},"decks":[{"name":{"value":"A"},"selected":{"value":false}},{"name":{"value":"Main"},"selected":{"value":true}}],
     "layers":[{"id":5,"name":{"value":"Layer #","id":55},"clips":[
        {"id":7,"name":{"value":"Clip A"},"connected":{"value":"Connected"}},
        {"id":8,"name":{"value":""},"connected":{"value":"Empty"}}]}],
     "columns":[{"id":9,"name":{"value":"Column #","id":99}},{"id":10,"name":{"value":"Intro","id":100}}]}"#;
    let s = decoder::decode(json.as_bytes()).unwrap();
    assert!(decoder::looks_like_composition(json.as_bytes()));
    assert_eq!(s.deck_name, "Main");
    assert_eq!(s.layers[0].display_name(), "Layer 1");
    assert_eq!(s.layers[0].name_param_id, Some(55));
    assert_eq!(s.layers[0].slots[0].clip_name.as_deref(), Some("Clip A"));
    assert!(s.layers[0].slots[0].is_playing());
    assert!(!s.layers[0].slots[1].is_occupied());
    assert!(s.columns[1].has_custom_name());
    assert_eq!(s.last_occupied_column(), 1);
}

#[test]
fn decode_escapes_unicode_nulls_and_unknown_members() {
    let json = r#"{"name" : {"valuetype":"ParamString","id":1,"value":"Comp \"Q\" \\ \/ é🎵 #"},
     "extra":{"layers":[{"id":"not this"}],"s":"]}[{"},
     "decks":null,
     "layers":[ {"clips":[{"name":{"value":"x"},"connected":{"value":"Disconnected"},"id":-3,"video":{"a":[1.5e3,true,null]}}],
                 "name":null,"id":42} ],
     "columns":[{"name":{"value":"A\nB","id":7},"id":8}]}"#;
    let s = decoder::decode(json.as_bytes()).unwrap();
    assert_eq!(s.composition_name, "Comp \"Q\" \\ / é🎵 #");
    assert_eq!(s.deck_name, "");
    assert_eq!(s.layers.len(), 1);
    assert_eq!(s.layers[0].id, 42);
    assert_eq!(s.layers[0].raw_name, "Layer #");
    assert_eq!(s.layers[0].slots[0].clip_id, -3);
    assert_eq!(s.layers[0].slots[0].clip_name.as_deref(), Some("x"));
    assert_eq!(s.columns[0].raw_name, "A\nB");
    assert_eq!(s.columns[0].name_param_id, Some(7));
}

#[test]
fn decode_surrogate_pairs_and_lone_surrogates() {
    let json = r#"{"name":{"value":"🎵 \ud800 é"},"layers":[],"columns":[]}"#;
    let s = decoder::decode(json.as_bytes()).unwrap();
    assert_eq!(s.composition_name, "🎵 \u{FFFD} é");
}

#[test]
fn decode_malformed_errors() {
    for bad in [r#"{"name":{"value":"x"},"layers":[{"id":1"#, r#"{"name":1}"#, r#"{"layers":[],"columns":[{"name":{}}]}"#, "{\"name\":\"x"] {
        assert!(decoder::decode(bad.as_bytes()).is_err(), "{bad}");
    }
    assert_eq!(
        decoder::decode(br#"{"name":{"value":"x"}}"#).unwrap_err(),
        ArenaError::BadResponse("composition JSON: no layers/columns at byte 22".to_string())
    );
    assert_eq!(
        decoder::decode(br#"{"layers":[],"columns":[{"name":{}}]}"#).unwrap_err().plain_line(),
        "Unexpected answer from Arena: composition JSON: column without id at byte 35"
    );
    assert_eq!(
        decoder::decode(br#"{"layers":[{"id":99999999999999999999}],"columns":[]}"#).unwrap_err(),
        ArenaError::BadResponse("composition JSON: integer overflow at byte 35".to_string())
    );
}

#[test]
fn decode_is_deterministic() {
    let data = br#"{"name":{"value":"C"},"layers":[{"id":1,"name":{"value":"L","id":2},"clips":[{"id":3,"name":{"value":"a"},"connected":{"value":"Connected"}}]}],"columns":[{"id":4,"name":{"value":"Column #","id":5}}]}"#;
    assert_eq!(decoder::decode(data).unwrap(), decoder::decode(data).unwrap());
}

/// Real-size composition (24 x 247, ~29 MB like Arena's push on a large comp).
#[test]
fn decode_real_size_composition() {
    let bulk = r#"{"valuetype":"ParamRange","id":123456789,"value":0.5,"min":0,"max":1,"options":["a","b]"]},"#.repeat(52);
    let mut json =
        String::from(r#"{"name":{"valuetype":"ParamString","id":1,"value":"Big"},"decks":[{"name":{"value":"Main"},"selected":{"value":true}}],"layers":["#);
    for l in 1..=24 {
        json += &format!(r#"{{"id":{l},"name":{{"value":"Layer #","id":{}}},"video":{{"effects":[{bulk}{{}}]}},"clips":["#, 1000 + l);
        let clips: Vec<String> = (1..=247)
            .map(|c| {
                let occ = c <= 16;
                format!(
                    r#"{{"id":{},"name":{{"value":"{}"}},"connected":{{"value":"{}"}},"video":{{"params":[{bulk}{{}}]}}}}"#,
                    l * 10_000 + c,
                    if occ { format!("Clip {c}") } else { String::new() },
                    if occ { "Disconnected" } else { "Empty" }
                )
            })
            .collect();
        json += &clips.join(",");
        json += "]}";
        if l < 24 {
            json += ",";
        }
    }
    json += r#"],"columns":["#;
    json += &(1..=247).map(|c| format!(r#"{{"id":{c},"name":{{"value":"Column #","id":{}}}}}"#, 5000 + c)).collect::<Vec<_>>().join(",");
    json += "]}";
    let data = json.into_bytes();
    assert!(data.len() > 25_000_000);
    let t0 = Instant::now();
    let s = decoder::decode(&data).unwrap();
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("LEAN: {:.1} MB decoded in {ms:.1} ms", data.len() as f64 / 1e6);
    assert_eq!(s.layers.len(), 24);
    assert_eq!(s.columns.len(), 247);
    assert_eq!(s.last_occupied_column(), 16);
    assert_eq!(s.layers[23].slots[246].clip_id, 240_247);
    assert!(ms < 2000.0);
}

/// The menu is built from the cache at a large comp's scale (24 layers x 247 columns).
#[test]
fn menu_build_at_real_scale() {
    let layers: Vec<LayerInfo> = (1..=24)
        .map(|l| {
            LayerInfo::new(
                l,
                l as i64,
                "Layer #",
                None,
                (1..=247)
                    .map(|c| {
                        let occ = c <= 16;
                        SlotInfo::new(c, (l * 1000 + c) as i64, occ.then(|| format!("Clip {c}")), if occ { "Disconnected" } else { "Empty" })
                    })
                    .collect(),
            )
        })
        .collect();
    let snap = CompositionSnapshot::new("Big", "Main", layers, (1..=247).map(|c| ColumnInfo::new(c, c as i64, "Column #", None)).collect());
    let cache = CacheState { status: connected(), snapshot: Some(snap), host_pid: 1, written_at: Timestamp::now(), updated_at: None };
    let bytes = glance_arena::swift_json::to_vec(&cache).unwrap();
    let t0 = Instant::now();
    let decoded: CacheState = serde_json::from_slice(&bytes).unwrap();
    let nodes = SendMenuModel::build(&SendMenuInput::new(decoded.snapshot, decoded.status, 3));
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("SCALE: cache {} bytes, decode+build {ms:.1} ms, {} top items", bytes.len(), nodes.len());
    assert!(ms < 250.0);
    // 16 occupied + 8 extra shown, the rest in chunks of 50 under "More Columns".
    let l24 = submenu(&nodes, "Layer 24");
    let MenuNode::Submenu { title, children } = l24.last().unwrap() else { panic!() };
    assert_eq!(title, "More Columns (25\u{2013}247)");
    assert_eq!(
        titles(children),
        ["Columns 25\u{2013}74", "Columns 75\u{2013}124", "Columns 125\u{2013}174", "Columns 175\u{2013}224", "Columns 225\u{2013}247"]
    );
}

// MARK: reconnect policy

#[test]
fn reconnect_policy_table() {
    let p = ReconnectPolicy::default();
    assert_eq!(p.delay(true, Some(false), 3, None), None, "local, not running: wait for launch");
    assert_eq!(p.delay(true, Some(false), 1, Some(5.0)), None, "quit right after launch: wait again");
    assert_eq!(p.delay(true, Some(true), 9, Some(5.0)), Some(1.0), "starting up: every second");
    assert_eq!(p.delay(true, Some(true), 9, Some(59.0)), Some(1.0));
    let backoff: Vec<Option<f64>> = (1..=8).map(|f| p.delay(true, Some(true), f, Some(61.0))).collect();
    let want: Vec<Option<f64>> = [1.0, 2.0, 4.0, 8.0, 16.0, 30.0, 30.0, 30.0].into_iter().map(Some).collect();
    assert_eq!(backoff, want, "running, webserver off: backoff capped at 30 s");
    let remote: Vec<Option<f64>> = (1..=8).map(|f| p.delay(false, None, f, None)).collect();
    assert_eq!(remote, want, "remote: backoff capped at 30 s");
    assert_eq!(p.delay(false, Some(false), 1, None), Some(1.0), "remote never waits on the local process");
    assert_eq!(p.delay(true, None, 2, None), Some(2.0), "unknown process state: backoff");
    assert_eq!(p.delay(false, None, 1000, None), Some(30.0));
}

// MARK: error texts

#[test]
fn error_texts_verbatim() {
    let h = |status: i64, body: &str| ArenaError::Http { status, path: "/composition/layers/1/clips/2/open".into(), body: body.into() }.plain_line();
    assert_eq!(h(404, ""), "not found: /composition/layers/1/clips/2/open");
    assert_eq!(h(412, "busy"), "Arena refused (composition locked or clip busy): /composition/layers/1/clips/2/open - busy");
    assert_eq!(h(400, ""), "bad request: /composition/layers/1/clips/2/open");
    assert_eq!(h(500, "x"), "HTTP 500: /composition/layers/1/clips/2/open - x");
    assert_eq!(ArenaError::NotRunning.plain_line(), "Arena is not running - start Resolume Arena.");
    assert_eq!(
        ArenaError::Unreachable { host: "10.0.0.5".into(), port: 8080, detail: "timed out".into() }.plain_line(),
        "Arena not reachable at 10.0.0.5:8080 - timed out"
    );
    assert_eq!(ArenaError::BadResponse("x".into()).plain_line(), "Unexpected answer from Arena: x");
    assert_eq!(ArenaError::LayerGone { layer_id: 7 }.plain_line(), "That layer no longer exists in Arena (id 7) - pick again.");
    assert_eq!(ArenaError::NoFiles.plain_line(), "No files Arena can open in the selection.");
    assert_eq!(ArenaError::Cancelled.plain_line(), "Cancelled.");
    assert_eq!(ArenaStatus::Unknown.plain_line(), "Checking Arena...");
    assert_eq!(connected().plain_line(), "Connected to Arena");
}

#[test]
fn outcome_summaries_verbatim() {
    let res = |c: usize, ok: bool| SlotResult {
        layer_index: 2,
        column: c,
        path: format!("/x/{c}.mov"),
        error: None,
        after: Some(SlotInfo::new(c, 1, None, if ok { "Disconnected" } else { "Empty" })),
    };
    let plan = LoadPlan { layer_index: 2, add_layer: false, add_columns: 0, assignments: vec![] };
    let o = LoadOutcome { plan: plan.clone(), layer_id: Some(1), results: vec![res(3, true), res(4, true), res(5, false)], triggered: true, skipped: 1 };
    assert_eq!(o.summary(), "Loaded 2/3 into Layer 2, columns 3-5, skipped 1 occupied, triggered");
    let one = LoadOutcome { plan: plan.clone(), layer_id: None, results: vec![res(1, true)], triggered: false, skipped: 0 };
    assert_eq!(one.summary(), "Loaded 1/1 into Layer 2, column 1");
    let none = LoadOutcome { plan, layer_id: None, results: vec![], triggered: false, skipped: 0 };
    assert_eq!(none.summary(), "Loaded 0/0 into Layer 2");
}
