//! A preferences value: the property-list types `UserDefaults` stores.

use std::collections::BTreeMap;

/// One stored value, as a property list holds it (`NSUserDefaults` on macOS; the same shape in
/// the Windows settings file). Dates are not used by any of the family's keys and are not read.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    String(String),
    /// A boolean as such (`CFBoolean`; Swift's `set(true, forKey:)`, a plist `<true/>`).
    Bool(bool),
    /// An integer number (Swift's `set(Int)`).
    Int(i64),
    /// A floating-point number (Swift's `set(Double)` / `set(Float)`).
    Real(f64),
    /// Bytes (Swift's `set(Data)`: the family's JSON-encoded keys, bookmarks).
    Data(Vec<u8>),
    Array(Vec<Value>),
    Dict(BTreeMap<String, Value>),
}
