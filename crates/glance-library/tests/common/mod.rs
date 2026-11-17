//! Shared by the cross-tests and the `write-fixtures` example: the synthetic library the Swift oracle builds
//! (`tools/swift-library-oracle/LibraryOracleTests.swift`, `Fx.build`), built step by step through the Rust API, and
//! the same reads and queries, in the same JSON shape.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_library::filter::{ClipFilter, ClipSort, Ground, MeasuredFlag, MeasuredSource, MeasuredValue, MotionState, TagLogic};
use glance_library::measurements::{self, ClipMeasurement};
use glance_library::query::{SqlArg, build_query_plan, build_query_plan_for_paths};
use glance_library::search::sanitize_search_query;
use glance_library::sidecar::ClipMetadata;
use glance_library::stash::{Stash, StashEntry};
use glance_library::store::{ClipCellMetadata, ClipRow, Location, MetadataStore};
use glance_media::AlphaView;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const ROOT: &str = "/Volumes/Fixture/Library";

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/swift-0.16.4")
}

/// A fresh directory under the system temp dir, removed on drop.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("glance-library-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

pub fn mounted() -> BTreeSet<String> {
    BTreeSet::from(["/Volumes/Fixture".to_string()])
}

pub fn id(c: char, size: i64) -> String {
    format!("{}:{size}", c.to_string().repeat(64))
}

pub struct Fx;

