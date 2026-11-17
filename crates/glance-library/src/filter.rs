//! The library filter, the sort choices and saved presets (`ClipFilter`, `MeasuredFlag`, `MeasuredValue`,
//! `ClipSort`, `SavedPreset`), with the JSON a preset stores in `saved_filters.filter_json`.

use crate::error::Result;
use crate::json::{self, Json};
use std::collections::BTreeSet;

/// `ClipFilter.TagLogic`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TagLogic {
    #[default]
    And,
    Or,
}

/// `ClipFilter.MotionState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionState {
    Moves,
    Still,
}

/// `ClipFilter.Ground`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    OnBlack,
    FullFrame,
}

/// `ClipFilter.MeasuredSource`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasuredSource {
    Lab,
    Native,
    NotMeasured,
}

/// An on/off measured criterion (`MeasuredFlag`): present means on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredFlag {
    /// Keep clips with no reading for this criterion (default on).
    pub include_not_measured: bool,
}

impl Default for MeasuredFlag {
    fn default() -> Self {
        MeasuredFlag { include_not_measured: true }
    }
}

impl MeasuredFlag {
    pub fn new(include_not_measured: bool) -> MeasuredFlag {
        MeasuredFlag { include_not_measured }
    }
}

/// A measured criterion with a value (`MeasuredValue<Value>`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasuredValue<T> {
    pub value: T,
    pub include_not_measured: bool,
}

impl<T> MeasuredValue<T> {
    pub fn new(value: T, include_not_measured: bool) -> MeasuredValue<T> {
        MeasuredValue { value, include_not_measured }
    }

    /// `MeasuredValue(value)` (include not measured: on).
    pub fn of(value: T) -> MeasuredValue<T> {
        MeasuredValue { value, include_not_measured: true }
    }
}

/// A closed range `lower...upper` (Swift's `ClosedRange`; `lower <= upper` holds for decoded values).
pub type Range<T> = (T, T);

/// The user's filter (`ClipFilter`); every field's "no filter" value is `None` / empty / `false`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipFilter {
    pub rating_min: Option<i64>,
    pub tags: BTreeSet<String>,
    pub tags_logic: TagLogic,
    pub bpm_range: Option<Range<f64>>,
    pub energy_range: Option<Range<i64>>,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub has_duplicates: bool,
    pub loops_only: Option<MeasuredFlag>,
    pub cycle_range: Option<MeasuredValue<Range<f64>>>,
    pub exact_cycles_only: Option<MeasuredFlag>,
    pub join_step_max: Option<MeasuredValue<f64>>,
    pub no_hard_cuts: Option<MeasuredFlag>,
    pub no_flashes: Option<MeasuredFlag>,
    pub moves_state: Option<MeasuredValue<MotionState>>,
    pub no_text: Option<MeasuredFlag>,
    pub ground: Option<MeasuredValue<Ground>>,
    pub centre_heavy: Option<MeasuredFlag>,
    pub measured_by: Option<MeasuredSource>,
}

/// The sort choices (`ClipSort`), whose raw values are stored in `saved_filters.sort_value`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipSort {
    DateAddedDesc,
    FilenameAZ,
    FilenameZA,
    MtimeNewest,
    MtimeOldest,
    RatingDesc,
    BpmAsc,
    BpmDesc,
    EnergyAsc,
    EnergyDesc,
    DurationAsc,
    DurationDesc,
    LoopScoreAsc,
    CycleAsc,
    CycleDesc,
    Random,
}

impl ClipSort {
    /// Every case, in Swift's declaration order (`allCases`).
    pub const ALL: [ClipSort; 16] = [
        ClipSort::DateAddedDesc,
        ClipSort::FilenameAZ,
        ClipSort::FilenameZA,
        ClipSort::MtimeNewest,
        ClipSort::MtimeOldest,
        ClipSort::RatingDesc,
        ClipSort::BpmAsc,
        ClipSort::BpmDesc,
        ClipSort::EnergyAsc,
        ClipSort::EnergyDesc,
        ClipSort::DurationAsc,
        ClipSort::DurationDesc,
        ClipSort::LoopScoreAsc,
        ClipSort::CycleAsc,
        ClipSort::CycleDesc,
        ClipSort::Random,
    ];

