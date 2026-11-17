// SPDX-License-Identifier: MIT
//! The host-facing controller: redraw rules, the host's default update mode against the user's
//! pick, menu-open rebuilds, loads with the three dialogs, recents.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{FSlot, FakeArena, closed_port, sleep, temp_dir};
use glance_arena::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Seen {
    redraws: AtomicUsize,
    states: AtomicUsize,
    replace: Mutex<Vec<Prompt>>,
    large: Mutex<Vec<Prompt>>,
    errors: Mutex<Vec<Prompt>>,
}

fn hooks(seen: &Arc<Seen>, running: Option<bool>, replace: ConflictResolution, large: bool) -> ControllerHooks {
    let (a, b, c, d, e) = (seen.clone(), seen.clone(), seen.clone(), seen.clone(), seen.clone());
    ControllerHooks {
        is_arena_running: Arc::new(move || running),
        ask_replace: Arc::new(move |p, _| {
            a.replace.lock().unwrap().push(p.clone());
            replace
        }),
        ask_large_load: Arc::new(move |p| {
            b.large.lock().unwrap().push(p.clone());
            large
        }),
        show_error: Arc::new(move |p, _| c.errors.lock().unwrap().push(p.clone())),
        redraw: Arc::new(move || {
            d.redraws.fetch_add(1, Ordering::SeqCst);
        }),
        state_changed: Arc::new(move || {
            e.states.fetch_add(1, Ordering::SeqCst);
        }),
        date_added: None,
    }
}

fn shared_with(json: Option<&str>) -> (SharedStore, PathBuf) {
    let dir = temp_dir("ctl");
    if let Some(j) = json {
        std::fs::write(dir.join("settings.json"), j).unwrap();
    }
    (SharedStore::new(&dir), dir)
}

fn w5_snap(states: &[&str]) -> CompositionSnapshot {
    CompositionSnapshot::new(
        "C",
        "D",
        vec![LayerInfo::new(
            1,
            1,
            "Layer #",
            Some(2),
            states.iter().enumerate().map(|(i, st)| SlotInfo::new(i + 1, 100 + i as i64, (*st != "Empty").then(|| format!("clip {i}")), *st)).collect(),
        )],
        (1..=states.len()).map(|c| ColumnInfo::new(c, 900 + c as i64, "Column #", None)).collect(),
    )
}

/// The redraw hook only for menu layout or status changes; state_changed for every delivery.
#[test]
fn controller_redraws_only_for_menu_or_status() {
    let (shared, dir) = shared_with(None);
    let seen = Arc::new(Seen::default());
    let c = Controller::new(shared, hooks(&seen, None, ConflictResolution::Cancel, false));
    let ok = ArenaStatus::Connected { product: "Arena 7.28.1".into() };
    c.receive(Some(w5_snap(&["Connected", "Disconnected", "Empty"])), ok.clone(), None);
    assert_eq!(seen.redraws.load(Ordering::SeqCst), 1);
    c.receive(Some(w5_snap(&["Disconnected", "Connected", "Empty"])), ok.clone(), None); // autopilot moved the play mark
    assert_eq!(seen.redraws.load(Ordering::SeqCst), 1);
    assert_eq!(seen.states.load(Ordering::SeqCst), 2);
    assert_eq!(c.snapshot().unwrap().layers[0].slots[1].state, "Connected", "the value still updates");
    c.receive(Some(w5_snap(&["Disconnected", "Connected", "Disconnected"])), ok, None); // occupancy
    assert_eq!(seen.redraws.load(Ordering::SeqCst), 2);
    c.receive(None, ArenaStatus::Failed(ArenaError::NotRunning), None);
    assert_eq!(seen.redraws.load(Ordering::SeqCst), 3);
    assert_eq!(c.redraws(), 3);
    let _ = std::fs::remove_dir_all(dir);
}

