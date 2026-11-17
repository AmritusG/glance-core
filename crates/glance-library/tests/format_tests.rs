//! Swift MaCrate 0.16.4's format tests, ported with the same inputs and expected values: `ClipMetadataTests`,
//! `SidecarStoreTests`, `StashOrderingTests`, `ClipFilterTests`, `ClipSortingTests`, plus the self-write tracker.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{TempDir, fixtures, read_json};
use glance_library::filter::{ClipFilter, ClipSort, MeasuredFlag, MeasuredSource, MeasuredValue, TagLogic};
use glance_library::sidecar::{ClipMetadata, SelfWriteTracker, delete_sidecar, move_sidecar, read_sidecar, scan_sidecars_direct, sidecar_path, write_sidecar};
use glance_library::sorting::{SortableClip, apply};
use glance_library::stash::{read_stash_file, reorder, write_stash_file};
use std::collections::BTreeSet;

fn tags(t: &[&str]) -> BTreeSet<String> {
    t.iter().map(|s| s.to_string()).collect()
}

// MARK: ClipMetadataTests

#[test]
fn clip_metadata_round_trips_losslessly() {
    let mut m = ClipMetadata::new("deadbeef:1024", 1_700_000_000.0);
    m.rating = 4;
    m.tags = vec!["intro".into(), "blue".into()];
    m.bpm = Some(128.0);
    m.mood = Some("dark".into());
    m.genre = Some("techno".into());
    m.energy = Some(7);
    m.updated_at = 1_700_000_500.0;
    assert_eq!(ClipMetadata::decode(&m.encode().unwrap()).unwrap(), m);
}

#[test]
fn emits_explicit_nulls_and_snake_case_keys() {
    let json = String::from_utf8(ClipMetadata::new("abc:0", 1_700_000_000.0).encode().unwrap()).unwrap();
    for k in ["\"bpm\" : null", "\"mood\" : null", "\"genre\" : null", "\"energy\" : null", "\"clip_id\"", "\"created_at\"", "\"updated_at\""] {
        assert!(json.contains(k), "{k}");
    }
    assert!(!json.contains("\"clipID\""));
}

