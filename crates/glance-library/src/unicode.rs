//! The few pieces of Foundation's Unicode handling that reach MaCrate's files, from tables generated out of
//! Foundation itself (`unicode_tables.rs`):
//!
//! - [`file_system_representation`]: what `URL(fileURLWithPath:)` does to a path on macOS. Swift MaCrate's
//!   `url.path`, `lastPathComponent` and every sidecar URL go through it, so the FTS `filename` column and the
//!   sidecar names it writes are in this form: the HFS+ decomposition (canonical decomposition, except the ranges
//!   U+2000–U+2FFF, U+F900–U+FAFF and U+2F800–U+2FAFF, from an older Unicode table), marks put in canonical order.
//! - `CharacterSet.alphanumerics` and `.whitespacesAndNewlines` membership, and `trimmingCharacters(in:)`.
//! - The two grapheme rules the search sanitiser's `split` depends on.

use crate::unicode_tables::{ALPHANUMERICS, COMBINING_CLASS, FS_DECOMPOSITION, JOINS_PREVIOUS, PREPEND, WHITESPACES_AND_NEWLINES};

fn in_ranges(table: &[(u32, u32)], c: char) -> bool {
    let v = c as u32;
    table
        .binary_search_by(|&(lo, hi)| {
            if hi < v {
                std::cmp::Ordering::Less
            } else if lo > v {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// `CharacterSet.alphanumerics.contains(c)` (general categories L*, M*, N*, as this Foundation has them).
pub fn is_alphanumeric(c: char) -> bool {
    in_ranges(ALPHANUMERICS, c)
}

/// `CharacterSet.whitespacesAndNewlines.contains(c)`.
pub fn is_whitespace_or_newline(c: char) -> bool {
    in_ranges(WHITESPACES_AND_NEWLINES, c)
}

/// `s.trimmingCharacters(in: .whitespacesAndNewlines)`.
pub fn trim_whitespaces_and_newlines(s: &str) -> &str {
    s.trim_matches(is_whitespace_or_newline)
}

/// A scalar that never begins a new `Character` after a letter or a space (grapheme Extend, SpacingMark, ZWJ).
pub(crate) fn joins_previous(c: char) -> bool {
    in_ranges(JOINS_PREVIOUS, c)
}

/// A scalar after which a space does not begin a new `Character` (grapheme Prepend).
pub(crate) fn is_prepend(c: char) -> bool {
    in_ranges(PREPEND, c)
}

fn combining_class(c: char) -> u8 {
    let v = c as u32;
    match COMBINING_CLASS.binary_search_by(|&(lo, hi, _)| {
        if hi < v {
            std::cmp::Ordering::Less
        } else if lo > v {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }) {
        Ok(i) => COMBINING_CLASS.get(i).map_or(0, |e| e.2),
        Err(_) => 0,
    }
}

const S_BASE: u32 = 0xAC00;
const L_BASE: u32 = 0x1100;
const V_BASE: u32 = 0x1161;
const T_BASE: u32 = 0x11A7;
const T_COUNT: u32 = 28;
const N_COUNT: u32 = 21 * T_COUNT;
const S_COUNT: u32 = 19 * N_COUNT;

fn push_decomposed(out: &mut Vec<char>, c: char) {
    let v = c as u32;
    if (S_BASE..S_BASE + S_COUNT).contains(&v) {
        let s = v - S_BASE;
        for part in [L_BASE + s / N_COUNT, V_BASE + (s % N_COUNT) / T_COUNT] {
            out.extend(char::from_u32(part));
        }
        if !s.is_multiple_of(T_COUNT) {
            out.extend(char::from_u32(T_BASE + s % T_COUNT));
        }
        return;
    }
    match FS_DECOMPOSITION.binary_search_by_key(&v, |&(k, _)| k) {
        Ok(i) => match FS_DECOMPOSITION.get(i) {
            Some((_, parts)) => out.extend(parts.iter().filter_map(|&p| char::from_u32(p))),
            None => out.push(c),
        },
        Err(_) => out.push(c),
    }
}

/// The path (or name) as `URL(fileURLWithPath:)` hands it back on macOS: each scalar decomposed as HFS+ does, and
/// combining marks put in canonical order (a stable sort by combining class) within each stretch that begins at an
/// input scalar of class 0 — a starter's decomposition is not reordered against the marks before it, as
/// CoreFoundation does it (`a\u{316}\u{F73}` keeps U+0316 first).
pub fn file_system_representation(s: &str) -> String {
    if s.is_ascii() {
        return s.to_string();
    }
    let mut out: Vec<char> = Vec::with_capacity(s.len());
    let mut segment: Vec<char> = Vec::new();
    for c in s.chars() {
        if combining_class(c) == 0 {
            flush_segment(&mut segment, &mut out);
        }
        push_decomposed(&mut segment, c);
    }
    flush_segment(&mut segment, &mut out);
    out.into_iter().collect()
}

fn flush_segment(segment: &mut Vec<char>, out: &mut Vec<char>) {
    let mut i = 0;
    while i < segment.len() {
        if segment.get(i).map_or(0, |&c| combining_class(c)) == 0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < segment.len() && segment.get(i).map_or(0, |&c| combining_class(c)) != 0 {
            i += 1;
        }
        if let Some(run) = segment.get_mut(start..i) {
            run.sort_by_key(|&c| combining_class(c));
        }
    }
    out.append(segment);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposition_and_order() {
        assert_eq!(file_system_representation("caf\u{e9}.mov"), "cafe\u{301}.mov");
        assert_eq!(file_system_representation("\u{ac00}\u{ac01}"), "\u{1100}\u{1161}\u{1100}\u{1161}\u{11a8}");
        assert_eq!(file_system_representation("e\u{301}\u{323}"), "e\u{323}\u{301}");
        assert_eq!(file_system_representation("\u{212b}\u{f900}"), "\u{212b}\u{f900}");
        assert!(is_alphanumeric('\u{301}') && !is_alphanumeric('_') && is_alphanumeric('\u{2163}'));
        assert_eq!(trim_whitespaces_and_newlines("\u{200b}a\u{feff}"), "a\u{feff}");
    }
}
