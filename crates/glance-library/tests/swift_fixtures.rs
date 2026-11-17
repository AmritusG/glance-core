//! Rust against Swift MaCrate 0.16.4's own output (golden files written by `tools/swift-library-oracle`, see
//! `tests/fixtures/swift-0.16.4/README.md`): the same database, value for value and type for type, the same reads,
//! queries and searches, and the same JSON bytes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use glance_library::alpha_probe::AlphaProbe;
use glance_library::dump::{checks, dump_file};
use glance_library::filter::ClipFilter;
use glance_library::search::{filename_token, sanitize_search_query};
use glance_library::sidecar::{ClipMetadata, clip_path_from_sidecar, sidecar_path};
use glance_library::stash::{decode_stash_file, encode_stash_file};
use glance_library::store::Location;
use glance_library::unicode::{file_system_representation, trim_whitespaces_and_newlines};
use serde_json::{Value, json};
use std::path::Path;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        if e.file_type().unwrap().is_file() {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

fn assert_same_dump(ours: &Path, golden: &Path, mask: bool) {
    let mut a = dump_file(ours).unwrap();
    let mut b = read_json(golden);
    if mask {
        mask_volatile(&mut a);
        mask_volatile(&mut b);
    }
    let mut diffs = Vec::new();
    dump_diff(&a, &b, "", &mut diffs);
    assert!(diffs.is_empty(), "{} differs from {}:\n{}", ours.display(), golden.display(), diffs.join("\n"));
}

#[test]
fn rust_dumps_swift_files_as_swift_does() {
    for (db, dump) in [
        ("empty/library.sqlite", "empty/dump.json"),
        ("empty/measurements.sqlite", "empty/measurements-dump.json"),
        ("populated/library.sqlite", "populated/dump.json"),
        ("populated/measurements.sqlite", "populated/measurements-dump.json"),
        ("legacy-v7/library.sqlite", "legacy-v7/dump.json"),
        ("legacy-v7-upgraded/library.sqlite", "legacy-v7-upgraded/dump.json"),
    ] {
        assert_same_dump(&fixtures().join(db), &fixtures().join(dump), false);
    }
}

#[test]
fn rust_creates_the_empty_library_swift_creates() {
    let dir = TempDir::new("empty");
    let lib = dir.path().join("library.sqlite");
    drop(open(Location::File(lib.clone())));
    assert_same_dump(&lib, &fixtures().join("empty/dump.json"), false);
    assert_same_dump(&dir.path().join("measurements.sqlite"), &fixtures().join("empty/measurements-dump.json"), false);
    let report = open(Location::File(lib.clone()));
    assert!(report.open_report().migrations.applied.is_empty(), "a second open runs no migration");
    assert!(report.open_report().wrote_pre_measurements_backup, "the second open backs up before the first attach, as Swift");
    assert!(!report.open_report().wrote_pre_v8_backup, "v8 has run: no pre-v8 backup");
}

#[test]
fn rust_builds_the_populated_library_swift_builds() {
    let dir = TempDir::new("populated");
    let lib = dir.path().join("library.sqlite");
    {
        let mut store = open(Location::File(lib.clone()));
        build(&mut store);
    }
    assert_same_dump(&lib, &fixtures().join("populated/dump.json"), true);
    assert_same_dump(&dir.path().join("measurements.sqlite"), &fixtures().join("populated/measurements-dump.json"), false);
    let conn = rusqlite::Connection::open(&lib).unwrap();
    assert!(same(&checks(&conn).unwrap(), &read_json(&fixtures().join("populated/checks.json"))));
}

#[test]
fn reads_queries_and_searches_match_swift_on_both_files() {
    let golden_reads = read_json(&fixtures().join("populated/reads.json"));
    let golden_queries = read_json(&fixtures().join("populated/queries.json"));
    let golden_search = read_json(&fixtures().join("populated/search.json"));
    let dir = TempDir::new("reads");
    // The Swift-made file (a copy: opening may write the pre-measurements backup) and a Rust-made one.
    let swift_dir = dir.path().join("swift");
    copy_dir(&fixtures().join("populated"), &swift_dir);
    let rust_lib = dir.path().join("rust/library.sqlite");
    build(&mut open(Location::File(rust_lib.clone())));
    for lib in [swift_dir.join("library.sqlite"), rust_lib] {
        let store = open(Location::File(lib.clone()));
        assert!(store.open_report().migrations.applied.is_empty());
        assert_eq!(store.open_report().search_backfilled, None);
        let reads = reads(&store);
        assert!(same(&reads, &golden_reads), "{}: reads differ in {:?}", lib.display(), differing_keys(&reads, &golden_reads));
        let mut diffs = Vec::new();
        for case in golden_queries.as_array().unwrap() {
            diffs.extend(check_query_case(&store, case));
        }
        diffs.extend(check_search(&store, &golden_search));
        assert!(diffs.is_empty(), "{}:\n{}", lib.display(), diffs.join("\n"));
    }
    assert_eq!(golden_queries.as_array().unwrap().len(), 77);
}

#[test]
fn the_legacy_v7_upgrade_matches_swift() {
    let dir = TempDir::new("legacy");
    let lib = dir.path().join("library.sqlite");
    std::fs::copy(fixtures().join("legacy-v7/library.sqlite"), &lib).unwrap();
    let store = open(Location::File(lib.clone()));
    let report = store.open_report().clone();
    drop(store);
    let upgrade = read_json(&fixtures().join("legacy-v7-upgraded/upgrade.json"));
    assert_eq!(report.migrations.needs_full_reindex, upgrade["forceFullReindex"].as_bool().unwrap());
    assert_eq!(report.migrations.applied.len(), 5);
    assert_eq!(report.migrations.v8_duplicates_removed, 2);
    assert_eq!(report.search_backfilled, Some(5));
    assert!(report.wrote_pre_v8_backup && report.wrote_pre_measurements_backup);
    let original = std::fs::read(fixtures().join("legacy-v7/library.sqlite")).unwrap();
    assert_eq!(std::fs::read(dir.path().join("library.sqlite.pre-v8.bak")).unwrap(), original);
    assert_eq!(std::fs::read(dir.path().join("library.sqlite.pre-measurements.bak")).unwrap(), original);
    assert_same_dump(&lib, &fixtures().join("legacy-v7-upgraded/dump.json"), false);
}

#[test]
fn unknown_migration_identifiers_are_tolerated() {
    let dir = TempDir::new("newer");
    let lib = dir.path().join("library.sqlite");
    drop(open(Location::File(lib.clone())));
    rusqlite::Connection::open(&lib).unwrap().execute("INSERT INTO grdb_migrations VALUES ('v99_from_the_future')", []).unwrap();
    let store = open(Location::File(lib));
    assert!(store.open_report().migrations.applied.is_empty());
}

#[test]
fn sidecar_bytes_match_swift() {
    let json_dir = fixtures().join("json");
    let mut n = 0;
    for e in std::fs::read_dir(&json_dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with("sidecar-") || name.contains("decode") {
            continue;
        }
        let bytes = std::fs::read(e.path()).unwrap();
        let meta = ClipMetadata::decode(&bytes).unwrap();
        assert_eq!(String::from_utf8(meta.encode().unwrap()).unwrap(), String::from_utf8(bytes).unwrap(), "{name}");
        n += 1;
    }
    assert_eq!(n, 5);
    // The same values built in Rust give the same bytes (the cases of `Bytes.sidecars`).
    for (name, meta) in sidecar_cases() {
        let golden = std::fs::read(json_dir.join(format!("sidecar-{name}.json"))).unwrap();
        assert_eq!(meta.encode().unwrap(), golden, "{name}");
    }
    for case in read_json(&json_dir.join("sidecar-decode.json")).as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let decoded = ClipMetadata::decode(input.as_bytes());
        assert_eq!(decoded.is_ok(), case["ok"].as_bool().unwrap(), "{input}");
        if let Ok(m) = decoded {
            assert_eq!(String::from_utf8(m.encode().unwrap()).unwrap(), case["reencoded"].as_str().unwrap(), "{input}");
        }
    }
}

