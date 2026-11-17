//! Swift MaCrate 0.16.4's store tests, ported with the same inputs and expected values: `MetadataStoreTests`,
//! `RetireOrphansTests`, `MeasuredFilterQueryTests` and the store parts of `NativeMeasurementStoreTests`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_library::filter::{ClipFilter, ClipSort, Ground, MeasuredFlag, MeasuredSource, MeasuredValue, MotionState, TagLogic};
use glance_library::measurements::{self, ClipMeasurement};
use glance_library::query::build_query_plan;
use glance_library::search::sanitize_search_query;
use glance_library::store::{ClipRow, Location, MetadataStore};
use glance_library::{LibraryError, PresetError};
use std::collections::BTreeSet;

fn store() -> MetadataStore {
    MetadataStore::open(Location::InMemory).unwrap()
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn set(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn make_row(clip_id: &str, rating: i64, bpm: Option<f64>, orphan: bool) -> ClipRow {
    let mut r = ClipRow::new(clip_id, format!("/tmp/{clip_id}.mov"), 100, 1_700_000_000.0, 1_700_000_000.0, 1_700_000_000.0);
    r.rating = rating;
    r.bpm = bpm;
    r.is_orphan = orphan;
    r
}

fn plain(clip_id: &str) -> ClipRow {
    make_row(clip_id, 0, None, false)
}

fn row_at(clip_id: &str, path: &str) -> ClipRow {
    ClipRow::new(clip_id, path, 1, 1.0, 1.0, 1.0)
}

fn with_filename(clip_id: &str, filename: &str) -> ClipRow {
    ClipRow::new(clip_id, format!("/tmp/library/{filename}"), 1024, 1_700_000_000.0, 1_700_000_000.0, 1_700_000_000.0)
}

#[derive(Default)]
struct Q<'a> {
    filename: Option<&'a str>,
    rating: i64,
    bpm: Option<f64>,
    energy: Option<i64>,
    mood: Option<&'a str>,
    genre: Option<&'a str>,
    tags: &'a [&'a str],
    orphan: bool,
    created: Option<f64>,
}

/// `MetadataStoreTests.upsert(_:clipID:…)`.
fn up(store: &mut MetadataStore, clip_id: &str, q: Q<'_>) -> ClipRow {
    let filename = q.filename.map_or(format!("{clip_id}.mov"), str::to_string);
    let created = q.created.unwrap_or(1_700_000_000.0);
    let mut r = ClipRow::new(clip_id, format!("/tmp/library/{filename}"), 1024, created, created, created);
    r.rating = q.rating;
    r.bpm = q.bpm;
    r.energy = q.energy;
    r.mood = q.mood.map(str::to_string);
    r.genre = q.genre.map(str::to_string);
    r.is_orphan = q.orphan;
    store.upsert(&r, &s(q.tags)).unwrap();
    r
}

fn query(store: &MetadataStore, filter: &ClipFilter, sort: ClipSort) -> Vec<String> {
    store.query("", filter, sort).unwrap()
}

#[test]
fn total_file_size_under_prefix_sums_non_orphan() {
    let mut st = store();
    let row = |id: &str, path: &str, size: i64, orphan: bool| {
        let mut r = ClipRow::new(id, path, size, 1.0, 1.0, 1.0);
        r.is_orphan = orphan;
        r
    };
    st.upsert(&row("a:1", "/Vol/Root/a.mov", 100, false), &[]).unwrap();
    st.upsert(&row("b:2", "/Vol/Root/sub/b.mov", 250, false), &[]).unwrap();
    st.upsert(&row("c:3", "/Vol/Other/c.mov", 999, false), &[]).unwrap();
    st.upsert(&row("d:4", "/Vol/Root/d.mov", 500, true), &[]).unwrap();
    assert_eq!(st.total_file_size("/Vol/Root").unwrap(), 350);
    assert_eq!(st.total_file_size("/Vol/Nope").unwrap(), 0);
}

#[test]
fn upsert_then_fetch_round_trip() {
    let mut st = store();
    let row = make_row("abc:100", 4, Some(128.0), false);
    st.upsert(&row, &s(&["intro", "blue"])).unwrap();
    assert_eq!(st.fetch_by_clip_id("abc:100").unwrap(), Some(row));
    assert_eq!(st.tags_for_clip_id("abc:100").unwrap(), s(&["blue", "intro"]));
}

#[test]
fn upsert_replaces_existing_row_and_tags() {
    let mut st = store();
    let row = make_row("abc:100", 1, None, false);
    st.upsert(&row, &s(&["a", "b"])).unwrap();
    let mut updated = row.clone();
    updated.rating = 5;
    updated.updated_at = 1_700_001_000.0;
    st.upsert(&updated, &s(&["c"])).unwrap();
    let fetched = st.fetch_by_clip_id("abc:100").unwrap().unwrap();
    assert_eq!((fetched.rating, fetched.updated_at), (5, 1_700_001_000.0));
    assert_eq!(st.tags_for_clip_id("abc:100").unwrap(), s(&["c"]), "tags must be fully replaced, not merged");
}

#[test]
fn upsert_keeps_dismissed_at() {
    let mut st = store();
    let mut row = make_row("o:1", 0, None, true);
    st.upsert(&row, &[]).unwrap();
    assert_eq!(st.dismiss_all_orphans().unwrap(), 1);
    row.rating = 3;
    st.upsert(&row, &[]).unwrap();
    assert_eq!(st.count_dismissed_orphans().unwrap(), 1, "an upsert is an UPDATE: dismissed_at survives (not a REPLACE)");
}

#[test]
fn delete_cascades_tags() {
    let mut st = store();
    st.upsert(&plain("del:1"), &s(&["x", "y"])).unwrap();
    st.delete_clip_id("del:1").unwrap();
    assert_eq!(st.fetch_by_clip_id("del:1").unwrap(), None);
    assert!(st.tags_for_clip_id("del:1").unwrap().is_empty());
    assert!(st.all_tags().unwrap().is_empty(), "FK ON DELETE CASCADE must remove tag rows");
}

