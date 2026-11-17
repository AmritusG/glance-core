// SPDX-License-Identifier: MIT
//! JSON the way Swift's `JSONEncoder` writes it (compact output, `dateEncodingStrategy =
//! .secondsSince1970`), so the files SendEhrKit wrote and the files this crate writes read the same
//! in both directions.
//!
//! What is matched exactly: `/` is written as `\/` (the encoder's default without
//! `.withoutEscapingSlashes`); control characters as `\b \f \n \r \t \u00xx`; non-ASCII as raw
//! UTF-8; a `Double` with no fraction and a magnitude up to 2^53 without `.0`; larger than 2^53
//! or smaller than 1e-4 in exponent form with a sign and at least two digits (`1e+16`, `1e-07`); a
//! `Date` as its seconds since 1970 after Swift's round trip through the 2001 reference date
//! (`0.1` is written `0.10000002384185791`, as Swift does).
//!
//! What cannot be matched byte for byte: Swift's encoder writes an object's keys in the order of
//! its per-process hash seed (a different order on every launch). This crate writes them in
//! declaration order; [`to_vec_sorted`] gives the order Swift's `.sortedKeys` gives, for comparing.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use serde_json::ser::Formatter;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds between 1970-01-01 and Swift's reference date 2001-01-01.
const REFERENCE_DATE_OFFSET: f64 = 978_307_200.0;

/// A point in time as seconds since 1970, the value a Swift `Date` encodes to. Compared as Swift
/// compares dates: by the value relative to 2001 (so a date and its encoded round trip are equal).
#[derive(Debug, Clone, Copy, Default)]
pub struct Timestamp(pub f64);

impl PartialEq for Timestamp {
    fn eq(&self, other: &Self) -> bool {
        self.reference_value() == other.reference_value()
    }
}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.reference_value().partial_cmp(&other.reference_value())
    }
}

impl Timestamp {
    /// Now, from the system clock.
    pub fn now() -> Self {
        let secs = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs_f64(),
            Err(e) => -e.duration().as_secs_f64(),
        };
        Timestamp(secs)
    }

    /// From a `SystemTime`.
    pub fn from_system_time(t: SystemTime) -> Self {
        match t.duration_since(UNIX_EPOCH) {
            Ok(d) => Timestamp(d.as_secs_f64()),
            Err(e) => Timestamp(-e.duration().as_secs_f64()),
        }
    }

    /// Seconds from `earlier` to `self`.
    pub fn seconds_since(self, earlier: Timestamp) -> f64 {
        self.0 - earlier.0
    }

    /// Seconds since 2001-01-01, what a Swift `Date` stores.
    pub fn reference_value(self) -> f64 {
        self.0 - REFERENCE_DATE_OFFSET
    }

    /// The number Swift writes for this date: stored relative to 2001, written relative to 1970.
    pub fn swift_encoded(self) -> f64 {
        self.reference_value() + REFERENCE_DATE_OFFSET
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(self.swift_encoded())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        f64::deserialize(d).map(Timestamp)
    }
}

/// The `serde_json` formatter that writes numbers and strings as Swift's `JSONEncoder` does.
#[derive(Debug, Clone, Copy, Default)]
pub struct SwiftFormatter;

impl Formatter for SwiftFormatter {
    fn write_f64<W: ?Sized + io::Write>(&mut self, w: &mut W, value: f64) -> io::Result<()> {
        w.write_all(swift_double(value).as_bytes())
    }

    fn write_f32<W: ?Sized + io::Write>(&mut self, w: &mut W, value: f32) -> io::Result<()> {
        w.write_all(swift_double(f64::from(value)).as_bytes())
    }

    fn write_string_fragment<W: ?Sized + io::Write>(&mut self, w: &mut W, fragment: &str) -> io::Result<()> {
        let bytes = fragment.as_bytes();
        let mut start = 0;
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'/' {
                w.write_all(&bytes[start..i])?;
                w.write_all(b"\\/")?;
                start = i + 1;
            }
        }
        w.write_all(&bytes[start..])
    }
}

/// A `Double` as Swift's `JSONEncoder` writes it.
pub fn swift_double(x: f64) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.to_string();
    }
    if !x.is_finite() {
        // Swift's encoder refuses non-finite numbers; nothing here produces one.
        return "0".to_string();
    }
    // Shortest round-trip digits and the decimal exponent of the first digit.
    let e = format!("{:e}", x.abs());
    let (mantissa, exp) = e.split_once('e').unwrap_or((e.as_str(), "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let sign = if x < 0.0 { "-" } else { "" };
    let a = x.abs();
    if (1e-4..=9_007_199_254_740_992.0).contains(&a) {
        let n = digits.len();
        let body = if exp >= 0 {
            let int_len = usize::try_from(exp).unwrap_or(0) + 1;
            if n > int_len { format!("{}.{}", &digits[..int_len], &digits[int_len..]) } else { format!("{}{}", digits, "0".repeat(int_len - n)) }
        } else {
            let zeros = usize::try_from(-exp - 1).unwrap_or(0);
            format!("0.{}{}", "0".repeat(zeros), digits)
        };
        format!("{sign}{body}")
    } else {
        let first = digits.get(..1).unwrap_or("0");
        let rest = digits.get(1..).unwrap_or("");
        let mant = if rest.is_empty() { first.to_string() } else { format!("{first}.{rest}") };
        let esign = if exp < 0 { '-' } else { '+' };
        format!("{sign}{mant}e{esign}{:02}", exp.unsigned_abs())
    }
}

/// Encodes `value` as Swift's `JSONEncoder` would (keys in declaration order).
pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, SwiftFormatter);
    value.serialize(&mut ser)?;
    Ok(out)
}