impl Fx {
    pub fn a() -> String {
        id('a', 1000)
    }
    pub fn b() -> String {
        id('b', 2000)
    }
    pub fn c() -> String {
        id('c', 3000)
    }
    pub fn d() -> String {
        id('d', 4000)
    }
    pub fn e() -> String {
        id('e', 5000)
    }
    pub fn f() -> String {
        id('f', 6000)
    }
    pub fn g() -> String {
        id('1', 7000)
    }
    pub fn r() -> String {
        id('2', 8000)
    }
    pub fn l() -> String {
        id('3', 9000)
    }
    pub fn m() -> String {
        id('5', 11000)
    }
    pub fn k(i: usize) -> String {
        id('4', 10 + i as i64)
    }
    pub const EDGE: [&'static str; 6] = [".hidden", "name.", "a.b.c.mov", "no_ext", "x.tar.gz", "sp ace.m ov"];

    pub fn p_a() -> String {
        format!("{ROOT}/blue_intro.mov")
    }
    pub fn p_b() -> String {
        format!("{ROOT}/Sub Dir/\u{DC}n\u{EF}c\u{F6}d\u{E9} caf\u{E9}.mov")
    }
    pub fn p_c() -> String {
        format!("{ROOT}/Sub Dir/U\u{308}ni\u{308}co\u{308}de\u{301} cafe\u{301}.mov")
    }
    pub fn p_d() -> String {
        format!("{ROOT}/odd name #%&?'\" .mov")
    }
    pub fn p_e1() -> String {
        format!("{ROOT}/dup/one.mov")
    }
    pub fn p_e2() -> String {
        "/Volumes/Fixture Two/Backup/one.mov".to_string()
    }
    pub fn p_g() -> String {
        format!("{ROOT}/gone-dismissed.mov")
    }
    pub fn p_f() -> String {
        format!("{ROOT}/gone.mov")
    }
    pub fn p_r_before() -> String {
        format!("{ROOT}/before rename.mov")
    }
    pub fn p_r() -> String {
        format!("{ROOT}/after rename.mov")
    }
    pub fn p_l() -> String {
        "/private/var/fixture/local/RedForm_4k_01.mov".to_string()
    }
    pub fn p_m() -> String {
        format!("{ROOT}/movement.MOV")
    }
    pub fn p_k(i: usize) -> String {
        format!("{ROOT}/edge/{}", Fx::EDGE[i])
    }

    pub const S1: &'static str = "8F2C0D6E-1B7A-4C3D-9E8F-0A1B2C3D4E5F";
    pub const S2: &'static str = "0A1B2C3D-4E5F-4A6B-8C7D-9E0F1A2B3C4D";
    pub const S3: &'static str = "FFFFFFFF-0000-4000-8000-000000000000";

    pub fn all_paths() -> Vec<String> {
        let mut v = vec![Fx::p_a(), Fx::p_b(), Fx::p_c(), Fx::p_d(), Fx::p_e1(), Fx::p_e2(), Fx::p_g(), Fx::p_f(), Fx::p_r(), Fx::p_l(), Fx::p_m()];
        v.extend((0..Fx::EDGE.len()).map(Fx::p_k));
        v
    }

    pub fn all_ids() -> Vec<String> {
        let mut v = vec![Fx::a(), Fx::b(), Fx::c(), Fx::d(), Fx::e(), Fx::f(), Fx::g(), Fx::r(), Fx::l(), Fx::m()];
        v.extend((0..Fx::EDGE.len()).map(Fx::k));
        v
    }

    pub fn everything() -> ClipFilter {
        ClipFilter {
            rating_min: Some(2),
            tags: tags(&["blue", "intro"]),
            tags_logic: TagLogic::Or,
            bpm_range: Some((100.5, 130.0)),
            energy_range: Some((3, 7)),
            mood: Some("dark".into()),
            genre: Some("techno".into()),
            has_duplicates: true,
            loops_only: Some(MeasuredFlag::default()),
            cycle_range: Some(MeasuredValue::of((2.0, 8.5))),
            exact_cycles_only: Some(MeasuredFlag::new(false)),
            join_step_max: Some(MeasuredValue::of(2.5)),
            no_hard_cuts: Some(MeasuredFlag::default()),
            no_flashes: Some(MeasuredFlag::new(false)),
            moves_state: Some(MeasuredValue::new(MotionState::Moves, false)),
            no_text: Some(MeasuredFlag::default()),
            ground: Some(MeasuredValue::of(Ground::OnBlack)),
            centre_heavy: Some(MeasuredFlag::default()),
            measured_by: Some(MeasuredSource::Lab),
        }
    }
}

pub fn tags(t: &[&str]) -> BTreeSet<String> {
    t.iter().map(|s| s.to_string()).collect()
}

pub fn strs(t: &[&str]) -> Vec<String> {
    t.iter().map(|s| s.to_string()).collect()
}

pub struct RowSpec {
    pub row: ClipRow,
}

pub fn row(id: &str, path: &str, size: i64) -> ClipRow {
    ClipRow::new(id, path, size, 1_700_000_000.0, 1_700_000_000.0, 1_700_000_000.0)
}

pub fn measurement(id: &str, key: &str, status: &str) -> ClipMeasurement {
    ClipMeasurement::new(id, key, status)
}

/// Open a store with the fixture's mounted volume and a fixed clock (a non-integral value, so a REAL stamp takes
/// the same 8 bytes on disk as Swift's `Date()` stamps).
pub fn open(location: Location) -> MetadataStore {
    let mut store = MetadataStore::open_with(location, Box::new(mounted)).unwrap();
    store.set_clock(Box::new(|| 1_800_000_000.123_456));
    store
}

/// `Fx.build`, step by step.
pub fn build(store: &mut MetadataStore) {
    let mut a = row(&Fx::a(), &Fx::p_a(), 1000);
    a.file_mtime = 1_731_500_000.25;
    a.rating = 5;
    a.bpm = Some(128.5);
    a.mood = Some("dark".into());
    a.genre = Some("techno".into());
    a.energy = Some(7);
    a.brief = Some("Path-like brief a/b \"quoted\"".into());
    a.duration_sec = Some(12.5);
    a.frame_rate = Some(29.97);
    a.video_width = Some(1920);
    a.video_height = Some(1080);
    a.created_at = 1_731_600_000.123;
    a.updated_at = 1_731_600_500.5;
    store.upsert(&a, &strs(&["blue", "intro", "loop"])).unwrap();
    store.upsert(&row(&Fx::b(), &Fx::p_b(), 2000), &strs(&["caf\u{E9}"])).unwrap();
    store.upsert(&row(&Fx::c(), &Fx::p_c(), 3000), &strs(&["cafe\u{301}"])).unwrap();
    let mut d = row(&Fx::d(), &Fx::p_d(), 4000);
    d.rating = 1;
    d.energy = Some(1);
    d.brief = Some("   ".into());
    d.frame_rate = Some(0.0);
    d.video_width = Some(0);
    d.video_height = Some(0);
    store.upsert(&d, &strs(&["odd"])).unwrap();
    let mut e1 = row(&Fx::e(), &Fx::p_e1(), 5000);
    e1.created_at = 1_700_000_050.0;
    e1.updated_at = 1_700_000_060.0;
    store.upsert(&e1, &strs(&["dup", "local"])).unwrap();
    let mut e2 = row(&Fx::e(), &Fx::p_e2(), 5000);
    e2.mood = Some("calm".into());
    e2.genre = Some("ambient".into());
    store.upsert(&e2, &strs(&["dup"])).unwrap();
    let mut g = row(&Fx::g(), &Fx::p_g(), 7000);
    g.is_orphan = true;
    store.upsert(&g, &strs(&["gone"])).unwrap();
    store.dismiss_all_orphans().unwrap();
    let mut f = row(&Fx::f(), &Fx::p_f(), 6000);
    f.is_orphan = true;
    store.upsert(&f, &[]).unwrap();
    store.upsert(&row(&Fx::r(), &Fx::p_r_before(), 8000), &strs(&["renamed", "zeta"])).unwrap();
    store.set_path(&Fx::p_r_before(), &Fx::p_r()).unwrap();
    let mut l = row(&Fx::l(), &Fx::p_l(), 9000);
    l.rating = 3;
    l.bpm = Some(100.0);
    l.mood = Some("bright".into());
    l.genre = Some("house".into());
    l.energy = Some(3);
    l.duration_sec = Some(5.0);
    store.upsert(&l, &[]).unwrap();
    for i in 0..Fx::EDGE.len() {
        let mut k = row(&Fx::k(i), &Fx::p_k(i), 10 + i as i64);
        k.created_at = 1_700_000_100.0 + i as f64;
        store.upsert(&k, &[]).unwrap();
    }
    let mut m = row(&Fx::m(), &Fx::p_m(), 11000);
    m.bpm = Some(140.0);
    m.energy = Some(10);
    m.duration_sec = Some(60.25);
    store.upsert(&m, &strs(&["and", "rock"])).unwrap();
    store.set_duration(&Fx::k(0), Some(3.5)).unwrap();
    store.set_frame_rate(&Fx::k(1), 25.0).unwrap();
    store.set_video_size(&Fx::k(2), 640, 360).unwrap();

    store.save_preset("Empty", &ClipFilter::empty(), None).unwrap();
    store.save_preset("  Everything  ", &Fx::everything(), Some(ClipSort::CycleDesc)).unwrap();
    store.save_preset("Empty", &ClipFilter { rating_min: Some(2), ..ClipFilter::empty() }, Some(ClipSort::RatingDesc)).unwrap();
    store.save_preset("Doomed", &ClipFilter::empty(), Some(ClipSort::Random)).unwrap();
    store.rename_preset("Doomed", "Renamed").unwrap();
    store.save_preset("Gone", &ClipFilter::empty(), None).unwrap();
    store.delete_preset("Gone").unwrap();

    let entry = |clip_id: String, hint: &str| StashEntry { clip_id, relative_path_hint: hint.to_string() };
    store
        .upsert_stash(&Stash {
            id: Fx::S1.into(),
            name: "Set A".into(),
            created_at: 1_731_600_000.123,
            updated_at: 1_731_600_500.5,
            clips: vec![entry(Fx::a(), "blue_intro.mov"), entry(Fx::d(), "odd name #%&?'\" .mov"), entry(Fx::e(), "dup/one.mov")],
        })
        .unwrap();
    store
        .upsert_stash(&Stash { id: Fx::S2.into(), name: "Empty Stash".into(), created_at: 1_700_000_000.0, updated_at: 1_700_000_000.0, clips: vec![] })
        .unwrap();
    store
        .upsert_stash(&Stash { id: Fx::S3.into(), name: "Tmp".into(), created_at: 1.0, updated_at: 2.0, clips: vec![entry(Fx::a(), "blue_intro.mov")] })
        .unwrap();
    store.delete_stash(Fx::S3).unwrap();
    store.rename_stash(Fx::S1, "Set A (renamed)", 1_731_601_000.75).unwrap();

    store.set_alpha_type(&Fx::a(), AlphaView::Straight).unwrap();
    store.set_alpha_probe(&Fx::a(), "choosable:looksStraight:1234:0.25").unwrap();
    store.set_alpha_probe(&Fx::b(), "stated:straight:HAP").unwrap();
    store.set_alpha_probe(&Fx::c(), "none").unwrap();
    store.set_alpha_type(&Fx::d(), AlphaView::Premultiplied).unwrap();

    let mut native = measurement(&Fx::a(), "native-a", "measured");
    native.lab_reason = Some("loops cleanly".into());
    native.pass1_score = Some(0.42);
    native.is_loop = Some(1);
    native.cycle_s = Some(4.0);
    native.cycle_rule = Some("whole file loops".into());
    native.approx_cycle = Some(0);
    native.join_step = Some(1.25);
    native.cuts = Some(0);
    native.flashes = Some(0);
    native.flicker = Some(0.012);
    native.flow_mean = Some(0.5);
    native.moves = Some(1);
    native.text_checked = 1;
    native.text_found = Some(0);
    native.black_share = Some(0.9);
    native.centre_share = Some(0.45);
    native.xfade_score = Some(0.1);
    native.measure_version = Some(measurements::SCANNER_MEASURE_VERSION.into());
    native.source = Some(measurements::SCANNER_SOURCE.into());
    native.measured_at = Some("2026-11-14T16:37:09Z".into());
    let wrote1 = store.insert_native_measurement(&native).unwrap();
    let mut again = native.clone();
    again.cuts = Some(9);
    let wrote2 = store.insert_native_measurement(&again).unwrap();
    let mut skip = measurement(&Fx::d(), "native-d", measurements::UNSUPPORTED_STATUS);
    skip.lab_reason = Some("unsupported-native: png".into());
    skip.measure_version = Some(measurements::SCANNER_MEASURE_VERSION.into());
    skip.measured_at = Some("2026-11-14T16:40:00Z".into());
    let wrote3 = store.insert_native_measurement(&skip).unwrap();
    assert_eq!([wrote1, wrote2, wrote3], [true, false, false]);
    let mut lab_e = measurement(&Fx::e(), "lab-e", "measured");
    lab_e.lab_reason = Some("why e".into());
    lab_e.pass1_score = Some(1.9);
    lab_e.is_loop = Some(1);
    lab_e.cycle_s = Some(12.0);
    lab_e.approx_cycle = Some(1);
    lab_e.join_step = Some(3.0);
    lab_e.cuts = Some(1);
    lab_e.flashes = Some(2);
    lab_e.black_share = Some(0.1);
    lab_e.centre_share = Some(0.2);
    lab_e.moves = Some(0);
    lab_e.measure_version = Some("ltx-lab screen (wave 98 rules)".into());
    lab_e.source = Some("lab wave 98".into());
    lab_e.measured_at = Some("2026-11-01T10:00:00Z".into());
    let mut lab_l = measurement(&Fx::l(), "lab-l", "not a loop");
    lab_l.lab_reason = Some("why l".into());
    lab_l.is_loop = Some(0);
    store.insert_lab_measurements(&[lab_e, lab_l]).unwrap();
}

// MARK: - JSON shapes (as the Swift oracle writes them)

fn opt<T: Into<Value>>(v: Option<T>) -> Value {
    v.map_or(Value::Null, Into::into)
}

pub fn clip_row_json(r: &ClipRow) -> Value {
    json!({
        "clipID": r.clip_id, "currentPath": r.current_path, "fileSize": r.file_size, "fileMtime": r.file_mtime,
        "rating": r.rating, "bpm": opt(r.bpm), "mood": opt(r.mood.clone()), "genre": opt(r.genre.clone()),
        "energy": opt(r.energy), "brief": opt(r.brief.clone()), "durationSec": opt(r.duration_sec),
        "frameRate": opt(r.frame_rate), "videoWidth": opt(r.video_width), "videoHeight": opt(r.video_height),
        "createdAt": r.created_at,
        "updatedAt": if r.current_path == Fx::p_r() { json!("volatile") } else { json!(r.updated_at) },
        "isOrphan": r.is_orphan,
    })
}

pub fn measurement_json(m: &Option<ClipMeasurement>) -> Value {
    let Some(m) = m else { return Value::Null };
    json!({
        "clip_id": m.clip_id, "lab_key": m.lab_key, "lab_status": m.lab_status, "lab_reason": opt(m.lab_reason.clone()),
        "pass1_score": opt(m.pass1_score), "is_loop": opt(m.is_loop), "cycle_s": opt(m.cycle_s), "cycle_rule": opt(m.cycle_rule.clone()),
        "approx_cycle": opt(m.approx_cycle), "join_step": opt(m.join_step), "cuts": opt(m.cuts), "flashes": opt(m.flashes),
        "flicker": opt(m.flicker), "flow_mean": opt(m.flow_mean), "moves": opt(m.moves), "text_checked": m.text_checked,
        "text_found": opt(m.text_found), "black_share": opt(m.black_share), "centre_share": opt(m.centre_share),
        "xfade_score": opt(m.xfade_score), "measure_version": opt(m.measure_version.clone()), "source": opt(m.source.clone()),
        "measured_at": opt(m.measured_at.clone()), "provenance": m.provenance,
    })
}

pub fn canon(f: &ClipFilter) -> Value {
    let flag = |x: &Option<MeasuredFlag>| x.map_or(Value::Null, |f| json!({"includeNotMeasured": f.include_not_measured}));
    json!({
        "ratingMin": opt(f.rating_min), "tags": f.tags.iter().collect::<Vec<_>>(),
        "tagsLogic": if f.tags_logic == TagLogic::And { "and" } else { "or" },
        "bpmRange": f.bpm_range.map_or(Value::Null, |(a, b)| json!([a, b])),
        "energyRange": f.energy_range.map_or(Value::Null, |(a, b)| json!([a, b])),
        "mood": opt(f.mood.clone()), "genre": opt(f.genre.clone()), "hasDuplicates": f.has_duplicates,
        "loopsOnly": flag(&f.loops_only), "exactCyclesOnly": flag(&f.exact_cycles_only), "noHardCuts": flag(&f.no_hard_cuts),
        "noFlashes": flag(&f.no_flashes), "noText": flag(&f.no_text), "centreHeavy": flag(&f.centre_heavy),
        "cycleRange": f.cycle_range.map_or(Value::Null, |v| json!({"value": [v.value.0, v.value.1], "includeNotMeasured": v.include_not_measured})),
        "joinStepMax": f.join_step_max.map_or(Value::Null, |v| json!({"value": v.value, "includeNotMeasured": v.include_not_measured})),
        "movesState": f.moves_state.map_or(Value::Null, |v| json!({"value": v.value.raw_value(), "includeNotMeasured": v.include_not_measured})),
        "ground": f.ground.map_or(Value::Null, |v| json!({"value": v.value.raw_value(), "includeNotMeasured": v.include_not_measured})),
        "measuredBy": f.measured_by.map_or(Value::Null, |m| json!(m.raw_value())),
    })
}

fn cell_json(cells: &BTreeMap<String, ClipCellMetadata>) -> Value {
    let mut m = Map::new();
    for (p, c) in cells {
        m.insert(
            p.clone(),
            json!({
                "clipID": opt(c.clip_id.clone()), "durationSec": opt(c.duration_sec), "frameRate": opt(c.frame_rate),
                "videoWidth": opt(c.video_width), "videoHeight": opt(c.video_height), "rating": opt(c.rating), "bpm": opt(c.bpm),
                "energy": opt(c.energy), "mood": opt(c.mood.clone()), "genre": opt(c.genre.clone()), "brief": opt(c.brief.clone()),
                "tags": opt(c.tags.clone()),
            }),
        );
    }
    Value::Object(m)
}

/// `Reads.all`.
pub fn reads(store: &MetadataStore) -> Value {
    let mut out = Map::new();
    out.insert("fetchAll".into(), store.fetch_all().unwrap().iter().map(clip_row_json).collect());
    out.insert("fetchAllUnderRoot".into(), json!(store.fetch_all_under(ROOT).unwrap().iter().map(|r| r.current_path.clone()).collect::<Vec<_>>()));
    let mut by_path = Vec::new();
    for p in Fx::all_paths().into_iter().chain(["/Volumes/Fixture/Library/missing.mov".to_string()]) {
        by_path.push(json!([p, {
            "tags": store.tags_for_path(&p).unwrap(),
            "fetchAt": store.fetch_at(&p).unwrap().as_ref().map_or(Value::Null, clip_row_json),
            "fetchByPath": opt(store.fetch_by_path(&p).unwrap().map(|r| r.clip_id)),
            "alphaType": opt(store.alpha_type_for_path(&p).unwrap().map(|a| a.stored_name())),
        }]));
    }
    out.insert("byPath".into(), Value::Array(by_path));
    let mut by_id = Map::new();
    for i in Fx::all_ids().into_iter().chain([id('9', 1)]) {
        let (alpha, probe) = store.alpha_info(&i).unwrap();
        by_id.insert(
            i.clone(),
            json!({
                "tags": store.tags_for_clip_id(&i).unwrap(),
                "currentPaths": store.current_paths(&i).unwrap(),
                "alphaType": opt(alpha.map(|a| a.stored_name())), "probe": opt(probe),
                "measurement": measurement_json(&store.measurement(&i).unwrap()),
                "rowsForClipID": store.fetch_rows_for_clip_id(&i).unwrap().iter().map(|r| r.current_path.clone()).collect::<Vec<_>>(),
                "fetchClipID": opt(store.fetch_by_clip_id(&i).unwrap().map(|r| r.current_path)),
            }),
        );
    }
    out.insert("byID".into(), Value::Object(by_id));
    let all_tags = store.all_tags().unwrap();
    out.insert("allTags".into(), json!(all_tags));
    out.insert(
        "clipIDsWithTag".into(),
        all_tags.iter().cloned().chain(["nope".to_string()]).map(|t| json!([t, store.clip_ids_with_tag(&t).unwrap()])).collect(),
    );
    out.insert("allMoods".into(), json!(store.all_moods().unwrap()));
    out.insert("allGenres".into(), json!(store.all_genres().unwrap()));
    out.insert("clipIDsWithDuplicates".into(), json!(store.clip_ids_with_duplicates().unwrap()));
    out.insert("countOrphans".into(), json!(store.count_orphans().unwrap()));
    out.insert("countDismissedOrphans".into(), json!(store.count_dismissed_orphans().unwrap()));
    out.insert("fetchOrphans".into(), store.fetch_orphans().unwrap().iter().map(|r| json!(r.current_path)).collect());
    out.insert("fetchDismissedOrphans".into(), store.fetch_dismissed_orphans().unwrap().iter().map(|r| json!(r.current_path)).collect());
    let mut under = Map::new();
    for p in [ROOT, "/Volumes/Fixture", "/Volumes/Fixture Two", "/Volumes/Fix", "/private/var/fixture", &format!("{ROOT}/edge"), "/nowhere"] {
        under.insert(
            p.to_string(),
            json!({
                "totalFileSize": store.total_file_size(p).unwrap(),
                "hasAnyRow": store.has_any_row(p).unwrap(),
                "unbriefedCount": store.unbriefed_distinct_count_under(p).unwrap(),
                "unbriefedEntries": store.unbriefed_distinct_entries_under(p).unwrap().iter().map(|e| json!([e.clip_id, e.sample_path])).collect::<Vec<_>>(),
            }),
        );
    }
    out.insert("underPrefix".into(), Value::Object(under));
    out.insert("unbriefedCountIn".into(), json!(store.unbriefed_distinct_count_in(&Fx::all_paths()).unwrap()));
    out.insert(
        "unbriefedEntriesIn".into(),
        store.unbriefed_distinct_entries_in(&Fx::all_paths()).unwrap().iter().map(|e| json!([e.clip_id, e.sample_path])).collect(),
    );
    out.insert(
        "presets".into(),
        store
            .list_presets()
            .unwrap()
            .iter()
            .map(|p| json!({"name": p.name, "sort": opt(p.sort.map(|s| s.raw_value())), "createdAt": "volatile", "filter": canon(&p.filter)}))
            .collect(),
    );
    out.insert(
        "stashes".into(),
        store
            .list_stashes()
            .unwrap()
            .iter()
            .map(|s| {
                json!({"id": s.id, "name": s.name, "createdAt": s.created_at, "updatedAt": s.updated_at,
                       "clips": s.clips.iter().map(|e| json!([e.clip_id, e.relative_path_hint])).collect::<Vec<_>>()})
            })
            .collect(),
    );
    out.insert("unmeasuredClips".into(), store.unmeasured_clips().unwrap().iter().map(|i| json!([i.path, i.clip_id])).collect());
    out.insert("unmeasuredClipsIn".into(), store.unmeasured_clips_at(&Fx::all_paths()).unwrap().iter().map(|i| json!([i.path, i.clip_id])).collect());
    let no_nfd: Vec<String> = Fx::all_paths().into_iter().filter(|p| *p != Fx::p_c()).collect();
    let mut with_nowhere = no_nfd.clone();
    with_nowhere.push("/nowhere.mov".into());
    out.insert("cellMetadata".into(), cell_json(&store.cell_metadata_by_path(&with_nowhere).unwrap()));
    out.insert("cellMetadataNFD".into(), cell_json(&store.cell_metadata_by_path(&[Fx::p_c()]).unwrap()));
    out.insert("tagsByPath".into(), json!(store.tags_by_path(&no_nfd).unwrap()));
    out.insert("tagsByPathNFD".into(), json!(store.tags_by_path(&[Fx::p_c()]).unwrap()));
    out.insert("tagsByClipID".into(), json!(store.tags_by_clip_id(&Fx::all_ids()).unwrap()));
    out.insert("fetchDuplicates".into(), store.fetch_duplicates(&Fx::e(), &Fx::p_e1()).unwrap().iter().map(|r| json!(r.current_path)).collect());
    out.insert("fetchRowsClipIDs".into(), store.fetch_rows_by_clip_ids(&Fx::all_ids()).unwrap().iter().map(|r| json!(r.current_path)).collect());
    out.insert("fetchRowsPaths".into(), store.fetch_rows_by_paths(&Fx::all_paths()).unwrap().iter().map(|r| json!(r.current_path)).collect());
    Value::Object(out)
}

pub fn arg_json(a: &SqlArg) -> Value {
    match a {
        SqlArg::Int(i) => json!(["integer", i.to_string()]),
        SqlArg::Real(r) => json!(["real", format!("{:016x}", r.to_bits())]),
        SqlArg::Text(t) => json!(["text", t]),
    }
}

/// Run one golden query case (its search, the filter decoded from Swift's JSON, its sort) and return the
/// differences from what Swift recorded.
pub fn check_query_case(store: &MetadataStore, case: &Value) -> Vec<String> {
    let name = case["name"].as_str().unwrap_or("?").to_string();
    let mut diffs = Vec::new();
    let search = case["search"].as_str().unwrap();
    let sort = ClipSort::from_raw_value(case["sort"].as_str().unwrap()).unwrap();
    let Some(filter) = ClipFilter::from_json(case["filterJSON"].as_str().unwrap().as_bytes()) else {
        return vec![format!("{name}: Swift's filter JSON does not decode")];
    };
    if !same(&canon(&filter), &case["filter"]) {
        diffs.push(format!("{name}: decoded filter {} != {}", canon(&filter), case["filter"]));
    }
    let volumes = mounted();
    let plan = build_query_plan(search, &filter, sort, Some(&volumes));
    let paths_plan = build_query_plan_for_paths(search, &filter, sort, Some(&volumes));
    let bare = build_query_plan(search, &filter, sort, None);
    let checks = [
        ("sql", json!(plan.sql)),
        ("args", plan.args.iter().map(arg_json).collect()),
        ("pathsSQL", json!(paths_plan.sql)),
        ("bareSQL", json!(bare.sql)),
        ("bareArgs", bare.args.iter().map(arg_json).collect()),
    ];
    for (key, got) in checks {
        if got != case[key] {
            diffs.push(format!("{name}.{key}: {got} != {}", case[key]));
        }
    }
    let mut paths = store.query_paths(search, &filter, sort).unwrap();
    let mut ids = store.query(search, &filter, sort).unwrap();
    if sort == ClipSort::Random {
        paths.sort();
        ids.sort();
    }
    if json!(paths) != case["paths"] {
        diffs.push(format!("{name}.paths: {:?} != {}", paths, case["paths"]));
    }
    if json!(ids) != case["ids"] {
        diffs.push(format!("{name}.ids: {:?} != {}", ids, case["ids"]));
    }
    diffs
}

pub fn check_search(store: &MetadataStore, golden: &Value) -> Vec<String> {
    let mut diffs = Vec::new();
    for case in golden.as_array().unwrap() {
        let s = case["search"].as_str().unwrap();
        if json!(sanitize_search_query(s)) != case["expression"] {
            diffs.push(format!("{s:?}.expression"));
        }
        if json!(store.search_paths(s).unwrap()) != case["paths"] {
            diffs.push(format!("{s:?}.paths: {:?} != {}", store.search_paths(s).unwrap(), case["paths"]));
        }
        if case["ids"] != json!("swift-traps") && json!(store.search_clip_ids(s).unwrap()) != case["ids"] {
            diffs.push(format!("{s:?}.ids"));
        }
    }
    diffs
}

/// JSON equality with numbers compared by value (Foundation writes `1700000000` for the double 1.7e9).
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w))),
        _ => a == b,
    }
}