#[test]
fn fetch_all_returns_all_rows() {
    let mut st = store();
    for id in ["a:1", "b:2", "c:3"] {
        st.upsert(&plain(id), &[]).unwrap();
    }
    let ids: BTreeSet<String> = st.fetch_all().unwrap().into_iter().map(|r| r.clip_id).collect();
    assert_eq!(ids, set(&["a:1", "b:2", "c:3"]));
}

#[test]
fn clip_ids_with_tag_and_all_tags() {
    let mut st = store();
    st.upsert(&plain("a:1"), &s(&["intro", "blue"])).unwrap();
    st.upsert(&plain("b:2"), &s(&["intro", "red"])).unwrap();
    st.upsert(&plain("c:3"), &s(&["outro"])).unwrap();
    assert_eq!(st.clip_ids_with_tag("intro").unwrap(), s(&["a:1", "b:2"]));
    assert_eq!(st.clip_ids_with_tag("outro").unwrap(), s(&["c:3"]));
    assert_eq!(st.all_tags().unwrap(), s(&["blue", "intro", "outro", "red"]));
}

#[test]
fn set_orphan_flag_persists() {
    let mut st = store();
    st.upsert(&plain("o:1"), &[]).unwrap();
    st.set_orphan_by_clip_id("o:1", true).unwrap();
    assert!(st.fetch_by_clip_id("o:1").unwrap().unwrap().is_orphan);
    st.set_orphan_by_clip_id("o:1", false).unwrap();
    assert!(!st.fetch_by_clip_id("o:1").unwrap().unwrap().is_orphan);
}

#[test]
fn clearing_orphan_clears_dismissal_and_undismiss_keeps_orphan() {
    let mut st = store();
    st.upsert(&make_row("o:1", 0, None, true), &[]).unwrap();
    st.dismiss_all_orphans().unwrap();
    st.clear_dismissal_at("/tmp/o:1.mov").unwrap();
    assert_eq!((st.count_orphans().unwrap(), st.count_dismissed_orphans().unwrap()), (1, 0));
    st.dismiss_all_orphans().unwrap();
    st.set_orphan_at("/tmp/o:1.mov", false).unwrap();
    assert_eq!((st.count_orphans().unwrap(), st.count_dismissed_orphans().unwrap()), (0, 0));
    st.set_orphan_at("/tmp/o:1.mov", true).unwrap();
    assert_eq!(st.fetch_orphans().unwrap().len(), 1, "back to an active orphan, not dismissed");
}

#[test]
fn fetch_missing_returns_none_and_nil_optionals_persist_as_null() {
    let mut st = store();
    assert_eq!(st.fetch_by_clip_id("ghost:0").unwrap(), None);
    st.upsert(&plain("opt:1"), &[]).unwrap();
    let f = st.fetch_by_clip_id("opt:1").unwrap().unwrap();
    assert!(f.bpm.is_none() && f.mood.is_none() && f.energy.is_none());
}

// MARK: search

#[test]
fn search_matches_filename_prefix() {
    let mut st = store();
    st.upsert(&with_filename("1", "blue_intro.mov"), &[]).unwrap();
    st.upsert(&with_filename("2", "red_outro.mov"), &[]).unwrap();
    st.upsert(&with_filename("3", "bluebird_drop.mov"), &[]).unwrap();
    let hits: BTreeSet<String> = st.search_clip_ids("blu").unwrap().into_iter().collect();
    assert_eq!(hits, set(&["1", "3"]));
}

#[test]
fn search_two_tokens_implies_and() {
    let mut st = store();
    st.upsert(&with_filename("1", "blue_intro.mov"), &[]).unwrap();
    st.upsert(&with_filename("2", "blue_outro.mov"), &[]).unwrap();
    st.upsert(&with_filename("3", "red_intro.mov"), &[]).unwrap();
    assert_eq!(st.search_clip_ids("blue intro").unwrap(), s(&["1"]));
}

#[test]
fn search_ignores_extension() {
    let mut st = store();
    st.upsert(&with_filename("1", "techno_set.mov"), &[]).unwrap();
    st.upsert(&with_filename("2", "movement.mov"), &[]).unwrap();
    assert_eq!(st.search_clip_ids("mov").unwrap(), s(&["2"]));
}

#[test]
fn search_empty_and_junk_input_returns_no_results() {
    let mut st = store();
    st.upsert(&with_filename("1", "anything.mov"), &[]).unwrap();
    for q in ["", "   ", "()*\"':", "&"] {
        assert!(st.search_clip_ids(q).unwrap().is_empty(), "{q:?}");
    }
}

#[test]
fn search_underscore_hyphen_and_keywords() {
    let mut st = store();
    st.upsert(&with_filename("1", "blue_intro.mov"), &[]).unwrap();
    st.upsert(&with_filename("2", "red_outro.mov"), &[]).unwrap();
    assert_eq!(st.search_clip_ids("blue_intro").unwrap(), s(&["1"]));
    st.upsert(&with_filename("3", "RedForm_4k_01.mov"), &[]).unwrap();
    assert_eq!(st.search_clip_ids("RedForm_4k").unwrap(), s(&["3"]));

    let mut st = store();
    st.upsert(&with_filename("1", "blue-intro.mov"), &[]).unwrap();
    st.upsert(&with_filename("2", "blue_outro.mov"), &[]).unwrap();
    assert_eq!(st.search_clip_ids("blue-intro").unwrap(), s(&["1"]));
    assert_eq!(st.search_clip_ids("blue_intro").unwrap(), s(&["1"]));

    let mut st = store();
    st.upsert(&with_filename("1", "rock_and_roll.mov"), &[]).unwrap();
    st.upsert(&with_filename("2", "rock_solo.mov"), &[]).unwrap();
    assert_eq!(st.search_clip_ids("rock AND").unwrap(), s(&["1"]));
}

#[test]
fn delete_removes_from_search_index() {
    let mut st = store();
    st.upsert(&with_filename("1", "doomed.mov"), &[]).unwrap();
    assert_eq!(st.search_clip_ids("doomed").unwrap(), s(&["1"]));
    st.delete_clip_id("1").unwrap();
    assert!(st.search_clip_ids("doomed").unwrap().is_empty());
}

