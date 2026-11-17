// SPDX-License-Identifier: MIT
//! Cross-test against SendEhrKit's own Swift code (`tools/swift-arena-oracle`): the kit's files
//! (settings, recents, requests, cache) encoded by Swift and by Rust and decoded by the other
//! side, plus composition decoding, file URLs, name order and settings decoding. Runs when
//! `GLANCE_ARENA_ORACLE` names the built oracle; skipped (and said so) otherwise.
//!
//! Byte equality is checked on Swift's `.sortedKeys` output: Swift's encoder writes object keys
//! in a per-process hash order, so its production bytes differ from run to run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use glance_arena::swift_json::{to_vec, to_vec_sorted};
use glance_arena::*;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

fn uuid_a() -> Uuid {
    Uuid::parse_str("0A1B2C3D-0000-4000-8000-00000000ABCD").unwrap()
}
fn uuid_b() -> Uuid {
    Uuid::parse_str("5E6F7A8B-1111-4222-9333-0123456789AB").unwrap()
}

fn snapshot() -> CompositionSnapshot {
    CompositionSnapshot::new(
        "Show / Night \"1\"",
        "Main",
        vec![
            LayerInfo::new(
                1,
                500,
                "Layer #",
                Some(501),
                vec![SlotInfo::new(1, 500_000, Some("Intro.mov".into()), "Connected"), SlotInfo::new(2, 500_001, None, "Empty")],
            ),
            LayerInfo::new(2, 9_007_199_254_740_993, "Bass \u{1F3B5}", None, vec![]),
        ],
        vec![ColumnInfo::new(1, 9000, "Column #", Some(9500)), ColumnInfo::new(2, 9001, "Drop", None)],
    )
}

type Check = Box<dyn Fn(&Path, &Path) -> Vec<String>>;

/// One sample: (name, Rust value) with its checks against the Swift files.
fn sample<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug + 'static>(name: &'static str, v: T) -> (&'static str, Check) {
    (
        name,
        Box::new(move |swift: &Path, rust: &Path| {
            let mut problems = Vec::new();
            // Rust writes its file for Swift to decode.
            std::fs::write(rust.join(format!("{name}.json")), to_vec(&v).unwrap()).unwrap();
            // 1. Same bytes as Swift with sorted keys.
            let swift_sorted = std::fs::read(swift.join(format!("{name}.sorted.json"))).unwrap();
            let rust_sorted = to_vec_sorted(&v).unwrap();
            if swift_sorted != rust_sorted {
                problems.push(format!(
                    "{name}: sorted bytes differ\n  swift {}\n  rust  {}",
                    String::from_utf8_lossy(&swift_sorted),
                    String::from_utf8_lossy(&rust_sorted)
                ));
            }
            // 2. Swift's production file decodes to the Rust sample.
            let swift_prod = std::fs::read(swift.join(format!("{name}.json"))).unwrap();
            match serde_json::from_slice::<T>(&swift_prod) {
                Ok(back) if back == v => {}
                Ok(back) => problems.push(format!("{name}: Swift's file decodes to {back:?}")),
                Err(e) => problems.push(format!("{name}: Swift's file does not decode: {e}")),
            }
            problems
        }),
    )
}

