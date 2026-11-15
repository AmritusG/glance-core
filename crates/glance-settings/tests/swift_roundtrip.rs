//! Settings round-trip against Swift Glance's own settings code (`tools/swift-settings/harness.swift`,
//! copied from Swift Glance a9dc601), in the throw-away domain `com.amritus.glancecore.sample` —
//! never Swift Glance's real domain. Swift writes, Rust reads; Rust writes, Swift reads. macOS
//! only; skipped (and said so) where `xcrun swift` is missing.
#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use glance_media::AlphaView;
use glance_platform::Defaults;
use glance_settings::{ALPHA_MEMORY_CAPACITY, BackgroundMode, GlanceSettings, Value, keys, read_string_array};
use std::path::PathBuf;
use std::process::Command;

const DOMAIN: &str = "com.amritus.glancecore.sample";
const PROBES: [&str; 9] =
    ["Probe.pairs", "Probe.json", "Probe.int", "Probe.double", "Probe.float", "Probe.rustDouble", "Probe.rustInt", "Probe.rustData", "Probe.rustPairs"];

fn harness(mode: &str) -> Option<String> {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/swift-settings/harness.swift");
    let out = match Command::new("xcrun").args(["swift", &script.to_string_lossy(), mode, DOMAIN]).output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("SKIP: xcrun swift not available ({e})");
            return None;
        }
    };
    assert!(out.status.success(), "swift harness {mode}: {}", String::from_utf8_lossy(&out.stderr));
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn key(i: usize) -> String {
    format!("{i:064x}")
}

fn clear(d: &Defaults) {
    for k in [keys::BACKGROUND, keys::ALPHA_TYPE_BY_CLIP, keys::DEFAULT_PROMPT_SHOWN].iter().chain(PROBES.iter()) {
        d.remove(k);
    }
    d.flush();
}

#[test]
fn swift_glance_and_the_rust_store_read_each_others_settings() {
    let defaults = Defaults::for_domain(DOMAIN).unwrap();
    clear(&defaults);

    // 1. Swift Glance writes.
    if harness("write").is_none() {
        return;
    }
    let settings = GlanceSettings::new(Box::new(Defaults::for_domain(DOMAIN).unwrap()));
    assert_eq!(settings.background(), BackgroundMode::Checkerboard);
    assert!(settings.default_prompt_shown());
    let lines = read_string_array(settings.store(), keys::ALPHA_TYPE_BY_CLIP).unwrap();
    assert_eq!(lines.len(), ALPHA_MEMORY_CAPACITY, "Swift capped the list");
    assert_eq!(settings.alpha_choice(&key(0)), None, "dropped by Swift's cap");
    assert_eq!(settings.alpha_choice(&key(3)), Some(AlphaView::Premultiplied));
    assert_eq!(settings.alpha_choice(&key(4)), Some(AlphaView::Premultiplied), "changed its mind, newest line wins");
    assert_eq!(settings.alpha_choice(&key(6)), Some(AlphaView::Straight));
    assert_eq!(settings.alpha_choice(&key(2002)), Some(AlphaView::Straight));
    // The other apps' shapes come through typed.
    let store = settings.store();
    assert_eq!(
        store.get("Probe.pairs"),
        Some(Value::Array(vec![
            Value::Array(vec![Value::String(key(1)), Value::String("straight".into())]),
            Value::Array(vec![Value::String(key(2)), Value::String("premultiplied".into())])
        ]))
    );
    assert_eq!(store.get("Probe.json"), Some(Value::Data(br#"{"quality":0.6}"#.to_vec())));
    assert_eq!(store.get("Probe.int"), Some(Value::Int(64)));
    assert_eq!(store.get("Probe.double"), Some(Value::Real(180.0)));
    assert_eq!(store.get("Probe.float"), Some(Value::Real(0.5)));

    // 2. Rust writes; Swift Glance reads.
    settings.set_background(BackgroundMode::Black);
    settings.remember_alpha(&key(7), AlphaView::Straight);
    settings.remember_alpha(&key(9000), AlphaView::Straight);
    settings.remember_alpha(&key(9001), AlphaView::Premultiplied);
    settings.remember_alpha(&key(9000), AlphaView::Premultiplied);
    store.set("Probe.rustDouble", &Value::Real(0.75));
    store.set("Probe.rustInt", &Value::Int(-3));
    store.set("Probe.rustData", &Value::Data(vec![0, 0xab, 0xff]));
    store.set("Probe.rustPairs", &Value::Array(vec![Value::Array(vec![Value::String("a".into()), Value::String("b".into())])]));
    assert!(store.flush());
    let read = harness("read").unwrap();
    let json: serde_json::Value = serde_json::from_str(read.trim()).unwrap();
    eprintln!("Swift Glance read back: {json}");
    assert_eq!(json["background"], "black");
    assert_eq!(json["promptShown"], true);
    assert_eq!(json["alphaLines"], ALPHA_MEMORY_CAPACITY, "Rust kept the cap");
    let c = &json["choices"];
    assert_eq!(c["0"], "none");
    assert_eq!(c["1"], "none", "the 3 newest Rust lines pushed the oldest out");
    assert_eq!(c["4"], "premultiplied");
    assert_eq!(c["7"], "straight", "Rust changed clip 7 from premultiplied");
    assert_eq!(c["9000"], "premultiplied");
    assert_eq!(c["9001"], "premultiplied");
    assert_eq!(c["2002"], "straight");
    assert_eq!(json["probeDouble"], 0.75);
    assert_eq!(json["probeInt"], -3);
    assert_eq!(json["probeData"], "00abff");
    assert_eq!(json["probePairs"], serde_json::json!([["a", "b"]]));

    clear(store);
}