#[test]
fn upsert_at_same_path_replaces_fts_row() {
    let mut st = store();
    let mut row = with_filename("1", "tag-bearer.mov");
    row.mood = Some("dark".into());
    st.upsert(&row, &[]).unwrap();
    assert_eq!(st.search_paths("dark").unwrap(), vec![row.current_path.clone()]);
    row.mood = Some("bright".into());
    st.upsert(&row, &[]).unwrap();
    assert!(st.search_paths("dark").unwrap().is_empty());
    assert_eq!(st.search_paths("bright").unwrap(), vec![row.current_path.clone()]);
}

#[test]
fn upsert_at_new_path_creates_distinct_row() {
    let mut st = store();
    let mut local = ClipRow::new("shared:1024", "/tmp/local/clip.mov", 1024, 1_700_000_000.0, 1_700_000_000.0, 1_700_000_000.0);
    local.mood = Some("calm".into());
    let mut external = ClipRow::new("shared:1024", "/tmp/external/clip.mov", 1024, 1_700_000_000.0, 1_700_000_000.0, 1_700_000_000.0);
    external.mood = Some("energetic".into());
    st.upsert(&local, &s(&["personal"])).unwrap();
    st.upsert(&external, &s(&["work"])).unwrap();
    assert_eq!(st.fetch_at(&local.current_path).unwrap().unwrap().mood.as_deref(), Some("calm"));
    assert_eq!(st.fetch_at(&external.current_path).unwrap().unwrap().mood.as_deref(), Some("energetic"));
    assert_eq!(st.tags_for_path(&local.current_path).unwrap(), s(&["personal"]));
    assert_eq!(st.tags_for_path(&external.current_path).unwrap(), s(&["work"]));
    assert_eq!(st.clip_ids_with_duplicates().unwrap(), s(&["shared:1024"]));
    let siblings: Vec<String> = st.fetch_duplicates("shared:1024", &local.current_path).unwrap().into_iter().map(|r| r.current_path).collect();
    assert_eq!(siblings, vec![external.current_path]);
}

#[test]
fn set_path_moves_fts_across_rename() {
    let mut st = store();
    let row = with_filename("1", "before.mov");
    st.upsert(&row, &s(&["intro"])).unwrap();
    assert_eq!(st.search_paths("before").unwrap(), vec![row.current_path.clone()]);
    let new_path = "/tmp/library/after.mov";
    st.set_path(&row.current_path, new_path).unwrap();
    assert!(st.search_paths("before").unwrap().is_empty());
    assert_eq!(st.search_paths("after").unwrap(), s(&[new_path]));
    assert_eq!(st.tags_for_path(new_path).unwrap(), s(&["intro"]), "ON UPDATE CASCADE");
    assert!(st.tags_for_path(&row.current_path).unwrap().is_empty());
}

#[test]
fn set_path_onto_an_existing_path_is_a_constraint_error() {
    let mut st = store();
    st.upsert(&with_filename("1", "a.mov"), &[]).unwrap();
    st.upsert(&with_filename("2", "b.mov"), &[]).unwrap();
    assert!(matches!(st.set_path("/tmp/library/a.mov", "/tmp/library/b.mov"), Err(LibraryError::Sqlite(_))));
}

#[test]
fn fetch_rows_by_clip_ids() {
    let mut st = store();
    for id in ["a", "b", "c"] {
        st.upsert(&with_filename(id, &format!("{id}.mov")), &[]).unwrap();
    }
    let ids: BTreeSet<String> = st.fetch_rows_by_clip_ids(&s(&["a", "c"])).unwrap().into_iter().map(|r| r.clip_id).collect();
    assert_eq!(ids, set(&["a", "c"]));
    assert!(st.fetch_rows_by_clip_ids(&[]).unwrap().is_empty());
}

// MARK: composite query

#[test]
fn query_empty_filter_returns_all_non_orphans() {
    let mut st = store();
    up(&mut st, "a", Q { filename: Some("a.mov"), ..Q::default() });
    up(&mut st, "b", Q { filename: Some("b.mov"), ..Q::default() });
    up(&mut st, "c", Q { filename: Some("c.mov"), orphan: true, ..Q::default() });
    assert_eq!(query(&st, &ClipFilter::empty(), ClipSort::FilenameAZ), s(&["a", "b"]));
}

#[test]
fn query_non_blank_but_unmatchable_search_returns_no_rows() {
    let mut st = store();
    up(&mut st, "a", Q { filename: Some("alpha.mov"), ..Q::default() });
    up(&mut st, "b", Q { filename: Some("beta.mov"), ..Q::default() });
    for junk in ["&", "()", ":", "*\"'"] {
        assert!(st.query(junk, &ClipFilter::empty(), ClipSort::FilenameAZ).unwrap().is_empty(), "{junk}");
    }
    assert_eq!(st.query("", &ClipFilter::empty(), ClipSort::FilenameAZ).unwrap(), s(&["a", "b"]));
    assert_eq!(st.query("   ", &ClipFilter::empty(), ClipSort::FilenameAZ).unwrap(), s(&["a", "b"]));
    assert_eq!(st.query("alpha", &ClipFilter::empty(), ClipSort::FilenameAZ).unwrap(), s(&["a"]));
}

#[test]
fn query_rating_bpm_energy_mood_genre() {
    let mut st = store();
    for (id, rating) in [("1", 5), ("2", 4), ("3", 2), ("4", 0)] {
        up(&mut st, id, Q { rating, ..Q::default() });
    }
    assert_eq!(query(&st, &ClipFilter { rating_min: Some(4), ..ClipFilter::empty() }, ClipSort::RatingDesc), s(&["1", "2"]));

    let mut st = store();
    for (id, bpm) in [("1", Some(100.0)), ("2", Some(125.0)), ("3", Some(130.0)), ("4", Some(140.0)), ("5", None)] {
        up(&mut st, id, Q { bpm, ..Q::default() });
    }
    assert_eq!(query(&st, &ClipFilter { bpm_range: Some((120.0, 130.0)), ..ClipFilter::empty() }, ClipSort::BpmAsc), s(&["2", "3"]));

    let mut st = store();
    for (id, energy) in [("1", 3), ("2", 5), ("3", 10)] {
        up(&mut st, id, Q { energy: Some(energy), ..Q::default() });
    }
    assert_eq!(query(&st, &ClipFilter { energy_range: Some((5, 10)), ..ClipFilter::empty() }, ClipSort::EnergyDesc), s(&["3", "2"]));

    let mut st = store();
    up(&mut st, "1", Q { mood: Some("dark"), genre: Some("techno"), ..Q::default() });
    up(&mut st, "2", Q { mood: Some("dark"), genre: Some("house"), ..Q::default() });
    up(&mut st, "3", Q { mood: Some("bright"), genre: Some("techno"), ..Q::default() });
    let mut f = ClipFilter { mood: Some("dark".into()), ..ClipFilter::empty() };
    let darks: BTreeSet<String> = query(&st, &f, ClipSort::FilenameAZ).into_iter().collect();
    assert_eq!(darks, set(&["1", "2"]));
    f.genre = Some("techno".into());
    assert_eq!(query(&st, &f, ClipSort::FilenameAZ), s(&["1"]));
}

