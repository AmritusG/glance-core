// SPDX-License-Identifier: MIT
//! The loader against the fake Arena: trigger scope, layer-index mismatch logging, occupied-slot
//! questions, renames, column adds, remaps, verification.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{FSlot, FakeArena};
use glance_arena::*;

fn groups() -> Vec<FolderGroup> {
    vec![
        FolderGroup::new("/x/Alpha", vec!["/x/Alpha/a1.mov".into(), "/x/Alpha/a2.mov".into()]),
        FolderGroup::new("/x/Bravo", vec!["/x/Bravo/b1.mov".into()]),
        FolderGroup::new("/x/Charlie", vec!["/x/Charlie/c1.mov".into()]),
    ]
}

fn loader(fake: &FakeArena, log: &EventLog) -> ArenaLoader {
    ArenaLoader::new(fake.client(Some(true)), vec![], Some(log.clone()))
}

#[test]
fn trigger_scope_all_vs_top_only() {
    // 1 layer x 4 columns; 3 folders -> new layers 4 (Alpha, top), 3, 2.
    let fake = FakeArena::start(1, 4);
    for (trigger, scope, expected) in [
        (true, TriggerScope::AllNewLayers, vec!["L4 C1", "L3 C1", "L2 C1"]),
        (true, TriggerScope::TopLayerOnly, vec!["L4 C1"]),
        (false, TriggerScope::AllNewLayers, vec![]),
        (false, TriggerScope::TopLayerOnly, vec![]),
    ] {
        fake.reset(1, 4);
        let out = loader(&fake, &EventLog::new()).load_layers(&groups(), trigger, scope, None).unwrap();
        assert_eq!(fake.lock().connects, expected, "{trigger} {scope:?}");
        assert_eq!(out.layers.iter().map(|l| l.triggered).collect::<Vec<_>>(), (0..out.layers.len()).map(|i| trigger && scope.triggers(i)).collect::<Vec<_>>());
        assert_eq!(out.layers.iter().map(LoadOutcome::ok_count).collect::<Vec<_>>(), [2, 1, 1]);
        // Only first clips: Alpha's second clip is never connected.
        assert_eq!(fake.lock().layers[3].slots[1].state, "Disconnected");
        // Each new layer is named after its folder.
        let names: Vec<String> = fake.lock().layers.iter().map(|l| l.name.clone()).collect();
        assert_eq!(names, ["Layer #", "Charlie", "Bravo", "Alpha"]);
    }
    let out = loader(&fake, &EventLog::new()).load_layers(&groups(), true, TriggerScope::TopLayerOnly, None);
    assert!(out.unwrap().summary().starts_with(
        "Loaded 4/4 into 3 new layers, top first: 7 \u{201C}Alpha\u{201D} 2/2, 6 \u{201C}Bravo\u{201D} 1/1, 5 \u{201C}Charlie\u{201D} 1/1, triggered"
    ));
}