    /// The stored raw value.
    pub fn raw_value(self) -> &'static str {
        match self {
            ClipSort::DateAddedDesc => "dateAddedDesc",
            ClipSort::FilenameAZ => "filenameAZ",
            ClipSort::FilenameZA => "filenameZA",
            ClipSort::MtimeNewest => "mtimeNewest",
            ClipSort::MtimeOldest => "mtimeOldest",
            ClipSort::RatingDesc => "ratingDesc",
            ClipSort::BpmAsc => "bpmAsc",
            ClipSort::BpmDesc => "bpmDesc",
            ClipSort::EnergyAsc => "energyAsc",
            ClipSort::EnergyDesc => "energyDesc",
            ClipSort::DurationAsc => "durationAsc",
            ClipSort::DurationDesc => "durationDesc",
            ClipSort::LoopScoreAsc => "loopScoreAsc",
            ClipSort::CycleAsc => "cycleAsc",
            ClipSort::CycleDesc => "cycleDesc",
            ClipSort::Random => "random",
        }
    }

    /// `ClipSort(rawValue:)`.
    pub fn from_raw_value(raw: &str) -> Option<ClipSort> {
        ClipSort::ALL.into_iter().find(|s| s.raw_value() == raw)
    }

    /// The sort menu's label (`displayName`).
    pub fn display_name(self) -> &'static str {
        match self {
            ClipSort::DateAddedDesc => "Date added",
            ClipSort::FilenameAZ => "Filename (A \u{2192} Z)",
            ClipSort::FilenameZA => "Filename (Z \u{2192} A)",
            ClipSort::MtimeNewest => "File date (newest)",
            ClipSort::MtimeOldest => "File date (oldest)",
            ClipSort::RatingDesc => "Rating (high to low)",
            ClipSort::BpmAsc => "BPM (low to high)",
            ClipSort::BpmDesc => "BPM (high to low)",
            ClipSort::EnergyAsc => "Energy (low to high)",
            ClipSort::EnergyDesc => "Energy (high to low)",
            ClipSort::DurationAsc => "Length (short to long)",
            ClipSort::DurationDesc => "Length (long to short)",
            ClipSort::LoopScoreAsc => "Loop score (best first)",
            ClipSort::CycleAsc => "Cycle (short to long)",
            ClipSort::CycleDesc => "Cycle (long to short)",
            ClipSort::Random => "Random",
        }
    }

    /// Orders by a measurements column, so the query needs the measurements join.
    pub fn uses_measurements(self) -> bool {
        matches!(self, ClipSort::LoopScoreAsc | ClipSort::CycleAsc | ClipSort::CycleDesc)
    }
}

/// One saved preset (`SavedPreset`).
#[derive(Clone, Debug, PartialEq)]
pub struct SavedPreset {
    pub name: String,
    pub filter: ClipFilter,
    pub sort: Option<ClipSort>,
    /// Seconds since 1970.
    pub created_at: f64,
}

impl TagLogic {
    fn raw(self) -> &'static str {
        match self {
            TagLogic::And => "and",
            TagLogic::Or => "or",
        }
    }
}

impl MotionState {
    pub fn raw_value(self) -> &'static str {
        match self {
            MotionState::Moves => "moves",
            MotionState::Still => "still",
        }
    }
}

impl Ground {
    pub fn raw_value(self) -> &'static str {
        match self {
            Ground::OnBlack => "onBlack",
            Ground::FullFrame => "fullFrame",
        }
    }
}

impl MeasuredSource {
    pub fn raw_value(self) -> &'static str {
        match self {
            MeasuredSource::Lab => "lab",
            MeasuredSource::Native => "native",
            MeasuredSource::NotMeasured => "notMeasured",
        }
    }
}

fn flag_json(f: &MeasuredFlag) -> Json {
    Json::object([("includeNotMeasured", Json::Bool(f.include_not_measured))])
}

fn value_json<T>(v: &MeasuredValue<T>, value: Json) -> Json {
    Json::object([("value", value), ("includeNotMeasured", Json::Bool(v.include_not_measured))])
}

impl ClipFilter {
    /// `ClipFilter.empty`.
    pub fn empty() -> ClipFilter {
        ClipFilter::default()
    }