#[test]
fn query_tags_and_or() {
    let mut st = store();
    up(&mut st, "1", Q { tags: &["intro", "techno"], ..Q::default() });
    up(&mut st, "2", Q { tags: &["intro"], ..Q::default() });
    up(&mut st, "3", Q { tags: &["techno"], ..Q::default() });
    up(&mut st, "4", Q { tags: &["outro"], ..Q::default() });
    let mut f = ClipFilter { tags: set(&["intro", "techno"]), tags_logic: TagLogic::And, ..ClipFilter::empty() };
    assert_eq!(query(&st, &f, ClipSort::FilenameAZ), s(&["1"]));
    f.tags_logic = TagLogic::Or;
    let hits: BTreeSet<String> = query(&st, &f, ClipSort::FilenameAZ).into_iter().collect();
    assert_eq!(hits, set(&["1", "2", "3"]));
}

#[test]
fn query_combines_rating_tag_bpm_and_search() {
    let mut st = store();
    up(&mut st, "match", Q { rating: 5, bpm: Some(128.0), tags: &["intro"], ..Q::default() });
    up(&mut st, "low-rating", Q { rating: 2, bpm: Some(128.0), tags: &["intro"], ..Q::default() });
    up(&mut st, "no-tag", Q { rating: 5, bpm: Some(128.0), ..Q::default() });
    up(&mut st, "wrong-bpm", Q { rating: 5, bpm: Some(90.0), tags: &["intro"], ..Q::default() });
    let f = ClipFilter { rating_min: Some(4), tags: set(&["intro"]), bpm_range: Some((120.0, 130.0)), ..ClipFilter::empty() };
    assert_eq!(query(&st, &f, ClipSort::FilenameAZ), s(&["match"]));

    let mut st = store();
    up(&mut st, "1", Q { filename: Some("blue_intro.mov"), rating: 5, tags: &["intro"], ..Q::default() });
    up(&mut st, "2", Q { filename: Some("blue_outro.mov"), rating: 5, tags: &["outro"], ..Q::default() });
    up(&mut st, "3", Q { filename: Some("red_intro.mov"), rating: 5, tags: &["intro"], ..Q::default() });
    let f = ClipFilter { rating_min: Some(4), tags: set(&["intro"]), ..ClipFilter::empty() };
    assert_eq!(st.query("blue", &f, ClipSort::FilenameAZ).unwrap(), s(&["1"]));
}

#[test]
fn query_excludes_orphans_across_every_filter() {
    let mut st = store();
    up(&mut st, "live", Q { rating: 5, ..Q::default() });
    up(&mut st, "dead", Q { rating: 5, orphan: true, ..Q::default() });
    assert_eq!(query(&st, &ClipFilter { rating_min: Some(4), ..ClipFilter::empty() }, ClipSort::RatingDesc), s(&["live"]));
}

#[test]
fn query_sort_variants_and_null_bpm_last() {
    let mut st = store();
    up(&mut st, "older", Q { filename: Some("z.mov"), rating: 1, bpm: Some(100.0), created: Some(1_700_000_000.0), ..Q::default() });
    up(&mut st, "newer", Q { filename: Some("a.mov"), rating: 5, bpm: Some(130.0), created: Some(1_700_001_000.0), ..Q::default() });
    let e = ClipFilter::empty();
    assert_eq!(query(&st, &e, ClipSort::FilenameAZ), s(&["newer", "older"]));
    assert_eq!(query(&st, &e, ClipSort::FilenameZA), s(&["older", "newer"]));
    assert_eq!(query(&st, &e, ClipSort::DateAddedDesc), s(&["newer", "older"]));
    assert_eq!(query(&st, &e, ClipSort::RatingDesc), s(&["newer", "older"]));
    assert_eq!(query(&st, &e, ClipSort::BpmAsc), s(&["older", "newer"]));

    let mut st = store();
    up(&mut st, "no-bpm", Q { filename: Some("a.mov"), ..Q::default() });
    up(&mut st, "slow", Q { filename: Some("b.mov"), bpm: Some(100.0), ..Q::default() });
    up(&mut st, "fast", Q { filename: Some("c.mov"), bpm: Some(140.0), ..Q::default() });
    assert_eq!(query(&st, &e, ClipSort::BpmAsc), s(&["slow", "fast", "no-bpm"]));
}

#[test]
fn query_random_sort_reseeds_per_call() {
    let mut st = store();
    for i in 0..8 {
        up(&mut st, &format!("c{i}"), Q::default());
    }
    let orders: BTreeSet<Vec<String>> = (0..6).map(|_| query(&st, &ClipFilter::empty(), ClipSort::Random)).collect();
    assert!(orders.iter().all(|o| o.len() == 8));
    assert!(orders.len() > 1);
}

