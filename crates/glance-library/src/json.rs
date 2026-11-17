//! JSON the way Foundation writes it, byte for byte: `JSONEncoder` with `[.prettyPrinted, .sortedKeys]` (the sidecar
//! and stash files) and a plain `JSONEncoder()` (a saved filter preset). Measured against Swift 6 Foundation on macOS by
//! the oracle in `tools/swift-library-oracle` (golden files in `tests/fixtures/swift-0.16.4/json`):
//!
//! - two-space indent, `"key" : value` (a space on both sides of the colon), no newline at the end;
//! - an empty array is `[`, an empty line, then `]` at the parent's indent; an empty object likewise;
//! - keys sorted by their UTF-8 bytes;
//! - `"`, `\` and `/` are escaped (`\"`, `\\`, `\/`), so are U+0008, U+0009, U+000A, U+000C, U+000D (`\b \t \n \f
//!   \r`) and the other controls below U+0020 (`\u001f`, lower-case hex); everything else, U+007F and U+2028
//!   included, is written as UTF-8;
//! - a `Double` is Swift's `description` with a trailing `.0` dropped (`1700000000`, `1731600000.123`, `-0`,
//!   `1e-05`, `1e+16`, `9.007199254740994e+15`); NaN and infinities cannot be encoded.

use crate::thumbkey::swift_double;
use std::fmt::Write as _;

/// A JSON value with its numbers kept as Swift typed them, and objects in insertion order (sorted on output when the
/// writer is asked to).
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Double(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// An object from `(key, value)` pairs.
    pub fn object<K: Into<String>>(pairs: impl IntoIterator<Item = (K, Json)>) -> Json {
        Json::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// A string value.
    pub fn string(s: impl Into<String>) -> Json {
        Json::String(s.into())
    }

    /// `Some(v)` as `f(v)`, `None` as `null`.
    pub fn or_null<T>(value: Option<T>, f: impl FnOnce(T) -> Json) -> Json {
        value.map_or(Json::Null, f)
    }
}

/// A double Foundation's JSON writer cannot write (NaN or an infinity).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NonFiniteNumber(pub f64);

impl std::fmt::Display for NonFiniteNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unable to encode {} in JSON", swift_double(self.0))
    }
}

impl std::error::Error for NonFiniteNumber {}

/// `JSONEncoder` with `[.prettyPrinted, .sortedKeys]`.
pub fn to_pretty_sorted(value: &Json) -> Result<String, NonFiniteNumber> {
    let mut out = String::new();
    write_value(&mut out, value, Some(0), true)?;
    Ok(out)
}