/// Encodes `value` with keys sorted by their UTF-8 bytes, as Swift's `JSONEncoder` with
/// `.sortedKeys` does for the ASCII keys used here. For comparing with Swift's output.
pub fn to_vec_sorted<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let v = serde_json::to_value(value)?;
    let mut out = Vec::new();
    write_sorted(&v, &mut out)?;
    Ok(out)
}

fn write_sorted(v: &Value, out: &mut Vec<u8>) -> Result<(), serde_json::Error> {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            out.push(b'{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                scalar(&Value::String((*k).clone()), out)?;
                out.push(b':');
                if let Some(child) = map.get(*k) {
                    write_sorted(child, out)?;
                }
            }
            out.push(b'}');
            Ok(())
        }
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_sorted(item, out)?;
            }
            out.push(b']');
            Ok(())
        }
        other => scalar(other, out),
    }
}

fn scalar(v: &Value, out: &mut Vec<u8>) -> Result<(), serde_json::Error> {
    let mut ser = serde_json::Serializer::with_formatter(out, SwiftFormatter);
    v.serialize(&mut ser)
}

/// `UUID` as Swift encodes it: upper-case, hyphenated.
pub(crate) mod uuid_upper {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    use uuid::Uuid;

    pub fn serialize<S: Serializer>(id: &Uuid, s: S) -> Result<S::Ok, S::Error> {
        let mut buf = [0u8; 36];
        s.serialize_str(id.hyphenated().encode_upper(&mut buf))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Uuid, D::Error> {
        let s = String::deserialize(d)?;
        Uuid::parse_str(&s).map_err(|_| D::Error::custom(format!("Attempted to decode UUID from invalid UUID string: {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_as_swift_writes_them() {
        // Measured with Swift 6.4 JSONEncoder on macOS 27.
        let cases: [(f64, &str); 21] = [
            (1.0, "1"),
            (0.5, "0.5"),
            (1e16, "1e+16"),
            (1e-7, "1e-07"),
            (123_456_789.123_456_79, "123456789.12345679"),
            (-0.0, "-0"),
            (3.0e15, "3000000000000000"),
            (9_007_199_254_740_993.0, "9007199254740992"),
            (1_763_380_123.456_789, "1763380123.456789"),
            (9_007_199_254_740_994.0, "9.007199254740994e+15"),
            (0.0001, "0.0001"),
            (0.000_123_45, "0.00012345"),
            (0.000_09, "9e-05"),
            (1.5e300, "1.5e+300"),
            (1.2345e20, "1.2345e+20"),
            (5e-324, "5e-324"),
            (12_345_678_901_234_567.0, "1.2345678901234568e+16"),
            (1e15, "1000000000000000"),
            (0.1, "0.1"),
            (100.25, "100.25"),
            (1_763_380_000.000_1, "1763380000.0001"),
        ];
        for (x, want) in cases {
            assert_eq!(swift_double(x), want, "{x:e}");
        }
    }

    #[test]
    fn dates_go_through_the_reference_date() {
        assert_eq!(swift_double(Timestamp(0.1).swift_encoded()), "0.10000002384185791");
        assert_eq!(swift_double(Timestamp(1_763_380_000.0).swift_encoded()), "1763380000");
        assert_eq!(swift_double(Timestamp(1_763_380_000.123_456).swift_encoded()), "1763380000.123456");
        assert_eq!(swift_double(Timestamp(1e17).swift_encoded()), "1e+17");
    }

    #[test]
    fn strings_escape_slash_and_controls() {
        let s = "Layer 1 \u{203A} Column 2/x \"q\" \u{1} \t \u{e9}";
        let out = to_vec(s).unwrap_or_default();
        assert_eq!(String::from_utf8_lossy(&out), "\"Layer 1 \u{203A} Column 2\\/x \\\"q\\\" \\u0001 \\t \u{e9}\"");
        let out = to_vec("\u{7f}\u{1f}\u{8}\u{c}\n\r\u{2028}<>&'").unwrap_or_default();
        assert_eq!(String::from_utf8_lossy(&out), "\"\u{7f}\\u001f\\b\\f\\n\\r\u{2028}<>&'\"");
    }
}