#[test]
fn query_plan_shapes() {
    let e = ClipFilter::empty();
    let plan = build_query_plan("", &e, ClipSort::FilenameAZ, None);
    assert!(!plan.sql.contains("JOIN clip_search") && !plan.sql.contains("rating >=") && !plan.sql.contains("BETWEEN"));
    assert!(plan.sql.contains("is_orphan = 0"));
    assert!(!plan.sql.contains("HAVING COUNT(*) > 1"));
    let with_search = build_query_plan("blue", &e, ClipSort::FilenameAZ, None);
    assert!(with_search.sql.contains("JOIN clip_search") && with_search.sql.contains("MATCH"));
    let mut f = ClipFilter { tags: set(&["intro", "techno"]), tags_logic: TagLogic::And, ..ClipFilter::empty() };
    assert!(build_query_plan("", &f, ClipSort::FilenameAZ, None).sql.contains("HAVING COUNT(DISTINCT tag) = ?"));
    f.tags_logic = TagLogic::Or;
    assert!(!build_query_plan("", &f, ClipSort::FilenameAZ, None).sql.contains("HAVING"));
    let dups = build_query_plan("", &ClipFilter { has_duplicates: true, ..ClipFilter::empty() }, ClipSort::FilenameAZ, None);
    assert!(dups.sql.contains("GROUP BY clip_id") && dups.sql.contains("HAVING COUNT(*) > 1"));
    let rated = build_query_plan("", &ClipFilter { rating_min: Some(3), ..ClipFilter::empty() }, ClipSort::RatingDesc, None);
    assert!(!rated.sql.contains("clip_measurements"));
    let measured = build_query_plan("", &ClipFilter { loops_only: Some(MeasuredFlag::default()), ..ClipFilter::empty() }, ClipSort::RatingDesc, None);
    assert!(measured.sql.contains("LEFT JOIN meas.clip_measurements m ON m.clip_id = c.clip_id"));
    assert!(!measured.sql.contains("xfade"));
}

#[test]
fn sanitize_search_query_produces_valid_fts_expression() {
    assert_eq!(sanitize_search_query("blue intro"), "\"blue\"* \"intro\"*");
    assert_eq!(sanitize_search_query("blue's"), "\"blue\"* \"s\"*");
}

#[test]
fn query_has_duplicates_returns_only_duplicate_clip_ids() {
    let mut st = store();
    for (id, path) in [("shared:99", "/tmp/library/a.mov"), ("shared:99", "/tmp/library/b.mov"), ("unique:77", "/tmp/library/c.mov")] {
        st.upsert(&ClipRow::new(id, path, 99, 1_700_000_000.0, 1_700_000_000.0, 1_700_000_000.0), &[]).unwrap();
    }
    let f = ClipFilter { has_duplicates: true, ..ClipFilter::empty() };
    assert_eq!(query(&st, &f, ClipSort::FilenameAZ), s(&["shared:99"]));
    assert_eq!(st.query_paths("", &f, ClipSort::FilenameAZ).unwrap(), s(&["/tmp/library/a.mov", "/tmp/library/b.mov"]));
}

#[test]
fn fetch_rows_for_clip_id_returns_all_sorted() {
    let mut st = store();
    for (id, path) in
        [("trip:200", "/tmp/c-third.mov"), ("trip:200", "/tmp/a-first.mov"), ("trip:200", "/tmp/b-second.mov"), ("other:300", "/tmp/unrelated.mov")]
    {
        st.upsert(&ClipRow::new(id, path, 200, 1_700_000_000.0, 1_700_000_000.0, 1_700_000_000.0), &[]).unwrap();
    }
    let paths: Vec<String> = st.fetch_rows_for_clip_id("trip:200").unwrap().into_iter().map(|r| r.current_path).collect();
    assert_eq!(paths, s(&["/tmp/a-first.mov", "/tmp/b-second.mov", "/tmp/c-third.mov"]));
}

// MARK: briefs

fn up_brief(st: &mut MetadataStore, id: &str, path: &str, brief: Option<&str>, orphan: bool, updated: f64) {
    let mut r = ClipRow::new(id, path, 100, 1_700_000_000.0, 1_700_000_000.0, updated);
    r.brief = brief.map(str::to_string);
    r.is_orphan = orphan;
    st.upsert(&r, &[]).unwrap();
}

#[test]
fn unbriefed_queries() {
    let mut st = store();
    up_brief(&mut st, "dup:1", "/tmp/folder/a.mov", None, false, 1_700_000_300.0);
    up_brief(&mut st, "dup:1", "/tmp/folder/a-copy.mov", None, false, 1_700_000_200.0);
    up_brief(&mut st, "solo:1", "/tmp/folder/solo.mov", None, false, 1_700_000_100.0);
    up_brief(&mut st, "done:1", "/tmp/folder/done.mov", Some("already briefed"), false, 1_700_000_000.0);
    let entries = st.unbriefed_distinct_entries_under("/tmp/folder").unwrap();
    assert_eq!(entries.iter().map(|e| e.clip_id.clone()).collect::<Vec<_>>(), s(&["dup:1", "solo:1"]));
    assert_eq!(entries[0].sample_path, "/tmp/folder/a-copy.mov");

    let mut st = store();
    up_brief(&mut st, "live:1", "/tmp/folder/live.mov", None, false, 1_700_000_000.0);
    up_brief(&mut st, "dead:1", "/tmp/folder/dead.mov", None, true, 1_700_000_000.0);
    assert_eq!(st.unbriefed_distinct_count_under("/tmp/folder").unwrap(), 1);

    let mut st = store();
    up_brief(&mut st, "in:1", "/tmp/folder/a.mov", None, false, 1_700_000_000.0);
    up_brief(&mut st, "outside:1", "/tmp/folder-other/b.mov", None, false, 1_700_000_000.0);
    assert_eq!(st.unbriefed_distinct_count_under("/tmp/folder").unwrap(), 1);

    let mut st = store();
    for id in ["a", "b", "c"] {
        up_brief(&mut st, &format!("{id}:1"), &format!("/tmp/{id}.mov"), None, false, 1_700_000_000.0);
    }
    let ids: BTreeSet<String> = st.unbriefed_distinct_entries_in(&s(&["/tmp/a.mov", "/tmp/c.mov"])).unwrap().into_iter().map(|e| e.clip_id).collect();
    assert_eq!(ids, set(&["a:1", "c:1"]));
    assert_eq!(st.unbriefed_distinct_count_in(&[]).unwrap(), 0);
}