fn samples() -> Vec<(&'static str, Check)> {
    let date = Timestamp;
    vec![
        sample("settings-default", SendEhrSettings::default()),
        sample(
            "settings-full",
            SendEhrSettings {
                endpoint: ArenaEndpoint::new("10.0.0.5", 8081),
                remaps: vec![
                    PathRemap { id: uuid_a(), local_prefix: "/Volumes/Media \u{00D6}/clips".into(), remote_prefix: "D:/Clips & #1".into() },
                    PathRemap { id: uuid_b(), local_prefix: "/".into(), remote_prefix: "/Volumes/Other".into() },
                ],
                occupied_slot_mode: OccupiedSlotMode::ReplaceItem,
                trigger_after_load: true,
                trigger_scope: TriggerScope::TopLayerOnly,
                extra_empty_columns: 3,
                sort_key: SortKey::DateAdded,
                sort_ascending: false,
                include_subfolders: true,
                large_folder_threshold: 120,
                update_mode: ArenaUpdateMode::Interval,
                update_interval: 30,
                update_mode_chosen: true,
            },
        ),
        sample(
            "recents",
            vec![
                RecentTarget {
                    destination: Destination::Slot { layer_id: 1_234_567_890_123, column: 7 },
                    label: "Layer 3 \u{201C}Bass\u{201D} \u{203A} Column 7".into(),
                    date: date(1_763_380_000.123_456),
                },
                RecentTarget {
                    destination: Destination::NextFree { layer_id: 5 },
                    label: "Layer 1 \u{203A} Next Free Column".into(),
                    date: date(1_763_380_123.5),
                },
                RecentTarget { destination: Destination::AppendColumns { layer_id: -2 }, label: "x/y \"q\" \\ \t".into(), date: date(0.1) },
                RecentTarget { destination: Destination::NewLayer, label: "New Layer".into(), date: date(1e9) },
                RecentTarget { destination: Destination::NewNamedLayer { name: "Warm Up / Set".into() }, label: "Folder".into(), date: date(1_763_380_000.0) },
                RecentTarget { destination: Destination::LayerPerFolder, label: "Folders".into(), date: date(1_763_380_000.75) },
            ],
        ),
        sample(
            "request-load",
            LoadRequest {
                id: uuid_a(),
                paths: vec!["/Volumes/A B/c.mov".into(), "/x/\u{00FC}.mov".into()],
                destination: Destination::LayerPerFolder,
                replace_confirmed: true,
                label: "Folders as New Layers (2)".into(),
                created_at: date(1_763_380_000.25),
                setting: None,
                expected_layers: Some(24..=26),
            },
        ),
        sample(
            "request-setting",
            LoadRequest {
                id: uuid_b(),
                paths: vec![],
                destination: Destination::NewLayer,
                replace_confirmed: false,
                label: "Arena Updates every 15 s".into(),
                created_at: date(1_763_380_001.0),
                setting: Some(SettingChange::UpdateInterval(15)),
                expected_layers: None,
            },
        ),
        sample(
            "setting-changes",
            vec![
                SettingChange::SortKey(SortKey::DateModified),
                SettingChange::SortAscending(false),
                SettingChange::TriggerAfterLoad(true),
                SettingChange::TriggerScope(TriggerScope::TopLayerOnly),
                SettingChange::IncludeSubfolders(true),
                SettingChange::UpdateMode(ArenaUpdateMode::OnDemand),
                SettingChange::UpdateInterval(300),
            ],
        ),
        sample(
            "statuses",
            vec![
                ArenaStatus::Connected { product: "Arena 7.28.1 (26535)".into() },
                ArenaStatus::Unknown,
                ArenaStatus::Failed(ArenaError::NotRunning),
                ArenaStatus::Failed(ArenaError::WebserverOff { host: "localhost".into(), port: 8080 }),
                ArenaStatus::Failed(ArenaError::Unreachable { host: "10.0.0.5".into(), port: 8080, detail: "timed out".into() }),
                ArenaStatus::Failed(ArenaError::Http { status: 412, path: "/composition".into(), body: "busy".into() }),
                ArenaStatus::Failed(ArenaError::BadResponse("x".into())),
                ArenaStatus::Failed(ArenaError::LayerGone { layer_id: 77 }),
                ArenaStatus::Failed(ArenaError::NoFiles),
                ArenaStatus::Failed(ArenaError::Cancelled),
            ],
        ),
        sample(
            "cache",
            CacheState {
                status: ArenaStatus::Connected { product: "Arena 7.28.1 (26535)".into() },
                snapshot: Some(snapshot()),
                host_pid: 4242,
                written_at: date(1_763_380_000.5),
                updated_at: Some(date(1_763_379_990.0)),
            },
        ),
        sample("cache-empty", CacheState { status: ArenaStatus::Unknown, snapshot: None, host_pid: 1, written_at: date(1_763_380_000.0), updated_at: None }),
    ]
}

