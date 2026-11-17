// SPDX-License-Identifier: MIT
//! The composition store against the fake Arena: change de-duplication, the reconnect policy,
//! the three update modes, and websocket pushes (full compositions, renames, subscriptions).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{FSlot, FakeArena, WsMode, closed_port, sleep};
use glance_arena::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn store(client: ArenaClient, mode: ArenaUpdateMode, interval: f64) -> CompositionStore {
    CompositionStore::new(client, mode, Duration::from_secs_f64(interval))
}

fn flag_client(port: u16, flag: &Arc<AtomicBool>) -> ArenaClient {
    let f = Arc::clone(flag);
    ArenaClient::with_probe(ArenaEndpoint::new("127.0.0.1", i64::from(port)), Arc::new(move || Some(f.load(Ordering::SeqCst))))
}

fn wait_until(secs: f64, f: impl Fn() -> bool) -> bool {
    let t0 = Instant::now();
    while t0.elapsed().as_secs_f64() < secs {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    f()
}

/// A refresh that yields the same snapshot delivers nothing; a play-state change does.
#[test]
fn store_delivers_only_on_change() {
    let fake = FakeArena::start(1, 3);
    fake.lock().layers[0].slots[0] = FSlot::loaded("a.mov", "Disconnected");
    let s = store(fake.client(Some(true)), ArenaUpdateMode::Live, 60.0);
    s.refresh("t1");
    sleep(0.3);
    assert_eq!(s.deliveries(), 1);
    assert!(s.status().is_connected());
    for i in 2..=4 {
        s.refresh(format!("t{i}"));
        sleep(0.3);
    }
    assert_eq!(s.deliveries(), 1, "identical pushes must not reach onChange");
    fake.lock().layers[0].slots[0].state = "Connected".into();
    s.refresh("play");
    sleep(0.3);
    assert_eq!(s.deliveries(), 2, "a play-state change still reaches onChange");
    fake.lock().layers[0].name = "Bass".into();
    s.refresh("rename");
    sleep(0.3);
    assert_eq!(s.deliveries(), 3);
}

/// A closed local port with Arena "not running": one attempt, then silence until
/// arena_launched, which retries every startup_interval.
#[test]
fn store_waits_for_launch_then_retries_fast() {
    let running = Arc::new(AtomicBool::new(false));
    let s = store(flag_client(closed_port(), &running), ArenaUpdateMode::Live, 60.0);
    let log = EventLog::new();
    s.set_log(Some(log.clone()));
    s.set_policy(ReconnectPolicy { startup_interval: 0.2, ..ReconnectPolicy::default() });
    s.start();
    sleep(2.0);
    assert_eq!(s.attempts(), 1, "no retries while Arena is not running");
    assert_eq!(s.status(), ArenaStatus::Failed(ArenaError::NotRunning));
    assert!(log.contains(|t| t == "Waiting for Arena to launch (no retries until then)"));
    running.store(true, Ordering::SeqCst);
    s.arena_launched();
    sleep(1.5);
    assert!(s.attempts() >= 4, "startup window retries every 0.2 s, got {}", s.attempts());
    running.store(false, Ordering::SeqCst);
    s.arena_terminated();
    sleep(0.7);
    let settled = s.attempts();
    sleep(1.5);
    assert_eq!(s.attempts(), settled, "quiet again after Arena quit");
    s.wake();
    sleep(0.5);
    assert_eq!(s.attempts(), settled + 1, "a menu opening tries once");
    s.stop();
}

/// Arena running but unreachable (local webserver off): exponential backoff.
#[test]
fn store_backs_off_when_running_but_unreachable() {
    let running = Arc::new(AtomicBool::new(true));
    let s = store(flag_client(closed_port(), &running), ArenaUpdateMode::Live, 60.0);
    s.start();
    sleep(4.5); // attempts at ~0, 1, 3 s; the next is due at ~7 s
    assert_eq!(s.attempts(), 3);
    assert_eq!(s.status(), ArenaStatus::Failed(ArenaError::WebserverOff { host: "127.0.0.1".into(), port: s.endpoint().port }));
    s.stop();
}

#[test]
fn mode_switch_opens_and_closes_socket_and_timer() {
    let fake = FakeArena::start(1, 3); // websocket refused: Live shows up as connect attempts
    let s = store(fake.client(Some(true)), ArenaUpdateMode::Live, 60.0);
    s.start();
    sleep(0.3);
    assert!(s.attempts() >= 1, "live: websocket attempt");
    assert_eq!(s.fetches(), 0, "live: no REST fetch at start");

    s.set_mode(ArenaUpdateMode::OnDemand, Duration::from_secs(60));
    sleep(0.5);
    let a = s.attempts();
    assert_eq!(s.activity(), Activity { socket: false, interval_timer: false, poll: false });
    assert_eq!(s.fetches(), 1, "switching to on demand fetches once");
    assert!(s.status().is_connected());
    assert_eq!(s.snapshot().unwrap().layers.len(), 1);
    sleep(2.5); // live backoff would have retried at 1 and 2 s
    assert_eq!(s.attempts(), a, "on demand: no websocket retries");
    assert_eq!(s.fetches(), 1, "on demand: nothing without a menu");

    s.set_mode(ArenaUpdateMode::Interval, Duration::from_secs_f64(0.3));
    sleep(0.1);
    assert_eq!(s.activity(), Activity { socket: false, interval_timer: true, poll: false });
    sleep(1.3); // ticks due at 0.3, 0.6, 0.9, 1.2 s
    assert!(s.fetches() >= 4, "interval: one fetch at switch + ticks, got {}", s.fetches());

    s.set_mode(ArenaUpdateMode::Live, Duration::from_secs_f64(0.3));
    sleep(0.3);
    assert!(!s.activity().interval_timer, "live: timer stopped");
    let f = s.fetches();
    assert!(s.attempts() > a, "live: websocket attempted again");
    sleep(0.7);
    assert_eq!(s.fetches(), f, "live: no interval fetches");
    s.stop();
}

#[test]
fn wake_fetches_on_demand_throttled() {
    let fake = FakeArena::start(1, 3);
    let s = store(fake.client(Some(true)), ArenaUpdateMode::OnDemand, 60.0);
    s.set_menu_fetch_gap(Duration::from_secs_f64(0.5));
    s.start();
    sleep(0.3);
    assert_eq!(s.fetches(), 1);
    s.wake();
    sleep(0.2);
    assert_eq!(s.fetches(), 2, "menu open fetches");
    s.wake();
    sleep(0.1);
    assert_eq!(s.fetches(), 2, "a second open within menu_fetch_gap does not");
    s.refresh("Refresh");
    sleep(0.2);
    assert_eq!(s.fetches(), 3, "Refresh is not throttled");
    sleep(0.4);
    s.wake();
    sleep(0.2);
    assert_eq!(s.fetches(), 4);
    // The fetched composition reaches the store: a rename made in Arena shows after the next open.
    fake.lock().layers[0].name = "Bass".into();
    sleep(0.5);
    assert_eq!(s.snapshot().unwrap().layers[0].raw_name, "Layer #", "stale until a menu opens");
    s.wake();
    sleep(0.3);
    assert_eq!(s.snapshot().unwrap().layers[0].raw_name, "Bass");
    s.stop();
}

#[test]
fn interval_timing_and_quiet_while_arena_absent() {
    let fake = FakeArena::start(1, 3);
    let running = Arc::new(AtomicBool::new(true));
    let s = store(flag_client(fake.port, &running), ArenaUpdateMode::Interval, 0.25);
    s.start();
    sleep(1.15);
    let n = s.fetches();
    assert!((4..=6).contains(&n), "1 at start + 4 ticks in 1.15 s, got {n}");
    running.store(false, Ordering::SeqCst);
    sleep(0.3);
    let f = s.fetches();
    sleep(0.8);
    assert_eq!(s.fetches(), f, "local Arena not running: ticks do not fetch");
    assert_eq!(s.status(), ArenaStatus::Failed(ArenaError::NotRunning));
    assert!(s.snapshot().is_none());
    s.stop();
}

#[test]
fn updated_at_delivered_only_outside_live() {
    let fake = FakeArena::start(1, 3);
    let on_demand = store(fake.client(Some(true)), ArenaUpdateMode::OnDemand, 60.0);
    let stamps: Arc<Mutex<Vec<Option<Timestamp>>>> = Arc::default();
    let st = Arc::clone(&stamps);
    on_demand.set_on_change(Some(Arc::new(move |_, _, up| st.lock().unwrap().push(up))));
    on_demand.refresh("a");
    sleep(0.3);
    on_demand.refresh("b");
    sleep(0.3);
    assert_eq!(on_demand.deliveries(), 2, "same composition, new fetch time: the label must update");
    let got = stamps.lock().unwrap().clone();
    assert_eq!(got.len(), 2);
    assert_ne!(got[0], got[1]);

    let live = store(fake.client(Some(true)), ArenaUpdateMode::Live, 60.0);
    live.refresh("a");
    sleep(0.3);
    live.refresh("b");
    sleep(0.3);
    assert_eq!(live.deliveries(), 1, "live shows no time: identical compositions deliver nothing");
}

/// Arena just launched, webserver not up yet: a mode switch keeps retrying until it answers.
#[test]
fn switch_inside_startup_window_keeps_retrying() {
    let running = Arc::new(AtomicBool::new(true));
    let s = store(flag_client(closed_port(), &running), ArenaUpdateMode::OnDemand, 60.0);
    s.set_policy(ReconnectPolicy { startup_interval: 0.2, ..ReconnectPolicy::default() });
    s.start();
    sleep(0.3);
    s.arena_launched();
    sleep(0.5);
    let before = s.fetches();
    assert!(before >= 3, "startup retries every 0.2 s, got {before}");
    s.set_mode(ArenaUpdateMode::Interval, Duration::from_secs(60));
    sleep(0.8);
    assert!(s.fetches() >= before + 3, "still retrying after the switch");
    s.stop();
}

// MARK: websocket pushes

fn live_fake(layers: usize, columns: usize) -> FakeArena {
    let fake = FakeArena::start(layers, columns);
    fake.lock().ws_mode = WsMode::Accept;
    fake
}

#[test]
fn push_on_connect_subscribes_names_and_applies_renames() {
    let fake = live_fake(2, 3);
    let s = store(fake.client(Some(true)), ArenaUpdateMode::Live, 60.0);
    let log = EventLog::new();
    s.set_log(Some(log.clone()));
    s.start();
    assert!(wait_until(3.0, || s.snapshot().is_some_and(|x| x.layers.len() == 2)), "first push");
    assert!(wait_until(2.0, || s.status().is_connected()));
    assert_eq!(s.fetches(), 0, "the push carried the composition");
    assert!(log.contains(|t| t == "Composition (websocket): \"Fake\" deck Main, 2 layers, 3 columns"), "{}", log.text());
    // Every layer and column name parameter is subscribed once.
    let mut want: Vec<String> =
        [501, 502, 9500, 9501, 9502].iter().map(|id| format!("{{\"action\":\"subscribe\",\"parameter\":\"/parameter/by-id/{id}\"}}")).collect();
    want.sort();
    assert!(fake.wait_for(Duration::from_secs(2), |f| f.ws_received.len() >= 5));
    let mut got = fake.lock().ws_received.clone();
    got.sort();
    assert_eq!(got, want);

    // A rename arrives as a parameter update (no full composition).
    let d = s.deliveries();
    fake.push(r#"{"type":"parameter_update","id":502,"value":"Bass","path":"/composition/layers/2/name"}"#, 0);
    assert!(wait_until(2.0, || s.snapshot().unwrap().layers[1].raw_name == "Bass"));
    assert!(wait_until(1.0, || s.deliveries() == d + 1));
    assert!(log.contains(|t| t == "Renamed in Arena -> \"Bass\""));
    fake.push(r#"{"type":"parameter_subscribed","id":9500,"value":"Intro"}"#, 0);
    assert!(wait_until(2.0, || s.snapshot().unwrap().columns[0].raw_name == "Intro"));
    // Same name again, unknown ids, other types: nothing.
    let d = s.deliveries();
    fake.push(r#"{"type":"parameter_update","id":502,"value":"Bass"}"#, 0);
    fake.push(r#"{"type":"parameter_update","id":1,"value":"x"}"#, 0);
    fake.push(r#"{"type":"sources_update","value":"x"}"#, 0);
    sleep(0.3);
    assert_eq!(s.deliveries(), d);
    s.stop();
}

#[test]
fn full_pushes_fragmented_empty_ignored_and_reconnect() {
    let fake = live_fake(1, 2);
    let s = store(fake.client(Some(true)), ArenaUpdateMode::Live, 60.0);
    s.start();
    assert!(wait_until(3.0, || s.snapshot().is_some()));
    assert!(wait_until(2.0, || s.status().is_connected()));
    let d = s.deliveries();

    // A structural change: one more layer, pushed in 1000-byte fragments.
    {
        let mut f = fake.lock();
        f.layers.push(common::FLayer { id: 777, name: "New".into(), slots: vec![FSlot::empty(); 2] });
    }
    fake.push(&fake.composition_json(), 1000);
    assert!(wait_until(2.0, || s.snapshot().unwrap().layers.len() == 2));
    assert_eq!(s.deliveries(), d + 1);
    // The new layer's name is subscribed.
    assert!(fake.wait_for(Duration::from_secs(2), |f| f.ws_received.iter().any(|m| m.contains("by-id/778"))));

    // A quitting Arena's empty composition is ignored.
    fake.push(r#"{"name":{"value":""},"layers":[],"columns":[]}"#, 0);
    // A large non-composition message is skipped unread.
    fake.push(&format!(r#"{{"type":"thumbnail_update","data":"{}"}}"#, "x".repeat(70_000)), 0);
    // A ping is answered.
    fake.push_raw(&common::frame(0x9, true, b"hi"));
    assert!(fake.wait_for(Duration::from_secs(2), |f| f.ws_received.contains(&"PONG hi".to_string())));
    assert_eq!(s.snapshot().unwrap().layers.len(), 2);
    assert_eq!(s.deliveries(), d + 1);

    // The socket drops: the store reconnects (running Arena: backoff from 1 s) and the push on
    // connect brings the composition again.
    let a = s.attempts();
    fake.drop_sockets();
    assert!(wait_until(4.0, || s.attempts() == a + 1), "reconnect attempt");
    assert!(wait_until(2.0, || fake.open_sockets() == 1));
    s.stop();
    assert!(wait_until(2.0, || fake.open_sockets() == 0 || s.activity() == Activity::default()));
}

#[test]
fn poll_only_while_no_push_came() {
    let fake = live_fake(1, 1);
    let s = store(fake.client(Some(true)), ArenaUpdateMode::Live, 60.0);
    s.set_poll_interval(Duration::from_secs_f64(0.4));
    s.start();
    assert!(wait_until(3.0, || s.snapshot().is_some()));
    assert!(s.activity().poll);
    // Pushes every 0.1 s: the poll stays quiet.
    let comp = fake.composition_json();
    for _ in 0..10 {
        fake.push(&comp, 0);
        sleep(0.1);
    }
    assert_eq!(s.fetches(), 0, "pushes keep the poll quiet");
    // No pushes: the safety net fetches.
    assert!(wait_until(2.0, || s.fetches() >= 1), "poll after a quiet interval");
    s.stop();
}

#[test]
fn bad_composition_push_is_logged() {
    let fake = live_fake(1, 1);
    fake.lock().push_on_connect = false;
    let s = store(fake.client(Some(true)), ArenaUpdateMode::Live, 60.0);
    let log = EventLog::new();
    s.set_log(Some(log.clone()));
    s.start();
    assert!(wait_until(3.0, || fake.open_sockets() == 1));
    fake.push(r#"{"name":{"value":"x"}}"#, 0);
    assert!(
        wait_until(2.0, || log.contains(|t| t == "websocket composition decode failed: badResponse(\"composition JSON: no layers/columns at byte 22\")")),
        "{}",
        log.text()
    );
    s.stop();
}