#[test]
fn apply_brief_fills_blanks_only_and_refreshes_fts() {
    let mut st = store();
    up_brief(&mut st, "shared:1", "/tmp/p1.mov", None, false, 1_700_000_000.0);
    up_brief(&mut st, "shared:1", "/tmp/p2.mov", Some("user-curated — keep me"), false, 1_700_000_000.0);
    up_brief(&mut st, "shared:1", "/tmp/p3.mov", Some("   "), false, 1_700_000_000.0);
    let affected: BTreeSet<String> = st.apply_brief_to_all_rows_sharing("shared:1", "bulk-generated description").unwrap().into_iter().collect();
    assert_eq!(affected, set(&["/tmp/p1.mov", "/tmp/p3.mov"]));
    assert_eq!(st.fetch_at("/tmp/p1.mov").unwrap().unwrap().brief.as_deref(), Some("bulk-generated description"));
    assert_eq!(st.fetch_at("/tmp/p2.mov").unwrap().unwrap().brief.as_deref(), Some("user-curated — keep me"));
    assert_eq!(st.fetch_at("/tmp/p3.mov").unwrap().unwrap().brief.as_deref(), Some("bulk-generated description"));
    assert!(st.apply_brief_to_all_rows_sharing("ghost:0", "x").unwrap().is_empty());

    let mut st = store();
    up_brief(&mut st, "fts:1", "/tmp/library/fts1.mov", None, false, 1_700_000_000.0);
    up_brief(&mut st, "fts:1", "/tmp/library/fts2.mov", None, false, 1_700_000_000.0);
    assert!(st.search_paths("iridescent").unwrap().is_empty());
    st.apply_brief_to_all_rows_sharing("fts:1", "iridescent oil-on-water shimmer").unwrap();
    let after: BTreeSet<String> = st.search_paths("iridescent").unwrap().into_iter().collect();
    assert_eq!(after, set(&["/tmp/library/fts1.mov", "/tmp/library/fts2.mov"]));
}

#[test]
fn overwrite_brief_variants() {
    let mut st = store();
    up_brief(&mut st, "ow:1", "/tmp/over1.mov", None, false, 1_700_000_000.0);
    up_brief(&mut st, "ow:1", "/tmp/over2.mov", Some("user-curated description"), false, 1_700_000_000.0);
    let affected: BTreeSet<String> = st.overwrite_brief_for_all_rows_sharing("ow:1", "imported").unwrap().into_iter().collect();
    assert_eq!(affected, set(&["/tmp/over1.mov", "/tmp/over2.mov"]));
    assert_eq!(st.fetch_at("/tmp/over2.mov").unwrap().unwrap().brief.as_deref(), Some("imported"));
    assert!(st.overwrite_brief_for_all_rows_sharing("ghost:0", "ignored").unwrap().is_empty());

    up_brief(&mut st, "atb:1", "/tmp/blank.mov", None, false, 1_700_000_000.0);
    assert!(st.overwrite_brief_at("/tmp/blank.mov", "filled", false).unwrap());
    assert_eq!(st.fetch_at("/tmp/blank.mov").unwrap().unwrap().brief.as_deref(), Some("filled"));
    up_brief(&mut st, "atb:2", "/tmp/exist.mov", Some("curated"), false, 1_700_000_000.0);
    assert!(!st.overwrite_brief_at("/tmp/exist.mov", "would-replace", false).unwrap());
    assert_eq!(st.fetch_at("/tmp/exist.mov").unwrap().unwrap().brief.as_deref(), Some("curated"));
    up_brief(&mut st, "atb:3", "/tmp/replace.mov", Some("old"), false, 1_700_000_000.0);
    assert!(st.overwrite_brief_at("/tmp/replace.mov", "new", true).unwrap());
    assert!(!st.overwrite_brief_at("/nowhere.mov", "x", true).unwrap());

    up_brief(&mut st, "fts-at:1", "/tmp/library/fts-at.mov", Some("stale brief"), false, 1_700_000_000.0);
    st.overwrite_brief_at("/tmp/library/fts-at.mov", "iridescent shimmer", true).unwrap();
    assert_eq!(st.search_paths("iridescent").unwrap(), s(&["/tmp/library/fts-at.mov"]));
    assert!(st.search_paths("stale").unwrap().is_empty());
}

#[test]
fn fetch_duplicates_excludes_specified_path() {
    let mut st = store();
    for p in ["/tmp/here.mov", "/tmp/there.mov", "/tmp/elsewhere.mov"] {
        st.upsert(&ClipRow::new("dup:42", p, 42, 1_700_000_000.0, 1_700_000_000.0, 1_700_000_000.0), &[]).unwrap();
    }
    let siblings: Vec<String> = st.fetch_duplicates("dup:42", "/tmp/here.mov").unwrap().into_iter().map(|r| r.current_path).collect();
    assert_eq!(siblings, s(&["/tmp/elsewhere.mov", "/tmp/there.mov"]));
}

// MARK: presets

#[test]
fn preset_crud() {
    let mut st = store();
    assert!(matches!(st.save_preset("  ", &ClipFilter::empty(), None), Err(LibraryError::Preset(PresetError::EmptyName))));
    st.save_preset(" Quick ", &ClipFilter { rating_min: Some(3), ..ClipFilter::empty() }, Some(ClipSort::RatingDesc)).unwrap();
    st.save_preset("Other", &ClipFilter::empty(), None).unwrap();
    let presets = st.list_presets().unwrap();
    assert_eq!(presets.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), s(&["Quick", "Other"]));
    assert_eq!(presets[0].sort, Some(ClipSort::RatingDesc));
    assert!(matches!(st.rename_preset("Quick", "Other"), Err(LibraryError::Preset(PresetError::DuplicateName(_)))));
    assert!(matches!(st.rename_preset("Missing", "X"), Err(LibraryError::Preset(PresetError::NotFound(_)))));
    st.rename_preset("Quick", "Quick").unwrap();
    st.rename_preset("Quick", "Renamed").unwrap();
    st.delete_preset("Other").unwrap();
    assert_eq!(st.list_presets().unwrap().iter().map(|p| p.name.clone()).collect::<Vec<_>>(), s(&["Renamed"]));
    st.connection().execute("UPDATE saved_filters SET filter_json = 'garbage', sort_value = 'nope'", []).unwrap();
    let p = &st.list_presets().unwrap()[0];
    assert_eq!((p.filter.clone(), p.sort), (ClipFilter::empty(), None), "undecodable → the empty filter, unknown sort → none");
}

// MARK: RetireOrphansTests

fn orphan_row(path: &str, id: &str, orphan: bool) -> ClipRow {
    let mut r = row_at(id, path);
    r.is_orphan = orphan;
    r
}

