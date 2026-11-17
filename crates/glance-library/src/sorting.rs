//! The in-memory sort of a folder's files (`ClipSorting.apply`), shared by the Mac grid in browse mode and iCrate:
//! the dimensions a file carries, filename A→Z for the ones only the database has, and a seeded shuffle that gives
//! Swift's order for the same seed.

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
    let by_duration = |a: &T, b: &T, descending: bool| match (a.duration_sec(), b.duration_sec()) {
        (Some(x), Some(y)) if x != y => {
            let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
            if descending { o.reverse() } else { o }
        }
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        _ => by_name(a, b),
    };
    match sort {
        ClipSort::FilenameZA => out.sort_by(|a, b| by_name(b, a)),
        ClipSort::MtimeNewest | ClipSort::DateAddedDesc => out.sort_by(|a, b| b.mtime().partial_cmp(&a.mtime()).unwrap_or(Ordering::Equal)),
        ClipSort::MtimeOldest => out.sort_by(|a, b| a.mtime().partial_cmp(&b.mtime()).unwrap_or(Ordering::Equal)),
        ClipSort::Random => shuffle(&mut out, &mut SplitMix64(random_seed)),
        ClipSort::DurationAsc => out.sort_by(|a, b| by_duration(a, b, false)),
        ClipSort::DurationDesc => out.sort_by(|a, b| by_duration(a, b, true)),
        ClipSort::FilenameAZ
        | ClipSort::RatingDesc
        | ClipSort::BpmAsc
        | ClipSort::BpmDesc
        | ClipSort::EnergyAsc
        | ClipSort::EnergyDesc
        | ClipSort::LoopScoreAsc
        | ClipSort::CycleAsc
        | ClipSort::CycleDesc => out.sort_by(by_name),
    }
    out
}
