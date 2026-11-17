//! The thumbnail cache's key and picture sizes, as Swift MaCrate has them, so the Rust MaCrate can keep using the
//! JPEGs Swift MaCrate left in `~/Library/Caches/com.amritus.crate/thumbs/<key>.jpg`.
//!
//! - [`thumbnail_key`]: `ThumbnailCache.cacheKey` (CrateKit/ThumbnailCache.swift:83-92) — the first 16 hex digits
//!   of `SHA-256(UTF-8("<path>:<size>:<mtime>"))`, the mtime written as Swift's `"\(Double)"` ([`swift_double`]).
//! - [`downscaled_size`]: `ThumbnailDownscale.downscaleIfNeeded`'s target (long edge at most 1280, rounded half away
//!   from zero, never below 1).
//! - [`checkerboard_tile`]: the tile of the checkerboard baked under a thumbnail
//!   (`ThumbnailExtractor.compositeOntoCheckerboard`: `max(8, max(w, h) / 10)`).

use sha2::{Digest, Sha256};
use std::path::Path;

/// The longest edge a cached thumbnail keeps (`ThumbnailDownscale.defaultLongEdge`).
pub const THUMBNAIL_LONG_EDGE: u32 = 1280;

/// The cache key of a clip at `path` (the path string Swift's `url.path` gives, as MaCrate stores it in
/// `clips.current_path`), `size` bytes, modified `mtime` seconds after 1970 (Foundation's `Date`, see
/// [`crate::enumerate::foundation_mtime`]).
pub fn thumbnail_key(path: &str, size: i64, mtime: f64) -> String {
    let composite = format!("{path}:{size}:{}", swift_double(mtime));
    let digest = Sha256::digest(composite.as_bytes());
    let mut key = String::with_capacity(16);
    for b in digest.iter().take(8) {
        key.push(char::from(HEX[usize::from(b >> 4)]));
        key.push(char::from(HEX[usize::from(b & 15)]));
    }
    key
}

/// [`thumbnail_key`] of `cacheKey(for: URL(fileURLWithPath: path))`: the size and modification date read as Swift
/// reads them (`FileManager.attributesOfItem`, which does not follow a symbolic link), and the path in the form a
/// URL built from a path string reports — Apple's file-system decomposition ([`crate::unicode::file_system_representation`]). That is
/// how Swift MaCrate keys a clip it knows by its stored `current_path` (library mode); a clip it found by listing a
/// folder is keyed by the listing's own path, the on-disk form ([`crate::enumerate::ClipFile::path`]) — call
/// [`thumbnail_key`] with that. The two differ only for a name stored composed (NFC) with non-ASCII letters, and
/// Swift's cache then holds both. `None` when the attributes cannot be read or the path is not UTF-8.
pub fn thumbnail_key_for(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    let size = i64::try_from(meta.len()).ok()?;
    Some(thumbnail_key(&crate::unicode::file_system_representation(path.to_str()?), size, crate::enumerate::foundation_mtime(&meta)?))
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Swift's `Double.description` (what `"\(x)"` writes), from the Swift runtime's `swift_dtoa_optimal_double`: the
/// shortest digits that read back as `x` (Rust's shortest round-trip digits), then
/// - exponential `d.ddde±XX` (at least two exponent digits, no `.` after a single digit) when `|x| > 2^53` or
///   `x < 10^-4` in magnitude (decimal exponent below -3): `1e+16`, `9.007199254740994e+15`, `1e-05`, `5e-324`;
/// - decimal otherwise, always with a fraction: `1731600000.0`, `9007199254740992.0`, `0.0001`;
/// - `0.0` / `-0.0`, `inf` / `-inf`, and `nan` for every NaN (sign and payload ignored, measured).
///
/// When `x` is exactly halfway between two shortest candidates Swift keeps the one ending in an even digit.
pub fn swift_double(x: f64) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x.is_infinite() {
        return format!("{sign}inf");
    }
    if x == 0.0 {
        return format!("{sign}0.0");
    }
    // Rust's `{:e}` writes the shortest round-trip digits: `d[.ddd]e<exp>`.
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let mut digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exp: i32 = exp.parse().unwrap_or(0);
    tie_to_even(x.abs(), exp, &mut digits);
    // Swift counts the exponent of 0.ddd: x = 0.d1d2… × 10^decimal_exponent.
    let decimal_exponent = exp + 1;
    if decimal_exponent < -3 || x.abs() > 9_007_199_254_740_992.0 {
        let (first, rest) = digits.split_at(1.min(digits.len()));
        let dot = if rest.is_empty() { String::new() } else { format!(".{rest}") };
        let esign = if exp < 0 { '-' } else { '+' };
        return format!("{sign}{first}{dot}e{esign}{:02}", exp.unsigned_abs());
    }
    let n = digits.len() as i32;
    if decimal_exponent <= 0 {
        let zeros = "0".repeat(decimal_exponent.unsigned_abs() as usize);
        format!("{sign}0.{zeros}{digits}")
    } else if decimal_exponent < n {
        let (int, frac) = digits.split_at(decimal_exponent as usize);
        format!("{sign}{int}.{frac}")
    } else {
        let zeros = "0".repeat((decimal_exponent - n) as usize);
        format!("{sign}{digits}{zeros}.0")
    }
}