    /// No dimension set.
    pub fn is_empty(&self) -> bool {
        self.rating_min.is_none()
            && self.tags.is_empty()
            && self.bpm_range.is_none()
            && self.energy_range.is_none()
            && self.mood.is_none()
            && self.genre.is_none()
            && !self.has_duplicates
            && !self.has_measured_criteria()
    }

    /// Any measured criterion on (the query then joins the measurements).
    pub fn has_measured_criteria(&self) -> bool {
        self.loops_only.is_some()
            || self.cycle_range.is_some()
            || self.exact_cycles_only.is_some()
            || self.join_step_max.is_some()
            || self.no_hard_cuts.is_some()
            || self.no_flashes.is_some()
            || self.moves_state.is_some()
            || self.no_text.is_some()
            || self.ground.is_some()
            || self.centre_heavy.is_some()
            || self.measured_by.is_some()
    }

    /// `preferredSort`: rating, then BPM, energy, tags, mood or genre decide; `None` when nothing does.
    pub fn preferred_sort(&self) -> Option<ClipSort> {
        if self.rating_min.is_some() {
            return Some(ClipSort::RatingDesc);
        }
        if self.bpm_range.is_some() {
            return Some(ClipSort::BpmAsc);
        }
        if self.energy_range.is_some() {
            return Some(ClipSort::EnergyDesc);
        }
        if !self.tags.is_empty() || self.mood.is_some() || self.genre.is_some() {
            return Some(ClipSort::DateAddedDesc);
        }
        None
    }

    /// The JSON a plain `JSONEncoder` writes (keys in `CodingKeys` order here; Swift's order varies run to run, so
    /// compare parsed). Unset optionals are left out, `hasDuplicates` only when true.
    pub fn to_json(&self) -> Result<String> {
        let mut pairs: Vec<(String, Json)> = Vec::new();
        let mut put = |k: &str, v: Json| pairs.push((k.to_string(), v));
        if let Some(r) = self.rating_min {
            put("ratingMin", Json::Int(r));
        }
        put("tags", Json::Array(self.tags.iter().cloned().map(Json::String).collect()));
        put("tagsLogic", Json::string(self.tags_logic.raw()));
        if let Some((lo, hi)) = self.bpm_range {
            put("bpmRange", Json::Array(vec![Json::Double(lo), Json::Double(hi)]));
        }
        if let Some((lo, hi)) = self.energy_range {
            put("energyRange", Json::Array(vec![Json::Int(lo), Json::Int(hi)]));
        }
        if let Some(m) = &self.mood {
            put("mood", Json::string(m.clone()));
        }
        if let Some(g) = &self.genre {
            put("genre", Json::string(g.clone()));
        }
        if self.has_duplicates {
            put("hasDuplicates", Json::Bool(true));
        }
        if let Some(f) = &self.loops_only {
            put("loopsOnly", flag_json(f));
        }
        if let Some(v) = &self.cycle_range {
            put("cycleRange", value_json(v, Json::Array(vec![Json::Double(v.value.0), Json::Double(v.value.1)])));
        }
        if let Some(f) = &self.exact_cycles_only {
            put("exactCyclesOnly", flag_json(f));
        }
        if let Some(v) = &self.join_step_max {
            put("joinStepMax", value_json(v, Json::Double(v.value)));
        }
        if let Some(f) = &self.no_hard_cuts {
            put("noHardCuts", flag_json(f));
        }
        if let Some(f) = &self.no_flashes {
            put("noFlashes", flag_json(f));
        }
        if let Some(v) = &self.moves_state {
            put("movesState", value_json(v, Json::string(v.value.raw_value())));
        }
        if let Some(f) = &self.no_text {
            put("noText", flag_json(f));
        }
        if let Some(v) = &self.ground {
            put("ground", value_json(v, Json::string(v.value.raw_value())));
        }
        if let Some(f) = &self.centre_heavy {
            put("centreHeavy", flag_json(f));
        }
        if let Some(m) = self.measured_by {
            put("measuredBy", Json::string(m.raw_value()));
        }
        Ok(json::to_compact(&Json::Object(pairs))?)
    }