#[test]
fn stale_cache_index_mismatch_logged() {
    // Menu was built from a cache showing 4 layers (-> 5-7); Arena has 1 now.
    let fake = FakeArena::start(1, 4);
    let log = EventLog::new();
    let out = loader(&fake, &log).load_layers(&groups(), false, TriggerScope::AllNewLayers, Some(5..=7)).unwrap();
    assert_eq!(out.plan.layers.iter().map(|l| l.layer_index).collect::<Vec<_>>(), [4, 3, 2]);
    assert!(log.contains(|t| t == "Menu showed Layers 5\u{2013}7 (cache at menu time); Arena made Layers 2\u{2013}4"), "{}", log.text());

    // Matching cache: no mismatch line.
    fake.reset(1, 4);
    let quiet = EventLog::new();
    loader(&fake, &quiet).load_layers(&groups(), false, TriggerScope::AllNewLayers, Some(2..=4)).unwrap();
    assert!(!quiet.contains(|t| t.starts_with("Menu showed")), "{}", quiet.text());

    // An empty folder makes one layer fewer than the menu could know.
    fake.reset(1, 4);
    let fewer = EventLog::new();
    let g = groups();
    let with_empty = vec![g[0].clone(), FolderGroup::new("/x/Empty", vec![]), g[2].clone()];
    loader(&fake, &fewer).load_layers(&with_empty, false, TriggerScope::AllNewLayers, Some(2..=4)).unwrap();
    assert!(fewer.contains(|t| t.ends_with("Arena made Layers 2\u{2013}3")), "{}", fewer.text());
    assert!(fewer.contains(|t| t == "Folder \u{201C}Empty\u{201D} has no media file - no layer"));

    // Single folder (Folder as New Layer) path.
    fake.reset(2, 4);
    let single = EventLog::new();
    loader(&fake, &single)
        .load(&["/x/a.mov".to_string()], &Destination::NewNamedLayer { name: "A".into() }, false, false, Some(9..=9), &|_| ConflictResolution::Cancel)
        .unwrap();
    assert!(single.contains(|t| t == "Menu showed Layer 9 (cache at menu time); Arena made Layer 3"), "{}", single.text());
    assert!(single.contains(|t| t == "Named layer 3 \u{201C}A\u{201D}"));
    assert_eq!(fake.lock().layers[2].name, "A");
    assert_eq!(fake.lock().puts.last().map(String::as_str), Some(r#"{"name":{"value":"A"}}"#));
}

#[test]
fn occupied_slots_ask_skip_replace_cancel() {
    let fake = FakeArena::start(1, 4);
    let files: Vec<String> = ["/m/a.mov", "/m/b.mov", "/m/c.mov"].iter().map(|s| s.to_string()).collect();
    let dest = Destination::Slot { layer_id: 500, column: 1 };
    let occupy = || {
        fake.reset(1, 4);
        fake.lock().layers[0].slots[1] = FSlot::loaded("busy.mov", "Disconnected");
    };
    // Skip: the occupied column 2 keeps its clip.
    occupy();
    let log = EventLog::new();
    let asked = std::sync::Mutex::new(Vec::new());
    let out = loader(&fake, &log)
        .load(&files, &dest, false, false, None, &|c| {
            asked.lock().unwrap().push(c.to_vec());
            ConflictResolution::Skip
        })
        .unwrap();
    assert_eq!(asked.lock().unwrap().len(), 1);
    assert_eq!(asked.lock().unwrap()[0][0].replacing.as_deref(), Some("busy.mov"));
    assert_eq!(out.results.iter().map(|r| r.column).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(out.skipped, 1);
    assert_eq!(out.summary(), "Loaded 2/2 into Layer 1, columns 1-3, skipped 1 occupied");
    assert!(log.contains(|t| t == "Occupied slots col 2 \"busy.mov\" -> skip"));
    assert_eq!(fake.lock().layers[0].slots[1].name, "busy.mov");
    // Cancel: nothing opened.
    occupy();
    let e = loader(&fake, &EventLog::new()).load(&files, &dest, false, false, None, &|_| ConflictResolution::Cancel).unwrap_err();
    assert_eq!(e, ArenaError::Cancelled);
    assert!(fake.lock().opened.len() == 2, "only the skip run opened files");
    // Replace confirmed by the menu item: no question.
    occupy();
    let out = loader(&fake, &EventLog::new()).load(&files, &dest, true, true, None, &|_| panic!("must not ask")).unwrap();
    assert_eq!(out.ok_count(), 3);
    assert!(out.triggered);
    assert_eq!(fake.lock().layers[0].slots[1].name, "b.mov");
    assert_eq!(fake.lock().connects, ["L1 C1"]);
}

#[test]
fn next_free_and_append_add_columns_and_remap() {
    let fake = FakeArena::start(1, 2);
    fake.lock().layers[0].slots[0] = FSlot::loaded("x.mov", "Connected");
    let l = ArenaLoader::new(fake.client(Some(true)), vec![PathRemap::new("/Volumes/Here", "/Volumes/There")], Some(EventLog::new()));
    let files: Vec<String> = ["/Volumes/Here/A B/1.mov", "/Volumes/Here/2#.mov", "/x/notes.txt"].iter().map(|s| s.to_string()).collect();
    let out = l.load(&files, &Destination::NextFree { layer_id: 500 }, false, false, None, &|_| ConflictResolution::Cancel).unwrap();
    assert_eq!(out.results.iter().map(|r| r.column).collect::<Vec<_>>(), [2, 3]);
    assert_eq!(fake.lock().columns, 3, "one column added");
    assert_eq!(fake.lock().opened, ["file:///Volumes/There/A%20B/1.mov", "file:///Volumes/There/2%23.mov"], "remapped and encoded; the .txt was dropped");
    let out = l.load(&files[..1], &Destination::AppendColumns { layer_id: 500 }, false, false, None, &|_| ConflictResolution::Cancel).unwrap();
    assert_eq!(out.results[0].column, 4);
    assert_eq!(fake.lock().columns, 4);
    // Layer gone; no media at all.
    assert_eq!(
        l.load(&files[..1], &Destination::NextFree { layer_id: 1 }, false, false, None, &|_| ConflictResolution::Cancel).unwrap_err(),
        ArenaError::LayerGone { layer_id: 1 }
    );
    assert_eq!(
        l.load(&["/x/a.txt".to_string()], &Destination::NewLayer, false, false, None, &|_| ConflictResolution::Cancel).unwrap_err(),
        ArenaError::NoFiles
    );
}

#[test]
fn per_file_failures_do_not_abort() {
    let fake = FakeArena::start(1, 4);
    fake.lock().fail_open = Some("missing".into());
    let log = EventLog::new();
    let files: Vec<String> = ["/m/a.mov", "/m/missing.mov", "/m/c.mov"].iter().map(|s| s.to_string()).collect();
    let out = loader(&fake, &log).load(&files, &Destination::Slot { layer_id: 500, column: 1 }, true, false, None, &|_| ConflictResolution::Cancel).unwrap();
    assert_eq!(out.results.iter().map(SlotResult::ok).collect::<Vec<_>>(), [true, false, true]);
    assert_eq!(out.results[1].error, Some(ArenaError::Http { status: 404, path: "/composition/layers/1/clips/2/open".into(), body: "file not found".into() }));
    assert_eq!(out.summary(), "Loaded 2/3 into Layer 1, columns 1-3, triggered");
    assert!(log.contains(|t| t == "open L1 C2 <- /m/missing.mov FAILED: not found: /composition/layers/1/clips/2/open - file not found"), "{}", log.text());
    assert!(log.contains(|t| t == "open L1 C1 <- /m/a.mov => Disconnected \"a.mov\""), "{}", log.text());
    assert!(log.contains(|t| t == "Triggered L1 C1"));

    // A 412 on every request fails the load at its fresh snapshot.
    fake.reset(1, 2);
    fake.lock().override_answer = Some((412, "locked".into()));
    let e = loader(&fake, &EventLog::new()).load(&files, &Destination::NewLayer, false, false, None, &|_| ConflictResolution::Cancel).unwrap_err();
    assert_eq!(e.plain_line(), "Arena refused (composition locked or clip busy): /composition - locked");
}

#[test]
fn load_reads_fresh_while_menu_is_stale() {
    let fake = FakeArena::start(1, 3);
    let store = CompositionStore::new(fake.client(Some(true)), ArenaUpdateMode::OnDemand, std::time::Duration::from_secs(60));
    store.start();
    common::sleep(0.3);
    assert!(!store.snapshot().unwrap().layers[0].slots[0].is_occupied());
    fake.lock().layers[0].slots[0] = FSlot::loaded("other.mov", "Disconnected");
    let out = loader(&fake, &EventLog::new())
        .load(&["/x/a.mov".to_string()], &Destination::NextFree { layer_id: 500 }, false, false, None, &|_| ConflictResolution::Cancel)
        .unwrap();
    assert_eq!(out.plan.assignments.iter().map(|a| a.column).collect::<Vec<_>>(), [2], "planned on Arena's real state");
    assert!(!store.snapshot().unwrap().layers[0].slots[0].is_occupied(), "the store was not asked");
    store.stop();
}

#[test]
fn a_slot_that_fills_after_the_open_answers_counts_as_loaded_and_is_triggered() {
    // Arena answers the open before the clip is in the slot: the read-back waits for it (fixed on purpose; SendEhrKit
    // 1.1.2 read once and reported "Loaded 0/1", and Trigger After Load then triggered nothing).
    let fake = FakeArena::start(1, 2);
    fake.lock().load_delay_reads = 3;
    let log = EventLog::new();
    let out = loader(&fake, &log).load(&["/x/a.mov".to_string()], &Destination::NewLayer, true, false, None, &|_| ConflictResolution::Cancel).unwrap();
    assert_eq!(out.ok_count(), 1, "{}", log.text());
    assert!(out.triggered);
    assert_eq!(fake.lock().connects, vec!["L2 C1".to_string()]);
}

#[test]
fn a_slot_that_never_fills_is_reported_as_it_reads_after_the_settle_time() {
    let fake = FakeArena::start(1, 2);
    fake.lock().load_delay_reads = 10_000;
    let log = EventLog::new();
    let started = std::time::Instant::now();
    let out = loader(&fake, &log).load(&["/x/a.mov".to_string()], &Destination::NewLayer, true, false, None, &|_| ConflictResolution::Cancel).unwrap();
    assert_eq!(out.ok_count(), 0);
    assert!(!out.triggered);
    assert!(started.elapsed() >= glance_arena::READ_BACK_SETTLE);
    assert!(started.elapsed() < glance_arena::READ_BACK_SETTLE * 3);
}
