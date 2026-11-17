//! The composite library query (`MetadataStore.buildQueryPlan`): full-text search, every filter dimension and the
//! sort, as one SQL statement, its text identical to Swift MaCrate's.

use crate::filter::{ClipFilter, ClipSort, Ground, MeasuredSource, MotionState, TagLogic};
use crate::measurements::{CENTRE_HEAVY, ON_BLACK, PROVENANCE_LAB, PROVENANCE_NATIVE, SCHEMA_NAME};
use crate::search::sanitize_search_query;
use crate::thumbkey::swift_double;
use crate::unicode::trim_whitespaces_and_newlines;
use rusqlite::ToSql;
use rusqlite::types::ToSqlOutput;
use std::collections::BTreeSet;

/// One bound argument of a plan.
#[derive(Clone, Debug, PartialEq)]
pub enum SqlArg {
    Int(i64),
    Real(f64),
    Text(String),
}

impl ToSql for SqlArg {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            SqlArg::Int(i) => i.to_sql(),
            SqlArg::Real(r) => r.to_sql(),
            SqlArg::Text(t) => t.to_sql(),
        }
    }
}

/// The statement and its arguments (`MetadataStore.QueryPlan`).
#[derive(Clone, Debug, PartialEq)]
pub struct QueryPlan {
    pub sql: String,
    pub args: Vec<SqlArg>,
}

/// `orderByClause(for:)`: NULLs last in both directions for the nullable dimensions; `random()` reseeds per query.
pub fn order_by_clause(sort: ClipSort) -> &'static str {
    match sort {
        ClipSort::DateAddedDesc => "c.created_at DESC, c.clip_id ASC",
        ClipSort::FilenameAZ => "LOWER(c.current_path) ASC, c.clip_id ASC",
        ClipSort::FilenameZA => "LOWER(c.current_path) DESC, c.clip_id DESC",
        ClipSort::MtimeNewest => "c.file_mtime DESC, c.clip_id ASC",
        ClipSort::MtimeOldest => "c.file_mtime ASC, c.clip_id ASC",
        ClipSort::RatingDesc => "c.rating DESC, c.clip_id ASC",
        ClipSort::BpmAsc => "c.bpm IS NULL, c.bpm ASC, c.clip_id ASC",
        ClipSort::BpmDesc => "c.bpm IS NULL, c.bpm DESC, c.clip_id DESC",
        ClipSort::EnergyAsc => "c.energy IS NULL, c.energy ASC, c.clip_id ASC",
        ClipSort::EnergyDesc => "c.energy IS NULL, c.energy DESC, c.clip_id DESC",
        ClipSort::DurationAsc => "c.duration_sec IS NULL, c.duration_sec ASC, c.clip_id ASC",
        ClipSort::DurationDesc => "c.duration_sec IS NULL, c.duration_sec DESC, c.clip_id DESC",
        ClipSort::LoopScoreAsc => "m.pass1_score IS NULL, m.pass1_score ASC, c.clip_id ASC",
        ClipSort::CycleAsc => "m.cycle_s IS NULL, m.cycle_s ASC, c.clip_id ASC",
        ClipSort::CycleDesc => "m.cycle_s IS NULL, m.cycle_s DESC, c.clip_id DESC",
        ClipSort::Random => "random()",
    }
}

fn append_measured(filter: &ClipFilter, wheres: &mut Vec<String>, args: &mut Vec<SqlArg>) {
    fn add(wheres: &mut Vec<String>, include: bool, not_measured: &str, predicate: &str) {
        wheres.push(if include { format!("({not_measured} OR {predicate})") } else { predicate.to_string() });
    }
    let no_verdict = "m.is_loop IS NULL";
    if let Some(f) = &filter.loops_only {
        add(wheres, f.include_not_measured, no_verdict, "m.is_loop = 1");
    }
    if let Some(f) = &filter.cycle_range {
        add(wheres, f.include_not_measured, no_verdict, "m.cycle_s BETWEEN ? AND ?");
        args.push(SqlArg::Real(f.value.0));
        args.push(SqlArg::Real(f.value.1));
    }
    if let Some(f) = &filter.exact_cycles_only {
        add(wheres, f.include_not_measured, no_verdict, "m.approx_cycle = 0");
    }
    if let Some(f) = &filter.join_step_max {
        add(wheres, f.include_not_measured, no_verdict, "m.join_step <= ?");
        args.push(SqlArg::Real(f.value));
    }
    if let Some(f) = &filter.no_hard_cuts {
        add(wheres, f.include_not_measured, "m.cuts IS NULL", "m.cuts = 0");
    }
    if let Some(f) = &filter.no_flashes {
        add(wheres, f.include_not_measured, "m.flashes IS NULL", "m.flashes = 0");
    }
    if let Some(f) = &filter.moves_state {
        let predicate = if f.value == MotionState::Moves { "m.moves = 1" } else { "m.moves = 0" };
        add(wheres, f.include_not_measured, "m.moves IS NULL", predicate);
    }
    if let Some(f) = &filter.no_text {
        add(wheres, f.include_not_measured, "(m.text_checked IS NULL OR m.text_checked = 0)", "(m.text_checked = 1 AND m.text_found = 0)");
    }
    if let Some(f) = &filter.ground {
        let threshold = swift_double(ON_BLACK);
        let predicate = if f.value == Ground::OnBlack { format!("m.black_share >= {threshold}") } else { format!("m.black_share < {threshold}") };
        add(wheres, f.include_not_measured, "m.black_share IS NULL", &predicate);
    }
    if let Some(f) = &filter.centre_heavy {
        add(wheres, f.include_not_measured, "m.centre_share IS NULL", &format!("m.centre_share >= {}", swift_double(CENTRE_HEAVY)));
    }
    match filter.measured_by {
        Some(MeasuredSource::Lab) => {
            wheres.push("m.provenance = ?".to_string());
            args.push(SqlArg::Text(PROVENANCE_LAB.to_string()));
        }
        Some(MeasuredSource::Native) => {
            wheres.push("m.provenance = ?".to_string());
            args.push(SqlArg::Text(PROVENANCE_NATIVE.to_string()));
        }
        Some(MeasuredSource::NotMeasured) => wheres.push("m.clip_id IS NULL".to_string()),
        None => {}
    }
}

