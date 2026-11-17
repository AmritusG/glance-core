// SPDX-License-Identifier: MIT
//! REST calls against the fake Arena, and the transport failures mapped to the kit's texts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{FSlot, FakeArena, closed_port};
use glance_arena::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn product_status_snapshot_slot() {
    let fake = FakeArena::start(2, 3);
    fake.lock().layers[1].slots[2] = FSlot::loaded("clip.mov", "Connected");
    let c = fake.client(None);
    assert_eq!(c.product().unwrap().version_string(), "Arena 7.28.1 (26535)");
    assert_eq!(c.status(), ArenaStatus::Connected { product: "Arena 7.28.1 (26535)".into() });
    assert_eq!(c.status().plain_line(), "Connected to Arena 7.28.1 (26535)");
    let s = c.snapshot().unwrap();
    assert_eq!((s.composition_name.as_str(), s.deck_name.as_str()), ("Fake", "Main"));
    assert_eq!(s.layers.len(), 2);
    assert_eq!(s.columns.len(), 3);
    assert_eq!(s.layers[1].name_param_id, Some(502));
    assert_eq!(s.layers[1].slots[2].clip_name.as_deref(), Some("clip.mov"));
    assert!(s.layers[1].slots[2].is_playing());
    let slot = c.slot(2, 3).unwrap();
    assert_eq!(slot, SlotInfo::new(3, 1, Some("clip.mov".into()), "Connected"));
    let empty = c.slot(1, 1).unwrap();
    assert_eq!(empty.clip_name, None);
    assert!(!empty.is_occupied());
    // Writes reach the fake with Arena's paths and bodies.
    c.add_layer().unwrap();
    c.add_column().unwrap();
    c.open(1, 4, "/Volumes/My Clips/a#1 & 50%?.mov").unwrap();
    c.connect(1, 4).unwrap();
    c.rename_layer(500, "Bass / Kick").unwrap();
    let f = fake.lock();
    assert_eq!(f.layers.len(), 3);
    assert_eq!(f.columns, 4);
    assert_eq!(f.opened, ["file:///Volumes/My%20Clips/a%231%20%26%2050%25%3F.mov"]);
    assert_eq!(f.connects, ["L1 C4"]);
    assert_eq!(f.puts, [r#"{"name":{"value":"Bass \/ Kick"}}"#]);
    assert_eq!(f.layers[0].name, "Bass / Kick");
    assert!(f.requests.contains(&"POST /api/v1/composition/layers/1/clips/4/open".to_string()));
    assert!(f.requests.contains(&"PUT /api/v1/composition/layers/by-id/500".to_string()));
}

#[test]
fn http_status_texts() {
    let fake = FakeArena::start(1, 1);
    let c = fake.client(None);
    for (status, body, want) in [
        (404, "", "not found: /composition/layers/1/clips/1/connect"),
        (412, "busy", "Arena refused (composition locked or clip busy): /composition/layers/1/clips/1/connect - busy"),
        (400, "bad", "bad request: /composition/layers/1/clips/1/connect - bad"),
        (503, "", "HTTP 503: /composition/layers/1/clips/1/connect"),
    ] {
        fake.lock().override_answer = Some((status, body.into()));
        let e = c.connect(1, 1).unwrap_err();
        assert_eq!(e.plain_line(), want);
    }
    // The body is cut to its first 300 bytes.
    fake.lock().override_answer = Some((500, "x".repeat(1000)));
    let ArenaError::Http { body, .. } = c.snapshot().unwrap_err() else { panic!() };
    assert_eq!(body.len(), 300);
    // An answer that is not the expected JSON.
    fake.lock().override_answer = Some((200, "{\"name\":1}".into()));
    let ArenaError::BadResponse(d) = c.product().unwrap_err() else { panic!() };
    assert!(d.starts_with("product: "), "{d}");
    let ArenaError::BadResponse(d) = c.slot(1, 1).unwrap_err() else { panic!() };
    assert!(d.starts_with("clip: "), "{d}");
    assert_eq!(c.snapshot().unwrap_err(), ArenaError::BadResponse("composition JSON: expected '{' at byte 8".into()));
}

fn local(port: u16, running: Option<bool>) -> ArenaClient {
    ArenaClient::with_probe(ArenaEndpoint::new("127.0.0.1", i64::from(port)), Arc::new(move || running))
}

#[test]
fn refused_local_maps_by_process_state() {
    let port = closed_port();
    assert_eq!(local(port, Some(false)).product().unwrap_err(), ArenaError::NotRunning);
    assert_eq!(local(port, Some(false)).status().plain_line(), "Arena is not running - start Resolume Arena.");
    let off = ArenaError::WebserverOff { host: "127.0.0.1".into(), port: i64::from(port) };
    assert_eq!(local(port, Some(true)).product().unwrap_err(), off);
    assert_eq!(local(port, None).snapshot().unwrap_err(), off);
    assert_eq!(off.plain_line(), format!("Arena's webserver is off - Arena > Preferences > Webserver: enable, port {port} (127.0.0.1)."));
    // "localhost" is local too (the default endpoint's host).
    let lh = ArenaClient::with_probe(ArenaEndpoint::new("localhost", i64::from(port)), Arc::new(|| Some(false)));
    assert_eq!(lh.product().unwrap_err(), ArenaError::NotRunning);
}

#[test]
fn timeout_maps_to_timed_out() {
    let fake = FakeArena::start(1, 1);
    fake.lock().hang = true;
    let mut c = fake.client(Some(true));
    c.request_timeout = Duration::from_millis(300);
    let t0 = Instant::now();
    let e = c.product().unwrap_err();
    assert!(t0.elapsed() < Duration::from_secs(3));
    assert_eq!(e, ArenaError::Unreachable { host: "127.0.0.1".into(), port: i64::from(fake.port), detail: "timed out".into() });
    assert_eq!(e.plain_line(), format!("Arena not reachable at 127.0.0.1:{} - timed out", fake.port));
}

#[test]
fn unknown_host_maps_to_host_not_found() {
    // The reserved .invalid name never resolves (RFC 6761).
    let c = ArenaClient::new(ArenaEndpoint::new("arena.invalid", 8080));
    assert_eq!(c.product().unwrap_err().plain_line(), "Arena not reachable at arena.invalid:8080 - host not found");
}

#[test]
fn endpoint_defaults() {
    let e = ArenaEndpoint::default();
    assert_eq!((e.host.as_str(), e.port), ("localhost", 8080));
    assert!(e.is_local());
    assert_eq!(e.api_base(), "http://localhost:8080/api/v1");
    assert!(ArenaEndpoint::new("LOCALHOST", 1).is_local());
    assert!(ArenaEndpoint::new("::1", 1).is_local());
    assert_eq!(ArenaEndpoint::new("::1", 8080).api_base(), "http://[::1]:8080/api/v1");
    assert!(!ArenaEndpoint::new("10.0.0.5", 8080).is_local());
}
