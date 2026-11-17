//! `recognise_text` on macOS: a picture with text drawn into it is read, a blank one gives nothing.
//! The text is drawn here with a 5 × 7 block font (our own glyphs), so the test needs no font file
//! and no drawing API.
#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_platform::{TextError, TextOptions, recognise_text};

const W: usize = 960;
const H: usize = 240;

/// 5 × 7 glyphs, one row per string, `#` = ink.
fn glyph(c: char) -> [&'static str; 7] {
    match c {
        'C' => [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."],
        'D' => ["###..", "#..#.", "#...#", "#...#", "#...#", "#..#.", "###.."],
        'E' => ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
        'H' => ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
        'L' => ["#....", "#....", "#....", "#....", "#....", "#....", "#####"],
        'O' => [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
        'R' => ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
        'W' => ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#."],
        _ => ["....."; 7],
    }
}

/// White picture, `text` in black, each font pixel `scale` × `scale`, starting at (`x0`, `y0`).
fn picture(text: &str, scale: usize, x0: usize, y0: usize) -> Vec<u8> {
    let mut px = vec![255u8; W * H * 4];
    for (i, c) in text.chars().enumerate() {
        for (gy, row) in glyph(c).iter().enumerate() {
            for (gx, ink) in row.chars().enumerate() {
                if ink != '#' {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let (x, y) = (x0 + (i * 6 + gx) * scale + dx, y0 + gy * scale + dy);
                        if x < W && y < H {
                            let o = (y * W + x) * 4;
                            px[o..o + 3].copy_from_slice(&[0, 0, 0]);
                        }
                    }
                }
            }
        }
    }
    px
}

#[test]
fn drawn_text_is_read_with_its_box() {
    let (scale, x0, y0) = (12, 60, 78);
    let got = recognise_text(&picture("HELLO WORLD", scale, x0, y0), W, H, &TextOptions::default()).unwrap();
    assert_eq!(got.len(), 1, "{got:?}");
    let o = &got[0];
    assert_eq!(o.text.to_uppercase().replace(' ', ""), "HELLOWORLD", "{o:?}");
    assert!(o.confidence > 0.3 && o.confidence <= 1.0, "{o:?}");
    // The ink spans x 60..840 and rows 78..162 of 240 (Vision's box: bottom-left origin, normalised).
    let (ink_x0, ink_x1) = (x0 as f64 / W as f64, (x0 + (11 * 6 - 1) * scale) as f64 / W as f64);
    let (ink_y0, ink_y1) = (1.0 - (y0 + 7 * scale) as f64 / H as f64, 1.0 - y0 as f64 / H as f64);
    assert!((o.x - ink_x0).abs() < 0.05 && (o.x + o.w - ink_x1).abs() < 0.05, "{o:?}");
    assert!((o.y - ink_y0).abs() < 0.1 && (o.y + o.h - ink_y1).abs() < 0.1, "{o:?}");
    // The same picture reads the same again (the scanner's readings rely on it).
    let again = recognise_text(&picture("HELLO WORLD", scale, x0, y0), W, H, &TextOptions::default()).unwrap();
    assert_eq!(again, got);
}

#[test]
fn a_blank_picture_has_no_text() {
    assert_eq!(recognise_text(&vec![255u8; W * H * 4], W, H, &TextOptions::default()).unwrap(), vec![]);
    assert_eq!(recognise_text(&picture("", 1, 0, 0).iter().map(|_| 0u8).collect::<Vec<_>>(), W, H, &TextOptions::default()).unwrap(), vec![]);
}

#[test]
fn a_buffer_that_does_not_match_its_size_is_refused() {
    assert_eq!(recognise_text(&[0u8; 12], 2, 2, &TextOptions::default()), Err(TextError::Image("CGImage")));
    assert_eq!(recognise_text(&[], 0, 0, &TextOptions::default()), Err(TextError::Image("CGImage")));
}

#[test]
fn an_unsupported_revision_is_vision_s_error() {
    let options = TextOptions { revision: 999, ..TextOptions::default() };
    match recognise_text(&vec![255u8; W * H * 4], W, H, &options) {
        Err(TextError::Recogniser(why)) => assert!(why.contains("Error Domain="), "{why}"),
        other => panic!("{other:?}"),
    }
}
