//! The in-memory sort of a folder's files (`ClipSorting.apply`, the Mac grid in browse mode and in a stash): the
//! dimensions a file carries, and a seeded shuffle that gives Swift's order for the same seed. Rating, BPM, energy,
//! loop score and cycle sort by their value, missing values last in both directions and ties by name, as the
//! library query orders them (Amrit, 2026-11-18: "Sorting: sort by the value."; Swift fell back to filename A→Z).

use crate::filter::ClipSort;
use crate::unicode::file_system_representation;
use std::cmp::Ordering;

/// What the browse sort reads from a file.
pub trait SortableClip {
    /// The name shown under the cell (with its extension).
    fn display_name(&self) -> &str;
    /// Modification time, seconds since 1970.
    fn mtime(&self) -> f64;
    fn duration_sec(&self) -> Option<f64>;
    /// The database's rating (`None`: no row yet).
    fn rating(&self) -> Option<i64> {
        None
    }
    fn bpm(&self) -> Option<f64> {
        None
    }
    fn energy(&self) -> Option<i64> {
        None
    }
    /// The measurement's loop score (`pass1_score`; lower is a better loop). `None`: not measured.
    fn loop_score(&self) -> Option<f64> {
        None
    }
    /// The measurement's cycle in seconds (`cycle_s`).
    fn cycle_s(&self) -> Option<f64> {
        None
    }
}

/// Values first in `descending` or ascending order, missing values last either way, ties by `tie`.
fn by_value(a: Option<f64>, b: Option<f64>, descending: bool, tie: impl FnOnce() -> Ordering) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) if x != y => {
            let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
            if descending { o.reverse() } else { o }
        }
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        _ => tie(),
    }
}

fn class(c: char) -> u8 {
    if c.is_alphabetic() {
        2
    } else if c.is_numeric() {
        1
    } else {
        0
    }
}

fn primary_key(s: &str) -> Vec<(u8, char)> {
    file_system_representation(s).chars().filter(|c| !crate::unicode::joins_previous(*c)).flat_map(char::to_lowercase).map(|c| (class(c), c)).collect()
}

/// `localizedCaseInsensitiveCompare`, approximated for file names: case and accents ignored first (punctuation
/// before digits before letters, digits not compared as numbers), then accents, then the exact text. Agrees with
/// Foundation on the names the oracle checks (`_x`, `10`, `9`, `apple`, `Banana`, `éclair`, `Zebra`, `zeta`); ICU's
/// full collation is not reproduced.
pub fn localized_case_insensitive_cmp(a: &str, b: &str) -> Ordering {
    primary_key(a).cmp(&primary_key(b)).then_with(|| file_system_representation(&a.to_lowercase()).cmp(&file_system_representation(&b.to_lowercase())))
}

/// Swift's `SplitMix64` from `ClipSorting.swift`.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `RandomNumberGenerator.next(upperBound:)` (Lemire's multiply-shift with rejection), `upper` > 0.
    fn next_below(&mut self, upper: u64) -> u64 {
        let mut m = u128::from(self.next()) * u128::from(upper);
        if (m as u64) < upper {
            let t = upper.wrapping_neg() % upper;
            while (m as u64) < t {
                m = u128::from(self.next()) * u128::from(upper);
            }
        }
        (m >> 64) as u64
    }
}

/// `MutableCollection.shuffle(using:)` (Fisher–Yates from the front, as the Swift standard library does it).
fn shuffle<T>(items: &mut [T], rng: &mut SplitMix64) {
    let mut amount = items.len();
    let mut current = 0;
    while amount > 1 {
        let random = rng.next_below(amount as u64) as usize;
        amount -= 1;
        items.swap(current, current + random);
        current += 1;
    }
}

/// `ClipSorting.apply(_:sort:randomSeed:)`.
pub fn apply<T: SortableClip + Clone>(files: &[T], sort: ClipSort, random_seed: u64) -> Vec<T> {
    let mut out = files.to_vec();
    let by_name = |a: &T, b: &T| localized_case_insensitive_cmp(a.display_name(), b.display_name());
    let int = |v: Option<i64>| v.map(|n| n as f64);
    match sort {
        ClipSort::FilenameZA => out.sort_by(|a, b| by_name(b, a)),
        ClipSort::MtimeNewest | ClipSort::DateAddedDesc => out.sort_by(|a, b| b.mtime().partial_cmp(&a.mtime()).unwrap_or(Ordering::Equal)),
        ClipSort::MtimeOldest => out.sort_by(|a, b| a.mtime().partial_cmp(&b.mtime()).unwrap_or(Ordering::Equal)),
        ClipSort::Random => shuffle(&mut out, &mut SplitMix64(random_seed)),
        ClipSort::DurationAsc => out.sort_by(|a, b| by_value(a.duration_sec(), b.duration_sec(), false, || by_name(a, b))),
        ClipSort::DurationDesc => out.sort_by(|a, b| by_value(a.duration_sec(), b.duration_sec(), true, || by_name(a, b))),
        ClipSort::RatingDesc => out.sort_by(|a, b| by_value(int(a.rating()), int(b.rating()), true, || by_name(a, b))),
        ClipSort::BpmAsc => out.sort_by(|a, b| by_value(a.bpm(), b.bpm(), false, || by_name(a, b))),
        ClipSort::BpmDesc => out.sort_by(|a, b| by_value(a.bpm(), b.bpm(), true, || by_name(a, b))),
        ClipSort::EnergyAsc => out.sort_by(|a, b| by_value(int(a.energy()), int(b.energy()), false, || by_name(a, b))),
        ClipSort::EnergyDesc => out.sort_by(|a, b| by_value(int(a.energy()), int(b.energy()), true, || by_name(a, b))),
        ClipSort::LoopScoreAsc => out.sort_by(|a, b| by_value(a.loop_score(), b.loop_score(), false, || by_name(a, b))),
        ClipSort::CycleAsc => out.sort_by(|a, b| by_value(a.cycle_s(), b.cycle_s(), false, || by_name(a, b))),
        ClipSort::CycleDesc => out.sort_by(|a, b| by_value(a.cycle_s(), b.cycle_s(), true, || by_name(a, b))),
        ClipSort::FilenameAZ => out.sort_by(by_name),
    }
    out
}