/// The keys of two JSON objects whose values differ (by `same`).
pub fn differing_keys(a: &Value, b: &Value) -> Vec<String> {
    let (Some(x), Some(y)) = (a.as_object(), b.as_object()) else { return vec!["<not objects>".into()] };
    let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
    keys.into_iter().filter(|k| !matches!((x.get(*k), y.get(*k)), (Some(p), Some(q)) if same(p, q))).cloned().collect()
}

/// Mask the values a library stamps with "now" (the rename's `updated_at`, the dismissal, preset and Alpha Type
/// stamps) and compare `filter_json` as a decoded filter, so a Swift dump and a Rust dump of the same scenario compare.
pub fn mask_volatile(dump: &mut Value) {
    let renamed = Fx::p_r();
    let dismissed = Fx::p_g();
    let tables = &mut dump["tables"];
    if let Some(rows) = tables["clips"]["rows"].as_array_mut() {
        for r in rows {
            let path = r["current_path"][1].as_str().unwrap_or("").to_string();
            if path == renamed {
                r["updated_at"][1] = json!("<now>");
            }
            if path == dismissed && !r["dismissed_at"].is_null() {
                r["dismissed_at"][1] = json!("<now>");
            }
        }
    }
    if let Some(rows) = tables["saved_filters"]["rows"].as_array_mut() {
        for r in rows {
            r["created_at"][1] = json!("<now>");
            let decoded = ClipFilter::from_json(r["filter_json"][1].as_str().unwrap_or("").as_bytes());
            r["filter_json"][1] = decoded.map_or(json!("<undecodable>"), |f| canon(&f));
        }
    }
    if let Some(rows) = tables["clip_alpha"]["rows"].as_array_mut() {
        for r in rows {
            r["updated_at"][1] = json!("<now>");
        }
    }
}