#[test]
fn stash_file_bytes_match_swift() {
    let json_dir = fixtures().join("json");
    let full = std::fs::read(json_dir.join("stash-full/.crate-stashes.json")).unwrap();
    let empty = std::fs::read(json_dir.join("stash-empty/.crate-stashes.json")).unwrap();
    assert_eq!(encode_stash_file(&stash_cases()).unwrap(), full);
    assert_eq!(encode_stash_file(&[]).unwrap(), empty);
    assert_eq!(decode_stash_file(&full).unwrap(), stash_cases());
    for case in read_json(&json_dir.join("stash-decode.json")).as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let got: Vec<Value> = decode_stash_file(input.as_bytes())
            .unwrap_or_default()
            .iter()
            .map(|s| {
                json!({"id": s.id, "name": s.name, "createdAt": s.created_at, "updatedAt": s.updated_at,
                       "clips": s.clips.iter().map(|e| json!([e.clip_id, e.relative_path_hint])).collect::<Vec<_>>()})
            })
            .collect();
        assert!(same(&json!(got), &case["stashes"]), "{input}");
    }
}

#[test]
fn filter_json_matches_swift() {
    let json_dir = fixtures().join("json");
    let cases = [
        ("empty", ClipFilter::empty()),
        ("everything", Fx::everything()),
        ("ranges", ClipFilter { bpm_range: Some((120.0, 130.0)), energy_range: Some((5, 10)), ..ClipFilter::empty() }),
        (
            "flags-false",
            ClipFilter {
                loops_only: Some(glance_library::filter::MeasuredFlag::new(false)),
                cycle_range: Some(glance_library::filter::MeasuredValue::new((2.5, 8.0), false)),
                ..ClipFilter::empty()
            },
        ),
        ("unicode", ClipFilter { tags: tags(&["caf\u{E9}", "a/b"]), mood: Some("d\"a/rk".into()), ..ClipFilter::empty() }),
    ];
    for (name, filter) in cases {
        let golden = std::fs::read(json_dir.join(format!("filter-{name}.json"))).unwrap();
        // Swift's key order varies from run to run: compare parsed, and same length.
        let ours = filter.to_json().unwrap();
        assert_eq!(ours.len(), golden.len(), "{name}");
        let a: Value = serde_json::from_str(&ours).unwrap();
        let mut b: Value = serde_json::from_slice(&golden).unwrap();
        if let Some(t) = b.get_mut("tags").and_then(Value::as_array_mut) {
            t.sort_by(|x, y| x.as_str().cmp(&y.as_str()));
        }
        assert!(same(&a, &b), "{name}: {a} != {b}");
        assert_eq!(ClipFilter::from_json(&golden), Some(filter), "{name}");
    }
    for case in read_json(&json_dir.join("filter-decode.json")).as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let got = ClipFilter::from_json(input.as_bytes()).map_or(Value::Null, |f| canon(&f));
        assert!(same(&got, &case["filter"]), "{input}: {got} != {}", case["filter"]);
    }
}