/// When `x` lies exactly halfway between two shortest candidates, Rust's digits take the upper one and Swift's the
/// one whose last digit is even (`1165512598289727.25` is `…727.2` in Swift): step `digits` down to Swift's.
fn tie_to_even(x: f64, exp: i32, digits: &mut String) {
    let Some(last) = digits.bytes().last() else { return };
    if last % 2 == 0 {
        return;
    }
    let mut down = digits.clone();
    down.pop();
    down.push(char::from(last - 1));
    // The exact decimal expansion (a double has at most 767 significant digits).
    let exact = format!("{x:.800e}");
    let Some((mantissa, e)) = exact.split_once('e') else { return };
    if e.parse::<i32>().ok() != Some(exp) {
        return;
    }
    let all: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let tail = all.get(down.len()..).unwrap_or("");
    // Only a true tie, and only when the lower candidate still reads back as `x` (at a power of two the gap
    // below is half the gap above: 2^-24 stays `5.960464477539063e-08`).
    let (first, rest) = down.split_at(1);
    let reads_back = format!("{first}.{rest}e{exp}").parse::<f64>().ok() == Some(x);
    if reads_back && all.starts_with(&down) && tail.starts_with('5') && tail.bytes().skip(1).all(|b| b == b'0') {
        let trimmed = down.trim_end_matches('0');
        *digits = if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() };
    }
}

/// The size `ThumbnailDownscale.downscaleIfNeeded` gives a `width` × `height` picture: unchanged while the long
/// edge is at most [`THUMBNAIL_LONG_EDGE`], otherwise scaled by `1280 / long edge` with each side rounded half
/// away from zero (Swift's `.rounded()`) and at least 1.
pub fn downscaled_size(width: u32, height: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= THUMBNAIL_LONG_EDGE {
        return (width, height);
    }
    let scale = f64::from(THUMBNAIL_LONG_EDGE) / f64::from(long);
    let side = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
    (side(width), side(height))
}

/// The checkerboard tile (pixels) baked under a `width` × `height` thumbnail before it is cached: about ten tiles
/// across the longer side, never below 8 (integer division, as Swift's `Int`).
pub fn checkerboard_tile(width: u32, height: u32) -> u32 {
    (width.max(height) / 10).max(8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swift_writes_doubles_like_this() {
        // Every expectation printed by Swift 6.4 (`"\(x)"`), tools/swift-library-oracle `doubles`.
        let cases: [(f64, &str); 22] = [
            (9_007_199_254_740_992.0, "9007199254740992.0"),
            (9_007_199_254_740_994.0, "9.007199254740994e+15"),
            (18_014_398_509_481_984.0, "1.8014398509481984e+16"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (9_999_999_999_999_998.0, "9.999999999999998e+15"),
            (0.001, "0.001"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (0.00012345, "0.00012345"),
            (1_731_600_000.0, "1731600000.0"),
            (1_731_600_000.123456, "1731600000.123456"),
            (5e-324, "5e-324"),
            (2.2250738585072014e-308, "2.2250738585072014e-308"),
            (f64::MAX, "1.7976931348623157e+308"),
            (-1.5, "-1.5"),
            (100.0, "100.0"),
            (0.1, "0.1"),
            (1.0 / 3.0, "0.3333333333333333"),
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (f64::NEG_INFINITY, "-inf"),
        ];
        for (x, want) in cases {
            assert_eq!(swift_double(x), want, "{x:e}");
        }
        assert_eq!(swift_double(f64::NAN), "nan");
        assert_eq!(swift_double(-f64::NAN), "nan");
        assert_eq!(swift_double(f64::from_bits(0x7ff0_0000_0000_0001)), "nan");
        // Exactly halfway: Swift keeps the even last digit.
        assert_eq!(swift_double(f64::from_bits(0x4310_901c_319b_f4fd)), "1165512598289727.2");
        assert_eq!(swift_double(f64::from_bits(0x41c2_b536_5ab6_8000)), "627731637.4257812");
        assert_eq!(swift_double(f64::from_bits(0x3e60_0000_0000_0000)), "2.9802322387695312e-08");
        assert_eq!(swift_double(f64::from_bits(0x3e70_0000_0000_0000)), "5.960464477539063e-08");
    }

    #[test]
    fn the_key_is_16_hex_of_the_composite() {
        // python3 -c "import hashlib;print(hashlib.sha256(b'/Volumes/Clips/a.mov:1024:1731600000.5').hexdigest()[:16])"
        assert_eq!(thumbnail_key("/Volumes/Clips/a.mov", 1024, 1_731_600_000.5), "f27164e384d51a7b");
    }

    #[test]
    fn downscale_keeps_small_and_bounds_large() {
        assert_eq!(downscaled_size(1280, 720), (1280, 720));
        assert_eq!(downscaled_size(1920, 1080), (1280, 720));
        assert_eq!(downscaled_size(7680, 2160), (1280, 360));
        assert_eq!(downscaled_size(1080, 1920), (720, 1280));
        assert_eq!(downscaled_size(20000, 1), (1280, 1));
    }

    #[test]
    fn the_checkerboard_tile_is_a_tenth_of_the_long_side_at_least_8() {
        assert_eq!(checkerboard_tile(1920, 1080), 192);
        assert_eq!(checkerboard_tile(40, 30), 8);
        assert_eq!(checkerboard_tile(89, 10), 8);
        assert_eq!(checkerboard_tile(90, 10), 9);
    }
}