/// The paths at which two dumps differ (first few).
pub fn dump_diff(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    if out.len() > 20 {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for k in keys {
                dump_diff(x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), &format!("{path}.{k}"), out);
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                dump_diff(p, q, &format!("{path}[{i}]"), out);
            }
        }
        _ if a != b => out.push(format!("{path}: {} != {}", short(a), short(b))),
        _ => {}
    }
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 160 { format!("{}…", &s[..s.char_indices().nth(160).map_or(s.len(), |c| c.0)]) } else { s }
}

/// The sidecar byte cases (`Bytes.sidecars` in the oracle).
pub fn sidecar_cases() -> Vec<(&'static str, ClipMetadata)> {
    let mut full = ClipMetadata::new(Fx::b(), 1_731_600_000.123);
    full.rating = 5;
    full.tags = strs(&["blue", "intro", "\u{FC}n\u{EF}", "\u{65E5}\u{672C}", "a/b", "cafe\u{301}"]);
    full.bpm = Some(128.5);
    full.mood = Some("dark/\"moody\"".into());
    full.genre = Some("techno".into());
    full.energy = Some(7);
    full.brief = Some("Path a/b \"quoted\" \\ back\ttab\nnewline \u{1} \u{7F} \u{2028} \u{1F600} caf\u{E9}".into());
    full.updated_at = 1_731_600_500.5;
    let mut integral = ClipMetadata::new(Fx::c(), 1_700_000_000.0);
    integral.bpm = Some(120.0);
    integral.mood = Some(String::new());
    integral.energy = Some(10);
    integral.brief = Some(String::new());
    integral.updated_at = 1_700_000_001.0;
    let mut odd = ClipMetadata::new(Fx::d(), 0.00001);
    odd.version = 2;
    odd.rating = 3;
    odd.tags = strs(&["x"]);
    odd.bpm = Some(0.1);
    odd.energy = Some(1);
    odd.updated_at = 9_007_199_254_740_994.0;
    let mut big = ClipMetadata::new(Fx::e(), -1.5);
    big.bpm = Some(1e16);
    big.updated_at = 123_456_789.000_001;
    vec![
        ("nil-everything", ClipMetadata::new(Fx::a(), 1_700_000_000.0)),
        ("full-fractional", full),
        ("integral", integral),
        ("odd-numbers", odd),
        ("big-small", big),
    ]
}

pub fn stash_cases() -> Vec<Stash> {
    let entry = |clip_id: String, hint: &str| StashEntry { clip_id, relative_path_hint: hint.to_string() };
    vec![
        Stash {
            id: Fx::S1.into(),
            name: "Set A".into(),
            created_at: 1_731_600_000.123,
            updated_at: 1_731_600_500.5,
            clips: vec![entry(Fx::a(), "blue_intro.mov"), entry(Fx::d(), "odd name #%&?'\" .mov"), entry(Fx::e(), "dup/one.mov")],
        },
        Stash { id: Fx::S2.into(), name: "Empty Stash".into(), created_at: 1_700_000_000.0, updated_at: 1_700_000_000.0, clips: vec![] },
        Stash {
            id: Fx::S3.into(),
            name: "\u{DC}n\u{EF}c\u{F6}d\u{E9} / set \"x\"".into(),
            created_at: 0.5,
            updated_at: 1e-05,
            clips: vec![entry(Fx::b(), "Sub Dir/\u{DC}n\u{EF}c\u{F6}d\u{E9} caf\u{E9}.mov")],
        },
    ]
}