#[test]
fn defaults_version_and_tolerates_missing_optionals() {
    let d = ClipMetadata::decode(br#"{"clip_id":"abc:0","rating":0,"tags":[],"created_at":1700000000,"updated_at":1700000000}"#).unwrap();
    assert_eq!(d.version, 1);
    let d = ClipMetadata::decode(br#"{"version":1,"clip_id":"abc:0","created_at":1700000000,"updated_at":1700000000}"#).unwrap();
    assert_eq!((d.clip_id.as_str(), d.rating, d.tags.len(), d.bpm), ("abc:0", 0, 0, None));
}

#[test]
fn file_size_from_clip_id() {
    let size = |id: &str| ClipMetadata::new(id, 0.0).file_size_from_clip_id();
    assert_eq!(size("abc:1048576"), Some(1_048_576));
    assert_eq!(size("abc:"), None);
    assert_eq!(size("abc"), None);
    assert_eq!(size("a:b:5"), None);
}

// MARK: SidecarStoreTests

#[test]
fn sidecar_url_is_hidden_and_scoped_to_clip() {
    let dir = TempDir::new("sidecar-url");
    let sidecar = sidecar_path(&dir.path().join("intro.mov"));
    assert_eq!(sidecar.file_name().unwrap(), ".intro.mov.crate-meta.json");
    assert_eq!(sidecar.parent().unwrap(), dir.path());
}

#[test]
fn sidecar_round_trip_overwrite_atomic_delete() {
    let dir = TempDir::new("sidecar");
    let clip = dir.path().join("clip.mov");
    std::fs::write(&clip, [0u8]).unwrap();
    let mut original = ClipMetadata::new("abc:1", 1_700_000_000.0);
    original.rating = 3;
    original.tags = vec!["a".into(), "b".into()];
    original.bpm = Some(120.0);
    original.genre = Some("house".into());
    original.energy = Some(8);
    original.updated_at = 1_700_000_001.0;
    let tracker = SelfWriteTracker::new();
    let written = write_sidecar(&original, &clip, Some(&tracker)).unwrap();
    assert!(tracker.is_self_write(&written));
    assert!(tracker.is_self_write(&dir.path().join("./x/../.clip.mov.crate-meta.json")), "compared after standardizing");
    assert!(!tracker.is_self_write(&dir.path().join("other")));
    assert_eq!(read_sidecar(&clip).unwrap(), Some(original.clone()));

    let mut second = original.clone();
    second.rating = 5;
    second.tags = vec!["a".into(), "b".into(), "c".into()];
    second.updated_at = 99.0;
    write_sidecar(&second, &clip, None).unwrap();
    assert_eq!(read_sidecar(&clip).unwrap(), Some(second));
    let names: Vec<String> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(names.iter().all(|n| !n.contains(".tmp")), "stray temp files: {names:?}");
    assert!(names.contains(&".clip.mov.crate-meta.json".to_string()));

    assert!(read_sidecar(&dir.path().join("nope.mov")).unwrap().is_none());
    delete_sidecar(&clip, None).unwrap();
    assert!(!sidecar_path(&clip).exists());
    delete_sidecar(&clip, None).unwrap();
}

#[test]
fn sidecar_read_fails_on_corrupt_json_and_moves_with_the_clip() {
    let dir = TempDir::new("sidecar-bad");
    let clip = dir.path().join("bad.mov");
    std::fs::write(sidecar_path(&clip), "not json").unwrap();
    assert!(read_sidecar(&clip).is_err());
    let good = dir.path().join("good.mov");
    write_sidecar(&ClipMetadata::new("g:1", 1.0), &good, None).unwrap();
    let moved = dir.path().join("moved.mov");
    move_sidecar(&good, &moved, None).unwrap();
    assert!(read_sidecar(&moved).unwrap().is_some() && read_sidecar(&good).unwrap().is_none());
    move_sidecar(&dir.path().join("none.mov"), &dir.path().join("none2.mov"), None).unwrap();
    write_sidecar(&ClipMetadata::new("h:1", 1.0), &good, None).unwrap();
    assert!(move_sidecar(&good, &moved, None).is_err(), "moveItem refuses an existing destination");
    let direct = scan_sidecars_direct(dir.path());
    assert_eq!(direct.len(), 2, "the corrupt one is skipped: {direct:?}");
}

// MARK: StashOrderingTests

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    clip_id: String,
    hint: String,
}

fn entries(ids: &[&str]) -> Vec<Entry> {
    ids.iter().map(|i| Entry { clip_id: i.to_string(), hint: format!("hint-{i}") }).collect()
}

fn ro(e: &[Entry], visible: &[&str]) -> Vec<String> {
    let visible: Vec<String> = visible.iter().map(|s| s.to_string()).collect();
    reorder(e, &visible, |x| x.clip_id.clone()).into_iter().map(|x| x.clip_id).collect()
}

#[test]
fn stash_ordering() {
    assert_eq!(ro(&entries(&["a", "b", "c", "d"]), &["c", "a", "d", "b"]), ["c", "a", "d", "b"]);
    assert_eq!(ro(&entries(&["a", "b", "c", "d", "e"]), &["d", "b"]), ["d", "b", "a", "c", "e"]);
    assert_eq!(ro(&entries(&["a", "b"]), &["b", "ghost", "a"]), ["b", "a"]);
    assert_eq!(ro(&entries(&["a", "b", "c"]), &[]), ["a", "b", "c"]);
    assert_eq!(ro(&entries(&["x1", "v1", "x2", "v2", "x3"]), &["v2", "v1"]), ["v2", "v1", "x1", "x2", "x3"]);
    let input = entries(&["a", "b"]);
    let visible = vec!["b".to_string(), "a".to_string()];
    assert_eq!(reorder(&input, &visible, |x| x.clip_id.clone()), vec![input[1].clone(), input[0].clone()]);
}

#[test]
fn stash_file_round_trip_and_unreadable_reads_empty() {
    let dir = TempDir::new("stash");
    assert!(read_stash_file(dir.path()).is_empty());
    let golden = std::fs::read(fixtures().join("json/stash-full/.crate-stashes.json")).unwrap();
    let stashes = glance_library::stash::decode_stash_file(&golden).unwrap();
    write_stash_file(&stashes, dir.path()).unwrap();
    assert_eq!(std::fs::read(dir.path().join(".crate-stashes.json")).unwrap(), golden);
    assert_eq!(read_stash_file(dir.path()), stashes);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no temp file left");
    std::fs::write(dir.path().join(".crate-stashes.json"), "{}").unwrap();
    assert!(read_stash_file(dir.path()).is_empty());
}

// MARK: ClipFilterTests

#[test]
fn filter_emptiness_and_preferred_sort() {
    assert!(ClipFilter::empty().is_empty());
    let e = ClipFilter::empty;
    for f in [
        ClipFilter { rating_min: Some(4), ..e() },
        ClipFilter { tags: tags(&["intro"]), ..e() },
        ClipFilter { bpm_range: Some((120.0, 130.0)), ..e() },
        ClipFilter { energy_range: Some((5, 10)), ..e() },
        ClipFilter { mood: Some("dark".into()), ..e() },
        ClipFilter { genre: Some("techno".into()), ..e() },
        ClipFilter { has_duplicates: true, ..e() },
        ClipFilter { measured_by: Some(MeasuredSource::Native), ..e() },
    ] {
        assert!(!f.is_empty(), "{f:?}");
    }
    assert_eq!(e().tags_logic, TagLogic::And);
    assert_eq!(e().preferred_sort(), None);
    assert_eq!(ClipFilter { rating_min: Some(4), ..e() }.preferred_sort(), Some(ClipSort::RatingDesc));
    assert_eq!(ClipFilter { bpm_range: Some((120.0, 130.0)), ..e() }.preferred_sort(), Some(ClipSort::BpmAsc));
    assert_eq!(ClipFilter { energy_range: Some((5, 10)), ..e() }.preferred_sort(), Some(ClipSort::EnergyDesc));
    assert_eq!(ClipFilter { tags: tags(&["intro"]), ..e() }.preferred_sort(), Some(ClipSort::DateAddedDesc));
    assert_eq!(ClipFilter { mood: Some("dark".into()), ..e() }.preferred_sort(), Some(ClipSort::DateAddedDesc));
    assert_eq!(ClipFilter { genre: Some("techno".into()), ..e() }.preferred_sort(), Some(ClipSort::DateAddedDesc));
    let all = ClipFilter { rating_min: Some(4), bpm_range: Some((120.0, 130.0)), tags: tags(&["intro"]), energy_range: Some((5, 10)), ..e() };
    assert_eq!(all.preferred_sort(), Some(ClipSort::RatingDesc));
    assert_eq!(ClipFilter { rating_min: None, ..all }.preferred_sort(), Some(ClipSort::BpmAsc));
}

#[test]
fn filter_json_round_trips() {
    let e = ClipFilter::empty;
    for f in [
        ClipFilter {
            rating_min: Some(4),
            tags: tags(&["intro", "techno"]),
            tags_logic: TagLogic::Or,
            bpm_range: Some((120.0, 130.0)),
            energy_range: Some((5, 10)),
            mood: Some("dark".into()),
            genre: Some("techno".into()),
            ..e()
        },
        e(),
        ClipFilter { has_duplicates: true, ..e() },
        ClipFilter { measured_by: Some(MeasuredSource::Native), ..e() },
        ClipFilter { loops_only: Some(MeasuredFlag::new(false)), cycle_range: Some(MeasuredValue::of((2.0, 8.0))), ..e() },
    ] {
        assert_eq!(ClipFilter::from_json(f.to_json().unwrap().as_bytes()), Some(f.clone()));
    }
    let empty = e().to_json().unwrap();
    assert!(!empty.contains("hasDuplicates") && !empty.contains("loopsOnly"));
    let pre = ClipFilter { rating_min: Some(4), tags: tags(&["intro"]), bpm_range: Some((120.0, 130.0)), ..e() }.to_json().unwrap();
    assert!(!pre.contains("hasDuplicates"));
    let legacy = ClipFilter::from_json(br#"{"ratingMin":3,"tags":["intro"],"tagsLogic":"and"}"#).unwrap();
    assert_eq!((legacy.rating_min, legacy.has_measured_criteria(), legacy.measured_by), (Some(3), false, None));
    let f = ClipFilter::from_json(br#"{"tags":[],"tagsLogic":"and","loopsOnly":{},"cycleRange":{"value":[2,8]}}"#).unwrap();
    assert_eq!(f.loops_only, Some(MeasuredFlag::new(true)));
    assert_eq!(f.cycle_range, Some(MeasuredValue::new((2.0, 8.0), true)));
}

#[test]
fn clip_sort_raw_values_and_names() {
    for (s, raw) in [
        (ClipSort::DateAddedDesc, "dateAddedDesc"),
        (ClipSort::FilenameAZ, "filenameAZ"),
        (ClipSort::RatingDesc, "ratingDesc"),
        (ClipSort::BpmAsc, "bpmAsc"),
        (ClipSort::Random, "random"),
    ] {
        assert_eq!(s.raw_value(), raw);
    }
    for s in ClipSort::ALL {
        assert!(!s.display_name().is_empty());
        assert_eq!(ClipSort::from_raw_value(s.raw_value()), Some(s));
    }
}

// MARK: ClipSortingTests

#[derive(Clone, Debug)]
struct F {
    name: String,
    mtime: f64,
    duration: Option<f64>,
}

impl SortableClip for F {
    fn display_name(&self) -> &str {
        &self.name
    }
    fn mtime(&self) -> f64 {
        self.mtime
    }
    fn duration_sec(&self) -> Option<f64> {
        self.duration
    }
}

fn clip(name: &str, mtime: f64, duration: Option<f64>) -> F {
    F { name: name.into(), mtime, duration }
}

fn names(v: &[F]) -> Vec<String> {
    v.iter().map(|f| f.name.clone()).collect()
}

#[test]
fn clip_sorting() {
    let three: Vec<F> = ["Banana.mov", "apple.mov", "Cherry.mov"].iter().map(|n| clip(n, 0.0, None)).collect();
    assert_eq!(names(&apply(&three, ClipSort::FilenameAZ, 0)), ["apple.mov", "Banana.mov", "Cherry.mov"]);
    assert_eq!(names(&apply(&three, ClipSort::FilenameZA, 0)), ["Cherry.mov", "Banana.mov", "apple.mov"]);
    for s in [ClipSort::RatingDesc, ClipSort::BpmAsc, ClipSort::BpmDesc, ClipSort::EnergyAsc, ClipSort::EnergyDesc] {
        assert_eq!(names(&apply(&three, s, 0)), ["apple.mov", "Banana.mov", "Cherry.mov"]);
    }
    let times = vec![clip("old.mov", 100.0, None), clip("new.mov", 300.0, None), clip("mid.mov", 200.0, None)];
    assert_eq!(names(&apply(&times, ClipSort::MtimeNewest, 0)), ["new.mov", "mid.mov", "old.mov"]);
    assert_eq!(names(&apply(&times, ClipSort::MtimeOldest, 0)), ["old.mov", "mid.mov", "new.mov"]);
    let durations = vec![clip("b-none.mov", 0.0, None), clip("short.mov", 0.0, Some(5.0)), clip("a-none.mov", 0.0, None), clip("long.mov", 0.0, Some(50.0))];
    assert_eq!(names(&apply(&durations, ClipSort::DurationAsc, 0)), ["short.mov", "long.mov", "a-none.mov", "b-none.mov"]);
    assert_eq!(names(&apply(&durations, ClipSort::DurationDesc, 0)), ["long.mov", "short.mov", "a-none.mov", "b-none.mov"]);
}

#[test]
fn shuffle_and_name_order_match_swift() {
    let rules = read_json(&fixtures().join("rules.json"));
    let files: Vec<F> = (0..25).map(|i| clip(&format!("clip{i}.mov"), 0.0, None)).collect();
    for case in rules["shuffle"].as_array().unwrap() {
        let seed: u64 = case[0].as_str().unwrap().parse().unwrap();
        let want: Vec<String> = case[1].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert_eq!(names(&apply(&files, ClipSort::Random, seed)), want, "seed {seed}");
    }
    let want: Vec<String> = rules["sortAZ"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let mut shuffled: Vec<F> = want.iter().rev().map(|n| clip(n, 0.0, None)).collect();
    shuffled.rotate_left(3);
    assert_eq!(names(&apply(&shuffled, ClipSort::FilenameAZ, 0)), want);
}

#[test]
fn seventeen_digit_timestamps_read_as_foundation_reads_them() {
    // Sidecars written by other tools carry 17 significant digits; Foundation parses them exactly rounded.
    for text in ["1778925862.2037559", "1731600000.1234567", "1700000000.0000001", "1778925862.2037558"] {
        let want: f64 = text.parse().unwrap();
        let json = format!(r#"{{"clip_id":"00:1","created_at":{text},"updated_at":{text}}}"#);
        let meta = glance_library::ClipMetadata::decode(json.as_bytes()).unwrap();
        assert_eq!(meta.updated_at.to_bits(), want.to_bits(), "{text}");
        assert_eq!(meta.created_at.to_bits(), want.to_bits(), "{text}");
    }
}