/// A new fetch time alone never redraws (an open menu would fetch again).
#[test]
fn controller_updated_at_does_not_redraw() {
    let (shared, dir) = shared_with(None);
    let seen = Arc::new(Seen::default());
    let c = Controller::new(shared, hooks(&seen, None, ConflictResolution::Cancel, false));
    let snap = CompositionSnapshot::new("C", "D", vec![], vec![]);
    let ok = ArenaStatus::Connected { product: "Arena".into() };
    c.receive(Some(snap.clone()), ok.clone(), Some(Timestamp(1.0)));
    c.receive(Some(snap), ok, Some(Timestamp(2.0)));
    assert_eq!(c.redraws(), 1);
    assert_eq!(seen.states.load(Ordering::SeqCst), 2, "the host is still told");
    assert_eq!(c.updated_at(), Some(Timestamp(2.0)));
    assert_eq!(c.menu_input(&["/x/a.mov"]).updated_at, Some(Timestamp(2.0)));
    let _ = std::fs::remove_dir_all(dir);
}

// MARK: a host's default mode; a user's pick always wins (endpoints point at a closed port)

fn endpoint_json(port: u16) -> String {
    format!(r#""endpoint":{{"host":"127.0.0.1","port":{port}}}"#)
}

fn launch(shared: SharedStore, default: ArenaUpdateMode, port: u16) -> Controller {
    let c = Controller::new(shared, ControllerHooks { is_arena_running: Arc::new(|| Some(true)), ..ControllerHooks::default() });
    if c.settings().endpoint.port != i64::from(port) {
        // No file: keep the test off any real Arena.
        let mut s = c.settings();
        s.endpoint = ArenaEndpoint::new("127.0.0.1", i64::from(port));
        c.update_settings(s);
    }
    c.start(default);
    sleep(0.3);
    c
}

#[test]
fn no_file_uses_host_default() {
    let port = closed_port();
    let (shared, dir) = shared_with(None);
    let c = launch(shared, ArenaUpdateMode::OnDemand, port);
    assert_eq!(c.settings().update_mode, ArenaUpdateMode::OnDemand);
    assert!(!c.settings().update_mode_chosen);
    assert_eq!(c.store().mode(), ArenaUpdateMode::OnDemand);
    assert_eq!(c.store().activity(), Activity::default());
    assert_eq!(c.store().attempts(), 0, "no Live socket was ever opened");
    assert!(c.log().contains(|t| t == "Arena Updates: host default On Demand (no user pick; file said Live)"), "{}", c.log().text());
    assert!(c.log().contains(|t| t == format!("SendEhrKit start: 127.0.0.1:{port}, Arena Updates On Demand")));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn old_file_without_flag_uses_host_default() {
    let port = closed_port();
    let (shared, dir) = shared_with(Some(&format!(r#"{{"updateMode":"live","updateInterval":60,{}}}"#, endpoint_json(port))));
    assert!(!shared.read_settings().update_mode_chosen, "missing key reads as not chosen");
    let c = launch(shared.clone(), ArenaUpdateMode::OnDemand, port);
    assert_eq!(c.store().mode(), ArenaUpdateMode::OnDemand);
    assert_eq!(c.store().attempts(), 0);
    let on_disk = shared.read_settings();
    assert_eq!(on_disk.update_mode, ArenaUpdateMode::OnDemand, "file follows the effective mode");
    assert!(!on_disk.update_mode_chosen, "still the host's default, not a pick");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn user_picked_live_wins_over_on_demand_host() {
    let port = closed_port();
    let (shared, dir) = shared_with(Some(&format!(r#"{{"updateMode":"live","updateInterval":60,"updateModeChosen":true,{}}}"#, endpoint_json(port))));
    let c = launch(shared, ArenaUpdateMode::OnDemand, port);
    assert_eq!(c.store().mode(), ArenaUpdateMode::Live);
    assert!(c.store().attempts() >= 1, "websocket attempt");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn user_picked_interval_kept() {
    let port = closed_port();
    let (shared, dir) = shared_with(Some(&format!(r#"{{"updateMode":"interval","updateInterval":30,"updateModeChosen":true,{}}}"#, endpoint_json(port))));
    let c = launch(shared, ArenaUpdateMode::OnDemand, port);
    assert_eq!(c.store().mode(), ArenaUpdateMode::Interval);
    assert_eq!(c.store().interval(), Duration::from_secs(30));
    assert!(c.store().activity().interval_timer);
    let _ = std::fs::remove_dir_all(dir);
}

/// A Live host (SendEhr.app's default): every file it can meet stays as before.
#[test]
fn live_host_unaffected() {
    let port = closed_port();
    let ep = endpoint_json(port);
    let live_no_flag = format!(r#"{{"updateMode":"live","updateInterval":60,{ep}}}"#);
    let picked_live = format!(r#"{{"updateMode":"live","updateInterval":60,"updateModeChosen":true,{ep}}}"#);
    for json in [None, Some(live_no_flag.as_str()), Some(picked_live.as_str())] {
        let (shared, dir) = shared_with(json);
        let c = launch(shared, ArenaUpdateMode::Live, port);
        assert_eq!(c.store().mode(), ArenaUpdateMode::Live, "{json:?}");
        let _ = std::fs::remove_dir_all(dir);
    }
    let picked_interval = format!(r#"{{"updateMode":"interval","updateInterval":30,"updateModeChosen":true,{ep}}}"#);
    let old_interval = format!(r#"{{"updateMode":"interval","updateInterval":30,{ep}}}"#);
    let old_on_demand = format!(r#"{{"updateMode":"onDemand","updateInterval":60,{ep}}}"#);
    for (json, mode) in [(picked_interval, ArenaUpdateMode::Interval), (old_interval, ArenaUpdateMode::Interval), (old_on_demand, ArenaUpdateMode::OnDemand)] {
        let (shared, dir) = shared_with(Some(&json));
        let c = launch(shared, ArenaUpdateMode::Live, port);
        assert_eq!(c.store().mode(), mode, "an older non-Live pick survives: {json}");
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Menu pick → remembered → the next launch of an On Demand host stays on the pick.
#[test]
fn menu_pick_is_remembered_across_launch() {
    let port = closed_port();
    let (shared, dir) = shared_with(Some(&format!(r#"{{"updateMode":"live","updateInterval":60,{}}}"#, endpoint_json(port))));
    let c = launch(shared.clone(), ArenaUpdateMode::OnDemand, port);
    let MenuNode::Submenu { children, .. } = SendMenuModel::update_nodes(&c.settings()) else { panic!() };
    let MenuNode::Toggle { title, action, .. } = &children[0] else { panic!() };
    assert_eq!(title, "Live");
    assert!(c.perform::<&str>(action, title, &[]).is_none());
    assert!(shared.read_settings().update_mode_chosen);
    assert!(c.log().contains(|t| t == "Setting: Arena Updates Live"));
    c.store().stop();
    drop(c);
    let again = launch(shared, ArenaUpdateMode::OnDemand, port);
    assert_eq!(again.store().mode(), ArenaUpdateMode::Live);
    let _ = std::fs::remove_dir_all(dir);
}

// MARK: menu opening rebuilds outside Live

fn layer_titles(nodes: &[MenuNode]) -> usize {
    nodes.iter().filter(|n| matches!(n, MenuNode::Submenu { title, .. } if title.starts_with("Layer "))).count()
}

fn chosen(mode: &str, fake: &FakeArena) -> String {
    format!(r#"{{"updateMode":"{mode}","updateModeChosen":true,{}}}"#, endpoint_json(fake.port))
}

#[test]
fn menu_open_rebuilds_and_fetches_on_demand() {
    let fake = FakeArena::start(1, 3);
    let (shared, dir) = shared_with(Some(&chosen("onDemand", &fake)));
    let seen = Arc::new(Seen::default());
    let c = Controller::new(shared, hooks(&seen, Some(true), ConflictResolution::Cancel, false));
    c.store().set_menu_fetch_gap(Duration::from_secs(1));
    let urls = ["/x/a.mov"];
    let will_change = || seen.redraws.load(Ordering::SeqCst);

    c.start(ArenaUpdateMode::OnDemand);
    sleep(0.4);
    assert_eq!(c.store().fetches(), 1, "on demand: one fetch at start");
    let base = will_change();

    // Before any Send to Arena menu was built, other menus opening change nothing.
    c.menu_opening();
    assert_eq!(c.menu_invalidations(), 0);
    assert_eq!(will_change(), base);

    // First open: the host builds the content (nodes wakes the store).
    assert_eq!(layer_titles(&c.nodes(&urls)), 1);
    sleep(0.4);
    assert_eq!(c.store().fetches(), 2, "the first build fetches");

    // A layer is added in Arena; a cached menu would keep showing one layer.
    fake.reset(2, 3);
    sleep(0.7); // past menu_fetch_gap since the first build

    // Next open: the controller drops the content (redraw) ...
    c.menu_opening();
    assert_eq!(c.menu_invalidations(), 1);
    assert_eq!(will_change(), base + 1, "content dropped when the menu opens");
    // ... the host builds it again, which wakes the store ...
    let _ = c.nodes(&urls);
    sleep(0.4);
    assert_eq!(c.store().fetches(), 3, "the reopen fetches");
    // ... and the fetched layer redraws the menu of this open.
    assert_eq!(will_change(), base + 2, "the new layer redraws");
    assert_eq!(layer_titles(&c.nodes(&urls)), 2, "the new layer is in the menu");

    // Opens within menu_fetch_gap rebuild (fresh "Updated n s ago") but do not fetch again.
    c.menu_opening();
    let _ = c.nodes(&urls);
    sleep(0.1);
    assert_eq!(c.menu_invalidations(), 2);
    assert_eq!(c.store().fetches(), 3, "throttled");
    c.store().stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn menu_open_in_live_only_while_disconnected() {
    let fake = FakeArena::start(1, 1);
    let (shared, dir) = shared_with(Some(&chosen("live", &fake)));
    let c = Controller::new(shared, ControllerHooks::default());
    let snap = CompositionSnapshot::new("C", "D", vec![], vec![]);
    c.receive(Some(snap), ArenaStatus::Connected { product: "Arena".into() }, None);
    let _ = c.nodes(&["/x/a.mov"]);
    c.store().stop();
    c.menu_opening();
    assert_eq!(c.menu_invalidations(), 0, "Live and connected: pushes keep the menu current");
    c.receive(None, ArenaStatus::Failed(ArenaError::NotRunning), None);
    c.menu_opening();
    assert_eq!(c.menu_invalidations(), 1, "Live and disconnected: rebuild (the build tries to connect)");
    let _ = std::fs::remove_dir_all(dir);
}

// MARK: loads through the controller

fn media(dir: &std::path::Path, rel: &str) -> String {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, b"x").unwrap();
    p.to_string_lossy().into_owned()
}

fn find_action(nodes: &[MenuNode], title: &str) -> Option<(MenuAction, String)> {
    for n in nodes {
        match n {
            MenuNode::Action { title: t, action, label } if t == title => return Some((action.clone(), label.clone())),
            MenuNode::Submenu { children, .. } => {
                if let Some(found) = find_action(children, title) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

#[test]
fn load_asks_replace_then_remembers_the_target() {
    let fake = FakeArena::start(1, 2);
    fake.lock().layers[0].slots[0] = FSlot::loaded("busy.mov", "Disconnected");
    let (shared, dir) = shared_with(Some(&chosen("onDemand", &fake)));
    let seen = Arc::new(Seen::default());
    let c = Controller::new(shared.clone(), hooks(&seen, Some(true), ConflictResolution::Replace, true));
    c.start(ArenaUpdateMode::OnDemand);
    sleep(0.3);
    let file = media(&dir, "clips/a.mov");
    let nodes = c.nodes(std::slice::from_ref(&file));
    let (action, label) = find_action(&nodes, "Column 1 \u{00B7} \u{25CF} busy.mov").unwrap();
    assert_eq!(label, "Layer 1 \u{203A} Column 1");
    c.perform(&action, &label, std::slice::from_ref(&file)).unwrap().join().unwrap();

    let asked = seen.replace.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].message, "Replace the clip in column 1?");
    assert_eq!(asked[0].informative, "Column 1: \u{201C}busy.mov\u{201D} \u{2192} a.mov");
    assert_eq!(asked[0].buttons, ["Replace", "Skip Occupied", "Cancel"]);
    assert_eq!(fake.lock().layers[0].slots[0].name, "a.mov");
    assert_eq!(c.last_outcome().as_deref(), Some("Loaded 1/1 into Layer 1, column 1"));
    assert!(!c.busy());
    let log = c.log();
    assert!(
        log.contains(|t| t == "Request: 1 item(s) -> Layer 1 \u{203A} Column 1, sort Name \u{2191}, subfolders off, ask over 50, trigger off (All New Layers)"),
        "{}",
        log.text()
    );
    assert!(log.contains(|t| t == "Occupied slots col 1 \"busy.mov\" -> replace"));
    // Remembered, on disk too, and offered under Recent.
    let recents = shared.read_recents();
    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].destination, Destination::Slot { layer_id: 500, column: 1 });
    assert_eq!(recents[0].label, "Layer 1 \u{203A} Column 1");
    sleep(0.3); // the refresh after the load
    let again = c.nodes(&[file]);
    assert_eq!(again[0], MenuNode::Info("Recent".into()));
    assert_eq!(again[1].title(), Some("Layer 1 \u{203A} Column 1 \u{00B7} \u{25CF} a.mov"));
    assert!(seen.errors.lock().unwrap().is_empty());
    c.store().stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn large_folder_ask_and_cancel() {
    let fake = FakeArena::start(1, 2);
    let (shared, dir) =
        shared_with(Some(&format!(r#"{{"updateMode":"onDemand","updateModeChosen":true,"largeFolderThreshold":2,{}}}"#, endpoint_json(fake.port))));
    let seen = Arc::new(Seen::default());
    let c = Controller::new(shared, hooks(&seen, Some(true), ConflictResolution::Cancel, false));
    for n in ["1", "2", "3"] {
        media(&dir, &format!("F/c{n}.mov"));
    }
    media(&dir, "F/readme.txt");
    let folder = dir.join("F").to_string_lossy().into_owned();
    let (tx, rx) = std::sync::mpsc::channel();
    c.load(
        vec![folder],
        Destination::NewLayer,
        false,
        "New Layer",
        None,
        Some(Box::new(move |r| {
            let _ = tx.send(r);
        })),
    )
    .unwrap()
    .join()
    .unwrap();
    assert_eq!(rx.recv().unwrap().unwrap_err(), ArenaError::Cancelled);
    let asked = seen.large.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].message, "Load 3 files?");
    assert_eq!(asked[0].informative, "\u{201C}F\u{201D}: 3 files (3 from 1 folder), skipped 1 not a type Arena opens. They fill consecutive columns.");
    assert_eq!(asked[0].buttons, ["Load 3 Files", "Cancel"]);
    assert_eq!(c.last_outcome().as_deref(), Some("Cancelled."));
    assert!(c.log().contains(|t| t == "Large folder (3 files) -> cancel"));
    assert!(c.log().contains(|t| t == "Cancelled - nothing loaded"));
    assert!(seen.errors.lock().unwrap().is_empty(), "no error dialog for a cancel");
    assert!(fake.lock().opened.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn failed_load_shows_the_error() {
    let port = closed_port();
    let (shared, dir) = shared_with(Some(&format!(r#"{{"updateMode":"onDemand","updateModeChosen":true,{}}}"#, endpoint_json(port))));
    let seen = Arc::new(Seen::default());
    let c = Controller::new(shared, hooks(&seen, Some(false), ConflictResolution::Cancel, true));
    let file = media(&dir, "a.mov");
    c.load(vec![file], Destination::NewLayer, false, "New Layer", Some(3..=3), None).unwrap().join().unwrap();
    let errors = seen.errors.lock().unwrap().clone();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "Send to Arena failed");
    assert_eq!(errors[0].informative, "Arena is not running - start Resolume Arena.");
    assert!(c.log().contains(|t| t == "Load failed: Arena is not running - start Resolume Arena."));
    assert!(c.log().contains(|t| t.ends_with(", menu showed Layer 3")));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn folders_as_new_layers_through_the_menu() {
    let fake = FakeArena::start(1, 2);
    let (shared, dir) = shared_with(Some(&format!(
        r#"{{"updateMode":"onDemand","updateModeChosen":true,"triggerAfterLoad":true,"triggerScope":"topLayerOnly",{}}}"#,
        endpoint_json(fake.port)
    )));
    let seen = Arc::new(Seen::default());
    let c = Controller::new(shared.clone(), hooks(&seen, Some(true), ConflictResolution::Cancel, true));
    c.start(ArenaUpdateMode::OnDemand);
    sleep(0.3);
    media(&dir, "Alpha/a1.mov");
    media(&dir, "Alpha/a2.mov");
    media(&dir, "Alpha/a3.mov");
    media(&dir, "Bravo/b1.mov");
    let loose = media(&dir, "loose.mov");
    let sel = vec![dir.join("Bravo").to_string_lossy().into_owned(), dir.join("Alpha").to_string_lossy().into_owned(), loose];
    let nodes = c.nodes(&sel);
    assert_eq!(nodes[0].title(), Some("2 folders + 1 file -> consecutive columns, sorted by Name \u{2191}"));
    let (action, label) = find_action(&nodes, "Folders as New Layers (2) - Layers 2\u{2013}3").unwrap();
    let MenuAction::Load { destination, .. } = &action else { panic!() };
    let expected = SendMenuModel::expected_layers(destination, c.snapshot().as_ref(), 2);
    assert_eq!(expected, Some(2..=3));
    c.perform(&action, &label, &sel).unwrap().join().unwrap();
    let f = fake.lock();
    assert_eq!(f.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Layer #", "Bravo", "Alpha"]);
    assert_eq!(f.columns, 3, "Alpha's three files need a third column");
    assert_eq!(f.connects, ["L3 C1"], "Top Layer Only");
    drop(f);
    assert_eq!(
        c.last_outcome().as_deref(),
        Some("Loaded 4/4 into 2 new layers, top first: 3 \u{201C}Alpha\u{201D} 3/3, 2 \u{201C}Bravo\u{201D} 1/1, triggered")
    );
    assert!(c.log().contains(|t| t == "Folders as New Layers: ignored 1 loose file(s) (not in a selected folder)"));
    assert!(c.recents().is_empty(), "new layers are not remembered");
    assert!(!shared.recents_path().exists());
    c.store().stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn settings_and_refresh_picks() {
    let fake = FakeArena::start(1, 1);
    let (shared, dir) = shared_with(Some(&chosen("onDemand", &fake)));
    let c = Controller::new(shared.clone(), ControllerHooks::default());
    c.start(ArenaUpdateMode::OnDemand);
    sleep(0.3);
    let f = c.store().fetches();
    assert!(c.perform::<&str>(&MenuAction::Refresh, "Refresh", &[]).is_none());
    sleep(0.3);
    assert_eq!(c.store().fetches(), f + 1);
    assert!(c.log().contains(|t| t == "Refresh"));
    c.perform::<&str>(&MenuAction::Setting(SettingChange::SortKey(SortKey::Size)), "Size", &[]);
    assert_eq!(shared.read_settings().sort_key, SortKey::Size);
    assert!(c.log().contains(|t| t == "Setting: Sort by Size"));
    c.perform::<&str>(&MenuAction::Setting(SettingChange::UpdateInterval(15)), "Every 15 s", &[]);
    sleep(0.2);
    assert_eq!(c.store().mode(), ArenaUpdateMode::Interval);
    assert_eq!(c.store().interval(), Duration::from_secs(15));
    assert!(c.store().activity().interval_timer);
    c.store().stop();
    let _ = std::fs::remove_dir_all(dir);
}