#[test]
fn retires_moved_from_orphan_keeps_true_loss_and_copies() {
    let mut st = store();
    st.upsert(&orphan_row("/new/x.mov", "C1", false), &[]).unwrap();
    st.upsert(&orphan_row("/old/x.mov", "C1", true), &[]).unwrap();
    assert_eq!(st.retire_orphans_with_live_twin().unwrap(), s(&["/old/x.mov"]));
    assert!(st.fetch_by_path("/old/x.mov").unwrap().is_none());
    assert!(st.fetch_by_path("/new/x.mov").unwrap().is_some());

    let mut st = store();
    st.upsert(&orphan_row("/gone/y.mov", "C2", true), &[]).unwrap();
    assert!(st.retire_orphans_with_live_twin().unwrap().is_empty());
    assert!(st.fetch_by_path("/gone/y.mov").unwrap().is_some());

    let mut st = store();
    st.upsert(&orphan_row("/a/z.mov", "C3", false), &[]).unwrap();
    st.upsert(&orphan_row("/b/z.mov", "C3", false), &[]).unwrap();
    assert!(st.retire_orphans_with_live_twin().unwrap().is_empty());
}

#[test]
fn retire_cascade_removes_retired_tags_and_keeps_survivors() {
    let mut st = store();
    st.upsert(&orphan_row("/new/x.mov", "C1", false), &s(&["livetag"])).unwrap();
    st.upsert(&orphan_row("/old/x.mov", "C1", true), &s(&["orphantag"])).unwrap();
    st.upsert(&orphan_row("/gone/y.mov", "C2", true), &s(&["losstag"])).unwrap();
    assert_eq!(st.retire_orphans_with_live_twin().unwrap(), s(&["/old/x.mov"]));
    let tags = st.all_tags().unwrap();
    assert!(!tags.contains(&"orphantag".to_string()) && tags.contains(&"livetag".to_string()) && tags.contains(&"losstag".to_string()));
    assert_eq!(st.query_paths("livetag", &ClipFilter::empty(), ClipSort::FilenameAZ).unwrap(), s(&["/new/x.mov"]));
}

// MARK: MeasuredFilterQueryTests

fn m_row(id: &str, status: &str, is_loop: Option<i64>) -> ClipMeasurement {
    let mut m = ClipMeasurement::new(id, id, status);
    m.is_loop = is_loop;
    m
}

fn measured_store() -> MetadataStore {
    let mut st = store();
    for (id, path) in [("loop", "/a.mov"), ("unchecked", "/b.mov"), ("nonloop", "/c.mov"), ("bare", "/d.mov")] {
        st.upsert(&row_at(id, path), &[]).unwrap();
    }
    let mut lp = m_row("loop", "measured", Some(1));
    lp.cuts = Some(0);
    lp.cycle_s = Some(4.0);
    lp.approx_cycle = Some(0);
    lp.join_step = Some(1.0);
    lp.text_checked = 1;
    lp.text_found = Some(0);
    lp.black_share = Some(0.9);
    lp.moves = Some(1);
    lp.pass1_score = Some(0.4);
    let mut un = m_row("unchecked", "measured", Some(1));
    un.cuts = Some(1);
    un.cycle_s = Some(12.0);
    un.approx_cycle = Some(1);
    un.join_step = Some(3.0);
    un.black_share = Some(0.1);
    un.moves = Some(0);
    un.pass1_score = Some(1.9);
    st.insert_lab_measurements(&[lp, un, m_row("nonloop", "not a loop", Some(0))]).unwrap();
    st
}

fn expect(st: &MetadataStore, f: ClipFilter, want: &[&str]) {
    assert_eq!(query(st, &f, ClipSort::FilenameAZ), s(want), "{f:?}");
}

fn flag(include: bool) -> Option<MeasuredFlag> {
    Some(MeasuredFlag::new(include))
}

#[test]
fn measured_filters() {
    let st = measured_store();
    let e = ClipFilter::empty;
    expect(&st, ClipFilter { loops_only: flag(true), ..e() }, &["loop", "unchecked", "bare"]);
    expect(&st, ClipFilter { loops_only: flag(false), ..e() }, &["loop", "unchecked"]);
    expect(&st, ClipFilter { no_hard_cuts: flag(true), ..e() }, &["loop", "nonloop", "bare"]);
    expect(&st, ClipFilter { no_hard_cuts: flag(false), ..e() }, &["loop"]);
    expect(&st, ClipFilter { no_text: flag(false), ..e() }, &["loop"]);
    expect(&st, ClipFilter { no_text: flag(true), ..e() }, &["loop", "unchecked", "nonloop", "bare"]);
    expect(&st, ClipFilter { cycle_range: Some(MeasuredValue::of((2.0, 8.0))), ..e() }, &["loop", "bare"]);
    expect(&st, ClipFilter { exact_cycles_only: flag(false), ..e() }, &["loop"]);
    expect(&st, ClipFilter { join_step_max: Some(MeasuredValue::new(2.5, false)), ..e() }, &["loop"]);
    expect(&st, ClipFilter { ground: Some(MeasuredValue::new(Ground::FullFrame, false)), ..e() }, &["unchecked"]);
    expect(&st, ClipFilter { moves_state: Some(MeasuredValue::of(MotionState::Still)), ..e() }, &["unchecked", "nonloop", "bare"]);
}

#[test]
fn measured_by_splits_lab_native_and_unmeasured() {
    let mut st = measured_store();
    let mut native = m_row("unchecked", "measured", Some(1));
    native.provenance = measurements::PROVENANCE_NATIVE.into();
    st.insert_lab_measurements(&[native]).unwrap();
    let e = ClipFilter::empty;
    expect(&st, ClipFilter { measured_by: Some(MeasuredSource::Lab), ..e() }, &["loop", "nonloop"]);
    expect(&st, ClipFilter { measured_by: Some(MeasuredSource::Native), ..e() }, &["unchecked"]);
    expect(&st, ClipFilter { measured_by: Some(MeasuredSource::NotMeasured), ..e() }, &["bare"]);
    expect(&st, ClipFilter { loops_only: flag(true), measured_by: Some(MeasuredSource::Lab), ..e() }, &["loop"]);
}