    /// `JSONDecoder().decode(ClipFilter.self, …)`: every key optional (absent or `null`); any wrong type, unknown
    /// enum value or reversed range refuses the whole filter (`None`; a preset then falls back to the empty filter).
    pub fn from_json(bytes: &[u8]) -> Option<ClipFilter> {
        let doc = json::parse(bytes)?;
        if !matches!(doc, Json::Object(_)) {
            return None;
        }
        // `decodeIfPresent`: Ok(None) when absent or null, Err(()) when present but wrong.
        fn opt<T>(doc: &Json, key: &str, f: impl Fn(&Json) -> Option<T>) -> std::result::Result<Option<T>, ()> {
            doc.present(key).map(|v| f(v).ok_or(())).transpose()
        }
        fn range<T: PartialOrd>(v: &Json, f: impl Fn(&Json) -> Option<T>) -> Option<(T, T)> {
            let items = v.as_array()?;
            let lo = f(items.first()?)?;
            let hi = f(items.get(1)?)?;
            (lo <= hi).then_some((lo, hi))
        }
        fn include(v: &Json) -> Option<bool> {
            match v.present("includeNotMeasured") {
                None => Some(true),
                Some(b) => b.as_bool(),
            }
        }
        fn flag(v: &Json) -> Option<MeasuredFlag> {
            matches!(v, Json::Object(_)).then_some(())?;
            Some(MeasuredFlag { include_not_measured: include(v)? })
        }
        fn value<T>(v: &Json, f: impl Fn(&Json) -> Option<T>) -> Option<MeasuredValue<T>> {
            Some(MeasuredValue { value: f(v.present("value")?)?, include_not_measured: include(v)? })
        }
        let string = |v: &Json| v.as_str().map(str::to_string);
        let decoded = (|| -> std::result::Result<ClipFilter, ()> {
            Ok(ClipFilter {
                rating_min: opt(&doc, "ratingMin", Json::as_i64)?,
                tags: opt(&doc, "tags", |v| v.as_array()?.iter().map(|t| t.as_str().map(str::to_string)).collect())?.unwrap_or_default(),
                tags_logic: opt(&doc, "tagsLogic", |v| match v.as_str()? {
                    "and" => Some(TagLogic::And),
                    "or" => Some(TagLogic::Or),
                    _ => None,
                })?
                .unwrap_or_default(),
                bpm_range: opt(&doc, "bpmRange", |v| range(v, Json::as_f64))?,
                energy_range: opt(&doc, "energyRange", |v| range(v, Json::as_i64))?,
                mood: opt(&doc, "mood", string)?,
                genre: opt(&doc, "genre", string)?,
                has_duplicates: opt(&doc, "hasDuplicates", Json::as_bool)?.unwrap_or(false),
                loops_only: opt(&doc, "loopsOnly", flag)?,
                cycle_range: opt(&doc, "cycleRange", |v| value(v, |r| range(r, Json::as_f64)))?,
                exact_cycles_only: opt(&doc, "exactCyclesOnly", flag)?,
                join_step_max: opt(&doc, "joinStepMax", |v| value(v, Json::as_f64))?,
                no_hard_cuts: opt(&doc, "noHardCuts", flag)?,
                no_flashes: opt(&doc, "noFlashes", flag)?,
                moves_state: opt(&doc, "movesState", |v| {
                    value(v, |s| match s.as_str()? {
                        "moves" => Some(MotionState::Moves),
                        "still" => Some(MotionState::Still),
                        _ => None,
                    })
                })?,
                no_text: opt(&doc, "noText", flag)?,
                ground: opt(&doc, "ground", |v| {
                    value(v, |s| match s.as_str()? {
                        "onBlack" => Some(Ground::OnBlack),
                        "fullFrame" => Some(Ground::FullFrame),
                        _ => None,
                    })
                })?,
                centre_heavy: opt(&doc, "centreHeavy", flag)?,
                measured_by: opt(&doc, "measuredBy", |v| match v.as_str()? {
                    "lab" => Some(MeasuredSource::Lab),
                    "native" => Some(MeasuredSource::Native),
                    "notMeasured" => Some(MeasuredSource::NotMeasured),
                    _ => None,
                })?,
            })
        })();
        decoded.ok()
    }
}