fn oracle() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("GLANCE_ARENA_ORACLE")?);
    p.exists().then_some(p)
}

fn run(oracle: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(oracle).args(args).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs an oracle command on a JSON input file; its JSON answer.
fn ask<T: DeserializeOwned>(oracle: &Path, dir: &Path, cmd: &str, input: &serde_json::Value) -> T {
    let f = dir.join(format!("{cmd}-input.json"));
    std::fs::write(&f, input.to_string()).unwrap();
    let (ok, out) = run(oracle, &[cmd, &f.to_string_lossy()]);
    assert!(ok, "{cmd}: {out}");
    serde_json::from_str(&out).unwrap()
}

#[test]
fn swift_oracle_cross_test() {
    let Some(oracle) = oracle() else {
        println!("SKIPPED: GLANCE_ARENA_ORACLE not set (build tools/swift-arena-oracle)");
        return;
    };
    let dir = common::temp_dir("oracle");
    let swift = dir.join("swift");
    let rust = dir.join("rust");
    std::fs::create_dir_all(&rust).unwrap();
    let mut problems: Vec<String> = Vec::new();

    // Files: Swift encodes; Rust compares bytes (sorted keys) and decodes Swift's files.
    let (ok, out) = run(&oracle, &["encode", &swift.to_string_lossy()]);
    assert!(ok, "{out}");
    let checks = samples();
    for (name, check) in &checks {
        let p = check(&swift, &rust);
        println!("files {name}: Swift sorted bytes == Rust sorted bytes, Swift file decodes to the Rust sample: {}", if p.is_empty() { "yes" } else { "NO" });
        problems.extend(p);
    }
    // Swift decodes Rust's files and re-encodes them (sorted): same bytes as Rust's.
    let (ok, out) = run(&oracle, &["decode", &rust.to_string_lossy()]);
    for line in out.lines() {
        println!("swift {line}");
    }
    if !ok {
        problems.push(format!("Swift could not decode every Rust file:\n{out}"));
    }
    for (name, _) in &checks {
        let swift_re = std::fs::read(rust.join(format!("{name}.swift-reencoded.sorted.json"))).unwrap_or_default();
        let swift_sorted = std::fs::read(swift.join(format!("{name}.sorted.json"))).unwrap();
        if swift_re != swift_sorted {
            problems.push(format!("{name}: Swift's re-encoding of the Rust file differs from Swift's own"));
        }
    }

    // Compositions: the decoder tests' inputs and the fake Arena's composition.
    let fake = common::FakeArena::start(2, 3);
    let compositions: Vec<String> = vec![
        r#"{"name":{"value":"Comp","id":1},"decks":[{"name":{"value":"A"},"selected":{"value":false}},{"name":{"value":"Main"},"selected":{"value":true}}],"layers":[{"id":5,"name":{"value":"Layer #","id":55},"clips":[{"id":7,"name":{"value":"Clip A"},"connected":{"value":"Connected"}},{"id":8,"name":{"value":""},"connected":{"value":"Empty"}}]}],"columns":[{"id":9,"name":{"value":"Column #","id":99}},{"id":10,"name":{"value":"Intro","id":100}}]}"#.into(),
        "{\"name\" : {\"valuetype\":\"ParamString\",\"id\":1,\"value\":\"Comp \\\"Q\\\" \\\\ \\/ é🎵 #\"},\n \"extra\":{\"layers\":[{\"id\":\"not this\"}],\"s\":\"]}[{\"},\n \"decks\":null,\n \"layers\":[ {\"clips\":[{\"name\":{\"value\":\"x\"},\"connected\":{\"value\":\"Disconnected\"},\"id\":-3,\"video\":{\"a\":[1.5e3,true,null]}}],\n \"name\":null,\"id\":42} ],\n \"columns\":[{\"name\":{\"value\":\"A\\nB\",\"id\":7},\"id\":8}]}".into(),
        r#"{"name":{"value":"\ud83c\udfb5 \ud800 \u00e9 \u0041"},"layers":[],"columns":[]}"#.into(),
        r#"{"name":{"value":"x"},"layers":[{"id":1"#.into(),
        r#"{"name":1}"#.into(),
        r#"{"layers":[],"columns":[{"name":{}}]}"#.into(),
        "{\"name\":\"x".into(),
        r#"{"name":{"value":"x"}}"#.into(),
        r#"{"layers":[{"id":99999999999999999999}],"columns":[]}"#.into(),
        r#"{"layers":[{"id":1.5,"clips":[{"id":2e3}]}],"columns":[]}"#.into(),
        r#"{"layers":[{"id":1,"clips":[{"name":{"value":"a"}}]}],"columns":[]}"#.into(),
        r#"{"layers":[{"name":{"value":"L"}}],"columns":[]}"#.into(),
        r#"[1,2]"#.into(),
        r#"{"layers":[],"columns":[] , "name" : {"value" : "late", "id" : -0}}"#.into(),
        r#"{"layers":[],"columns":[],"decks":[{"selected":{"value":true}},{"name":{"value":"B"},"selected":{"value":true}}]}"#.into(),
        fake.composition_json(),
    ];
    let swift_comp: Vec<String> = ask(&oracle, &dir, "composition", &serde_json::json!(compositions));
    let mut comp_same = 0;
    for (input, want) in compositions.iter().zip(&swift_comp) {
        let got = match glance_arena::decoder::decode(input.as_bytes()) {
            Ok(s) => format!("ok {}", String::from_utf8_lossy(&to_vec_sorted(&s).unwrap())),
            Err(e) => format!("error {}", e.plain_line()),
        };
        if &got == want {
            comp_same += 1;
        } else {
            problems.push(format!("composition {input:?}\n  swift {want}\n  rust  {got}"));
        }
    }
    println!("compositions: {comp_same}/{} decode identically (snapshot bytes or error text)", compositions.len());

    // File URLs.
    let paths: Vec<&str> = vec![
        "/Users/Resolume/My Videos/file 1.mov",
        "/a/#1 & 50%?.mov",
        "/Users/x/Mo\u{0308}tley Cru\u{0308}e/Bjo\u{0308}rk.mov",
        "/Users/x/M\u{00F6}tley Cr\u{00FC}e/Bj\u{00F6}rk.mov",
        "/Volumes/VJ Clips/Resolume Clips/日本 ñ (1).mov",
        "relative/clip.mov",
        "/a/b+c=d;e,f@g$h!i'j(k)l*m:n[o]p{q}r~s_t-u.v",
        "/emoji/\u{1F3B5}.mov",
        "/a/%41.mov",
        "/tab\there/new\nline.mov",
        "/Ångström/ﬁ.mov",
    ];
    let swift_urls: Vec<Vec<String>> = ask(&oracle, &dir, "fileurl", &serde_json::json!(paths));
    let mut url_same = 0;
    for (p, want) in paths.iter().zip(&swift_urls) {
        let enc = ArenaFileURL::encode(p);
        let dec = ArenaFileURL::decode(&enc).unwrap_or_else(|| "(nil)".into());
        if enc == want[0] && dec == want[1] {
            url_same += 1;
        } else {
            problems.push(format!("file URL {p:?}\n  swift {want:?}\n  rust  {:?}", [enc, dec]));
        }
    }
    println!("file URLs: {url_same}/{} encode and decode identically", paths.len());

    // Name order (localizedStandardCompare).
    let names = [
        "clip 2",
        "clip 10",
        "Clip 2",
        "été",
        "f",
        "a 1",
        "a1",
        "007",
        "7",
        "B two",
        "b",
        "x 9.mov",
        "x 10.mov",
        "none.mov",
        "a_b",
        "a-b",
        "ab",
        "a.b",
        "a b",
        "é",
        "e",
        "É",
        "ä",
        "Z",
        "a",
        "1",
        "_",
        "(1)",
        "a10b",
        "a9c",
        "file01",
        "file1",
        "mov",
        "png",
        "wav",
        "",
        "ß",
        "ss",
        "ø",
        "o",
        "日本",
        "Ab",
        "aB",
        "a01b",
        "1.5",
        "1.10",
        "x 1",
        "v2.0",
        "v10.0",
        "😀",
        "A",
        "aA",
        "02",
        "2",
        "0",
        "00",
        "ÉCOLE",
        "ecole",
        "Set",
        "A loose.mov",
        "#1",
        "$",
        "~",
        "&",
        "@home",
        "[a]",
        "{a}",
    ];
    let mut pairs = Vec::new();
    for a in names {
        for b in names {
            pairs.push(vec![a, b]);
        }
    }
    let swift_order: Vec<String> = ask(&oracle, &dir, "compare", &serde_json::json!(pairs));
    let mut order_same = 0;
    let mut order_diff = Vec::new();
    for (p, want) in pairs.iter().zip(&swift_order) {
        let got = match glance_arena::media::natural_compare(p[0], p[1]) {
            std::cmp::Ordering::Less => "<",
            std::cmp::Ordering::Greater => ">",
            std::cmp::Ordering::Equal => "=",
        };
        if got == want {
            order_same += 1;
        } else {
            order_diff.push(format!("{:?} {want} {:?} (rust {got})", p[0], p[1]));
        }
    }
    println!("name order: {order_same}/{} pairs agree with localizedStandardCompare", pairs.len());
    for d in order_diff.iter().take(40) {
        println!("  differs: {d}");
    }

    // Settings files with missing keys, wrong types, unknown keys.
    let settings_inputs = [
        "{}",
        r#"{"endpoint":{"host":"10.0.0.5","port":8081},"remaps":[],"occupiedSlotMode":"replaceItem","triggerAfterLoad":true,"extraEmptyColumns":3}"#,
        r#"{"triggerAfterLoad":true}"#,
        r#"{"sortKey":"name"}"#,
        r#"{"updateMode":"live","updateInterval":60}"#,
        r#"{"updateMode":"onDemand","updateInterval":60}"#,
        r#"{"updateMode":"interval","updateInterval":30}"#,
        r#"{"updateMode":"interval","updateInterval":0,"updateModeChosen":false}"#,
        r#"{"updateInterval":-5}"#,
        r#"{"endpoint":{"host":"x"}}"#,
        r#"{"sortKey":"color"}"#,
        r#"{"future":1,"sortAscending":false}"#,
        r#"{"remaps":[{"id":"0a1b2c3d-0000-4000-8000-00000000abcd","localPrefix":"/a","remotePrefix":"/b"}]}"#,
        r#"{"remaps":[{"id":"not-a-uuid","localPrefix":"/a","remotePrefix":"/b"}]}"#,
        r#"{"triggerAfterLoad":null,"sortKey":null}"#,
        r#"{"largeFolderThreshold":1.0}"#,
        r#"{"largeFolderThreshold":1.5}"#,
        "[]",
    ];
    let swift_settings: Vec<String> = ask(&oracle, &dir, "settings", &serde_json::json!(settings_inputs));
    let mut settings_same = 0;
    for (input, want) in settings_inputs.iter().zip(&swift_settings) {
        let got = match serde_json::from_str::<SendEhrSettings>(input) {
            Ok(s) => String::from_utf8_lossy(&to_vec_sorted(&s).unwrap()).into_owned(),
            Err(_) => "error".to_string(),
        };
        if &got == want {
            settings_same += 1;
        } else {
            problems.push(format!("settings {input}\n  swift {want}\n  rust  {got}"));
        }
    }
    println!("settings decoding: {settings_same}/{} inputs give the same settings (or both refuse)", settings_inputs.len());

    let _ = std::fs::remove_dir_all(&dir);
    for p in &problems {
        println!("PROBLEM {p}");
    }
    assert!(problems.is_empty(), "{} problems", problems.len());
    assert!(order_diff.is_empty(), "{} name-order pairs differ", order_diff.len());
}