#[test]
fn small_rules_match_swift() {
    let rules = read_json(&fixtures().join("rules.json"));
    let pairs =
        |key: &str| -> Vec<(String, Value)> { rules[key].as_array().unwrap().iter().map(|p| (p[0].as_str().unwrap().to_string(), p[1].clone())).collect() };
    for (path, want) in pairs("sidecarURL") {
        assert_eq!(json!(sidecar_path(Path::new(&path)).to_string_lossy()), want, "{path}");
    }
    for (name, want) in pairs("clipURLFromSidecar") {
        let got = clip_path_from_sidecar(&Path::new("/r").join(&name)).map(|p| p.to_string_lossy().into_owned());
        assert_eq!(got.map_or(Value::Null, Value::from), want, "{name:?}");
    }
    for (path, want) in pairs("filenameToken") {
        assert_eq!(json!(filename_token(&path)), want, "{path:?}");
    }
    for (raw, want) in pairs("sanitize") {
        assert_eq!(json!(sanitize_search_query(&raw)), want, "{raw:?}");
    }
    for (raw, want) in pairs("trim") {
        assert_eq!(json!(trim_whitespaces_and_newlines(&raw)), want, "{raw:?}");
    }
    for (raw, want) in pairs("fsRepresentation") {
        assert_eq!(json!(file_system_representation(&raw)), want, "{raw:?}");
    }
    for (bits, want) in pairs("doubleDescription") {
        let v = f64::from_bits(u64::from_str_radix(&bits, 16).unwrap());
        assert_eq!(json!(glance_library::thumbkey::swift_double(v)), want, "{bits}");
    }
    for (raw, want) in pairs("alphaDecode") {
        assert_eq!(AlphaProbe::decode(&raw).map_or(Value::Null, |p| json!(p.encode())), want, "{raw:?}");
    }
    let encoded: Vec<String> = [
        AlphaProbe::None,
        AlphaProbe::Choosable { look: Some(glance_media::AlphaLook::LooksStraight), samples: 1234, share: 0.25 },
        AlphaProbe::Choosable { look: None, samples: 0, share: 0.0 },
        AlphaProbe::Choosable { look: Some(glance_media::AlphaLook::CantTell), samples: 7, share: 1.0 / 3.0 },
        AlphaProbe::Stated { alpha: glance_media::AlphaView::Straight, by: "HAP".into() },
        AlphaProbe::Stated { alpha: glance_media::AlphaView::Premultiplied, by: "a:b".into() },
    ]
    .iter()
    .map(AlphaProbe::encode)
    .collect();
    let want: Vec<Value> = rules["alphaEncode"].as_array().unwrap().iter().map(|p| p[1].clone()).collect();
    assert_eq!(json!(encoded), json!(want));
}