/// `MetadataStore.buildQueryPlan`: selects `c.clip_id`. `mounted_volumes` `None` leaves the mount check out (Swift's
/// plan-shape tests); otherwise rows under `/Volumes/<name>` pass only for a mounted `<name>`.
pub fn build_query_plan(search: &str, filter: &ClipFilter, sort: ClipSort, mounted_volumes: Option<&BTreeSet<String>>) -> QueryPlan {
    let mut joins: Vec<String> = Vec::new();
    let mut wheres: Vec<String> = vec!["c.is_orphan = 0".to_string()];
    let mut args: Vec<SqlArg> = Vec::new();

    if let Some(volumes) = mounted_volumes {
        let mut keep = vec!["c.current_path < '/Volumes/'".to_string(), "c.current_path >= '/Volumes0'".to_string()];
        for volume in volumes {
            keep.push("(c.current_path >= ? AND c.current_path < ?)".to_string());
            args.push(SqlArg::Text(format!("{volume}/")));
            args.push(SqlArg::Text(format!("{volume}0")));
        }
        wheres.push(format!("({})", keep.join(" OR ")));
    }

    let raw_trimmed = trim_whitespaces_and_newlines(search);
    let expression = sanitize_search_query(search);
    if !expression.is_empty() {
        joins.push("JOIN clip_search s ON s.current_path = c.current_path".to_string());
        wheres.push("clip_search MATCH ?".to_string());
        args.push(SqlArg::Text(expression));
    } else if !raw_trimmed.is_empty() {
        // Typed, but nothing searchable: no rows, not every row.
        wheres.push("0".to_string());
    }

    if let Some(r) = filter.rating_min {
        wheres.push("c.rating >= ?".to_string());
        args.push(SqlArg::Int(r));
    }
    if let Some((lo, hi)) = filter.bpm_range {
        wheres.push("c.bpm BETWEEN ? AND ?".to_string());
        args.push(SqlArg::Real(lo));
        args.push(SqlArg::Real(hi));
    }
    if let Some((lo, hi)) = filter.energy_range {
        wheres.push("c.energy BETWEEN ? AND ?".to_string());
        args.push(SqlArg::Int(lo));
        args.push(SqlArg::Int(hi));
    }
    if let Some(mood) = filter.mood.as_ref().filter(|m| !m.is_empty()) {
        wheres.push("c.mood = ?".to_string());
        args.push(SqlArg::Text(mood.clone()));
    }
    if let Some(genre) = filter.genre.as_ref().filter(|g| !g.is_empty()) {
        wheres.push("c.genre = ?".to_string());
        args.push(SqlArg::Text(genre.clone()));
    }
    if filter.has_duplicates {
        wheres.push("c.clip_id IN (\n    SELECT clip_id FROM clips\n    GROUP BY clip_id\n    HAVING COUNT(*) > 1\n)".to_string());
    }
    if !filter.tags.is_empty() {
        let tags: Vec<&String> = filter.tags.iter().collect();
        let placeholders = vec!["?"; tags.len()].join(", ");
        match filter.tags_logic {
            TagLogic::And => {
                wheres.push(format!(
                    "c.current_path IN (\n    SELECT current_path FROM clip_tags\n    WHERE tag IN ({placeholders})\n    GROUP BY current_path\n    HAVING COUNT(DISTINCT tag) = ?\n)"
                ));
                args.extend(tags.iter().map(|t| SqlArg::Text((*t).clone())));
                args.push(SqlArg::Int(tags.len() as i64));
            }
            TagLogic::Or => {
                wheres.push(format!("c.current_path IN (\n    SELECT current_path FROM clip_tags WHERE tag IN ({placeholders})\n)"));
                args.extend(tags.iter().map(|t| SqlArg::Text((*t).clone())));
            }
        }
    }

    append_measured(filter, &mut wheres, &mut args);
    if filter.has_measured_criteria() || sort.uses_measurements() {
        joins.push(format!("LEFT JOIN {SCHEMA_NAME}.clip_measurements m ON m.clip_id = c.clip_id"));
    }

    let join_clause = if joins.is_empty() { String::new() } else { format!("{} ", joins.join(" ")) };
    let sql = format!("SELECT c.clip_id FROM clips c {join_clause}WHERE {} ORDER BY {}", wheres.join(" AND "), order_by_clause(sort));
    QueryPlan { sql, args }
}

/// `buildQueryPlanForPaths`: the same plan selecting `c.current_path`.
pub fn build_query_plan_for_paths(search: &str, filter: &ClipFilter, sort: ClipSort, mounted_volumes: Option<&BTreeSet<String>>) -> QueryPlan {
    let base = build_query_plan(search, filter, sort, mounted_volumes);
    QueryPlan { sql: base.sql.replace("SELECT c.clip_id FROM clips c", "SELECT c.current_path FROM clips c"), args: base.args }
}