#[test]
fn loop_score_sort_puts_unmeasured_last_and_measurement_fetch() {
    let st = measured_store();
    let order = query(&st, &ClipFilter { rating_min: Some(0), ..ClipFilter::empty() }, ClipSort::LoopScoreAsc);
    assert_eq!(order[..2], s(&["loop", "unchecked"]));
    assert!(st.measurement("bare").unwrap().is_none());
    assert_eq!(st.measurement("loop").unwrap().unwrap().text_checked, 1);
}

#[test]
fn library_query_leaves_out_clips_on_unmounted_volumes() {
    let mut st =
        MetadataStore::open_with(Location::InMemory, Box::new(|| BTreeSet::from(["/Volumes/Here".to_string(), "/Volumes/Here 2".to_string()]))).unwrap();
    for (id, path) in [
        ("int", "/Users/x/a.mov"),
        ("here", "/Volumes/Here/b.mov"),
        ("here2", "/Volumes/Here 2/c.mov"),
        ("gone", "/Volumes/Gone/d.mov"),
        ("gonePrefix", "/Volumes/Here2/e.mov"),
    ] {
        let mut r = row_at(id, path);
        r.rating = 4;
        st.upsert(&r, &[]).unwrap();
    }
    let mut rated = query(&st, &ClipFilter { rating_min: Some(3), ..ClipFilter::empty() }, ClipSort::FilenameAZ);
    rated.sort();
    assert_eq!(rated, s(&["here", "here2", "int"]));
    let mut loops = query(&st, &ClipFilter { loops_only: flag(true), ..ClipFilter::empty() }, ClipSort::FilenameAZ);
    loops.sort();
    assert_eq!(loops, s(&["here", "here2", "int"]));
    let mut paths = st.query_paths("", &ClipFilter { rating_min: Some(3), ..ClipFilter::empty() }, ClipSort::FilenameAZ).unwrap();
    paths.sort();
    assert_eq!(paths, s(&["/Users/x/a.mov", "/Volumes/Here 2/c.mov", "/Volumes/Here/b.mov"]));
}

// MARK: NativeMeasurementStoreTests (store rules)

fn native_row(id: &str, status: &str, cuts: Option<i64>) -> ClipMeasurement {
    let mut m = ClipMeasurement::new(id, format!("k{id}"), status);
    m.lab_reason = Some("why".into());
    m.pass1_score = Some(0.5);
    m.is_loop = if status == "measured" { Some(1) } else { None };
    m.cuts = cuts;
    m
}

#[test]
fn provenance_migration_defaults_existing_rows_to_lab() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let ddl = measurements::create_table_sql("main").replace(",\n    provenance      TEXT NOT NULL DEFAULT 'lab'", "");
    assert!(!ddl.contains("provenance"));
    conn.execute_batch(&ddl).unwrap();
    conn.execute("INSERT INTO clip_measurements(clip_id, lab_key, lab_status) VALUES ('a', 'ka', 'measured')", []).unwrap();
    measurements::migrate(&conn, "main").unwrap();
    measurements::migrate(&conn, "main").unwrap();
    let p: String = conn.query_row("SELECT provenance FROM clip_measurements WHERE clip_id = 'a'", [], |r| r.get(0)).unwrap();
    assert_eq!(p, "lab");
    assert_eq!(measurements::measurement(&conn, "main", "a").unwrap().unwrap().provenance, "lab");
}

#[test]
fn insert_native_never_replaces_and_skips_unsupported() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    measurements::migrate(&conn, "main").unwrap();
    measurements::insert_lab(&conn, &[native_row("lab1", "measured", Some(3))]).unwrap();
    let now = || "2026-01-01T00:00:00Z".to_string();
    let n = measurements::insert_native(
        &conn,
        &[native_row("lab1", "measured", Some(0)), native_row("new1", "measured", Some(0)), native_row("x", measurements::UNSUPPORTED_STATUS, Some(0))],
        &now,
    )
    .unwrap();
    assert_eq!(n, 1);
    let kept = measurements::measurement(&conn, "main", "lab1").unwrap().unwrap();
    assert_eq!((kept.cuts, kept.provenance.as_str()), (Some(3), "lab"));
    assert_eq!(measurements::measurement(&conn, "main", "new1").unwrap().unwrap().provenance, "native");
    assert!(measurements::measurement(&conn, "main", "x").unwrap().is_none());
    let reason: String = conn.query_row("SELECT reason FROM scan_skips WHERE clip_id = 'x'", [], |r| r.get(0)).unwrap();
    assert_eq!(reason, "why");
    assert_eq!(measurements::insert_native(&conn, &[native_row("new1", "measured", Some(9))], &now).unwrap(), 0);
    assert_eq!(measurements::measurement(&conn, "main", "new1").unwrap().unwrap().cuts, Some(0));
}

#[test]
fn unmeasured_leaves_out_current_version_skips_only() {
    let mut st = store();
    for (path, id) in [("/a/png.mov", "P"), ("/a/old.mov", "O"), ("/a/plain.mov", "N"), ("/a/lab.mov", "L")] {
        st.upsert(&row_at(id, path), &[]).unwrap();
    }
    st.insert_lab_measurements(&[native_row("L", "measured", Some(3))]).unwrap();
    let ids = |st: &MetadataStore| st.unmeasured_clips().unwrap().into_iter().map(|i| i.clip_id).collect::<Vec<_>>();
    assert_eq!(ids(&st), s(&["O", "N", "P"]));
    let mut png = native_row("P", measurements::UNSUPPORTED_STATUS, Some(0));
    png.lab_reason = Some("unsupported-native: png".into());
    png.measure_version = Some(measurements::SCANNER_MEASURE_VERSION.into());
    assert!(!st.insert_native_measurement(&png).unwrap());
    assert!(st.measurement("P").unwrap().is_none());
    st.connection().execute("INSERT INTO scan_skips VALUES ('O', 'unsupported-native: qtrle', 'native v0', 'x')", []).unwrap();
    assert_eq!(ids(&st), s(&["O", "N"]));
    assert!(st.insert_native_measurement(&native_row("O", "measured", Some(0))).unwrap());
    assert_eq!(ids(&st), s(&["N"]));
}