/// A plain `JSONEncoder()`: compact, keys in the order given (Swift's own order is unstable; readers compare parsed).
pub fn to_compact(value: &Json) -> Result<String, NonFiniteNumber> {
    let mut out = String::new();
    write_value(&mut out, value, None, false)?;
    Ok(out)
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn write_value(out: &mut String, value: &Json, pretty: Option<usize>, sort: bool) -> Result<(), NonFiniteNumber> {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Json::Double(d) => out.push_str(&json_double(*d)?),
        Json::String(s) => write_string(out, s),
        Json::Array(items) => {
            out.push('[');
            match pretty {
                Some(level) => {
                    out.push('\n');
                    if items.is_empty() {
                        out.push('\n');
                    }
                    for (i, item) in items.iter().enumerate() {
                        indent(out, level + 1);
                        write_value(out, item, Some(level + 1), sort)?;
                        if i + 1 < items.len() {
                            out.push(',');
                        }
                        out.push('\n');
                    }
                    indent(out, level);
                }
                None => {
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        write_value(out, item, None, sort)?;
                    }
                }
            }
            out.push(']');
        }
        Json::Object(pairs) => {
            let mut refs: Vec<&(String, Json)> = pairs.iter().collect();
            if sort {
                refs.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            }
            out.push('{');
            match pretty {
                Some(level) => {
                    out.push('\n');
                    if refs.is_empty() {
                        out.push('\n');
                    }
                    for (i, (key, item)) in refs.iter().map(|p| (&p.0, &p.1)).enumerate() {
                        indent(out, level + 1);
                        write_string(out, key);
                        out.push_str(" : ");
                        write_value(out, item, Some(level + 1), sort)?;
                        if i + 1 < refs.len() {
                            out.push(',');
                        }
                        out.push('\n');
                    }
                    indent(out, level);
                }
                None => {
                    for (i, (key, item)) in refs.iter().map(|p| (&p.0, &p.1)).enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        write_string(out, key);
                        out.push(':');
                        write_value(out, item, None, sort)?;
                    }
                }
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A double as Foundation's JSON writer prints it: Swift's `description`, without a trailing `.0`.
pub fn json_double(value: f64) -> Result<String, NonFiniteNumber> {
    if !value.is_finite() {
        return Err(NonFiniteNumber(value));
    }
    let mut s = swift_double(value);
    if s.ends_with(".0") {
        s.truncate(s.len() - 2);
    }
    Ok(s)
}

/// Parse a JSON document as Foundation's `JSONDecoder` reads it: a key given twice keeps its first value, and a
/// number keeps whether it was written as an integer. `None` for anything that is not JSON.
pub fn parse(bytes: &[u8]) -> Option<Json> {
    serde_json::from_slice::<Json>(bytes).ok()
}

/// 2^63 (`Int64` holds [-2^63, 2^63)).
const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;

impl Json {
    /// The value of `key` in an object (the first, if the key repeats); `None` when absent or not an object.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// `decodeIfPresent`: a missing key and an explicit `null` are both `None`.
    pub fn present(&self, key: &str) -> Option<&Json> {
        self.get(key).filter(|v| !matches!(v, Json::Null))
    }

    /// Foundation's `Double` from a JSON number.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Int(i) => Some(*i as f64),
            Json::Double(d) => Some(*d),
            _ => None,
        }
    }

    /// Foundation's `Int` from a JSON number: an integer, or a number with an integral value that fits (`4.0`).
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Int(i) => Some(*i),
            Json::Double(d) if d.fract() == 0.0 && *d >= -TWO_POW_63 && *d < TWO_POW_63 => Some(*d as i64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }
}

impl<'de> serde::Deserialize<'de> for Json {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Json, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Json;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON")
            }
            fn visit_unit<E>(self) -> Result<Json, E> {
                Ok(Json::Null)
            }
            fn visit_bool<E>(self, v: bool) -> Result<Json, E> {
                Ok(Json::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Json, E> {
                Ok(Json::Int(v))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Json, E> {
                // Beyond Int64: Foundation reads it as a Double (and refuses it as an Int).
                Ok(i64::try_from(v).map_or(Json::Double(v as f64), Json::Int))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Json, E> {
                Ok(Json::Double(v))
            }
            fn visit_str<E>(self, v: &str) -> Result<Json, E> {
                Ok(Json::String(v.to_string()))
            }
            fn visit_string<E>(self, v: String) -> Result<Json, E> {
                Ok(Json::String(v))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Json, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element::<Json>()? {
                    items.push(item);
                }
                Ok(Json::Array(items))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
                let mut pairs = Vec::new();
                while let Some((k, v)) = map.next_entry::<String, Json>()? {
                    pairs.push((k, v));
                }
                Ok(Json::Object(pairs))
            }
        }
        deserializer.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_shapes() {
        let v = Json::object([
            ("e", Json::Array(vec![])),
            ("a", Json::Array(vec![Json::Array(vec![])])),
            ("o", Json::Object(vec![])),
            ("s", Json::string("a/b\u{1}\u{7f}")),
        ]);
        assert_eq!(
            to_pretty_sorted(&v).ok().as_deref(),
            Some("{\n  \"a\" : [\n    [\n\n    ]\n  ],\n  \"e\" : [\n\n  ],\n  \"o\" : {\n\n  },\n  \"s\" : \"a\\/b\\u0001\u{7f}\"\n}")
        );
        assert!(to_pretty_sorted(&Json::Double(f64::NAN)).is_err());
    }
}
