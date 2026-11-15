//! # glance-settings — the family's settings, in the Swift apps' own stores and encodings
//!
//! A [`Store`] is where an app's settings live: on macOS the `UserDefaults` domain of its bundle
//! id ([`glance_platform::Defaults`]), the store the Swift twin uses, so settings survive the swap
//! in both directions (docs/00-ROADMAP.md §6 step 3); on Windows one JSON file per app in the
//! per-user application-data folder ([`FileStore`], `%APPDATA%\Amritus\<App>\preferences.json`);
//! [`MemoryStore`] for tests.
//!
//! Values are read the way Swift's `UserDefaults` reads them ([`read_bool`], [`read_string`],
//! [`read_string_array`]), so a value of an unexpected type gives the same answer it gives the
//! Swift app.
//!
//! [`GlanceSettings`] is Swift Glance's settings, key for key (`docs/research/glance.md` §6;
//! `glance/Sources/Glance/PlayerModel.swift:63-71`, `AlphaTypeMemory.swift:11-26`,
//! `DefaultHandlerControl.swift:25,44,49`): exactly three persisted values — the background, the
//! per-clip Alpha Type list, the default-prompt-shown flag. Volume, mute, loop and the last file
//! are not persisted (Swift persists none of them).
#![forbid(unsafe_code)]

use glance_media::AlphaView;
pub use glance_platform::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where settings live.
pub trait Store {
    fn get(&self, key: &str) -> Option<Value>;
    fn set(&self, key: &str, value: Value);
    fn remove(&self, key: &str);
}

#[cfg(target_os = "macos")]
impl Store for glance_platform::Defaults {
    fn get(&self, key: &str) -> Option<Value> {
        glance_platform::Defaults::get(self, key)
    }
    fn set(&self, key: &str, value: Value) {
        glance_platform::Defaults::set(self, key, &value);
    }
    fn remove(&self, key: &str) {
        glance_platform::Defaults::remove(self, key);
    }
}

/// Settings in memory (tests).
#[derive(Debug, Default)]
pub struct MemoryStore(Mutex<BTreeMap<String, Value>>);

impl Store for MemoryStore {
    fn get(&self, key: &str) -> Option<Value> {
        self.0.lock().ok()?.get(key).cloned()
    }
    fn set(&self, key: &str, value: Value) {
        if let Ok(mut m) = self.0.lock() {
            m.insert(key.to_string(), value);
        }
    }
    fn remove(&self, key: &str) {
        if let Ok(mut m) = self.0.lock() {
            m.remove(key);
        }
    }
}

/// Settings in one JSON file: the Windows store (and any system without a preferences domain).
///
/// The file is an object from key to one tagged value, so every property-list type keeps its
/// type: `{"Glance.backgroundMode": {"string": "black"}, "Glance.defaultHandlerPromptShown":
/// {"bool": true}}`; `int`, `real`, `data` (hex), `array` and `dict` likewise. Written whole on
/// every change (to a temporary file, then renamed over the old one). A missing or unreadable
/// file is an empty store.
#[derive(Debug)]
pub struct FileStore {
    path: PathBuf,
    values: Mutex<BTreeMap<String, Value>>,
}

fn to_json(v: &Value) -> serde_json::Value {
    use serde_json::json;
    match v {
        Value::String(s) => json!({ "string": s }),
        Value::Bool(b) => json!({ "bool": b }),
        Value::Int(i) => json!({ "int": i }),
        Value::Real(r) => json!({ "real": r }),
        Value::Data(d) => json!({ "data": d.iter().map(|b| format!("{b:02x}")).collect::<String>() }),
        Value::Array(a) => json!({ "array": a.iter().map(to_json).collect::<Vec<_>>() }),
        Value::Dict(m) => json!({ "dict": m.iter().map(|(k, v)| (k.clone(), to_json(v))).collect::<serde_json::Map<_, _>>() }),
    }
}

fn from_json(j: &serde_json::Value) -> Option<Value> {
    let (tag, v) = j.as_object()?.iter().next()?;
    Some(match tag.as_str() {
        "string" => Value::String(v.as_str()?.to_string()),
        "bool" => Value::Bool(v.as_bool()?),
        "int" => Value::Int(v.as_i64()?),
        "real" => Value::Real(v.as_f64()?),
        "data" => {
            let hex = v.as_str()?;
            Value::Data((0..hex.len()).step_by(2).map(|i| hex.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect::<Option<Vec<u8>>>()?)
        }
        "array" => Value::Array(v.as_array()?.iter().map(from_json).collect::<Option<Vec<_>>>()?),
        "dict" => Value::Dict(v.as_object()?.iter().map(|(k, v)| Some((k.clone(), from_json(v)?))).collect::<Option<BTreeMap<_, _>>>()?),
        _ => return None,
    })
}

impl FileStore {
    /// The store in `path` (read now; created on the first write).
    pub fn open(path: &Path) -> FileStore {
        let values = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&b).ok())
            .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), from_json(v)?))).collect())
            .unwrap_or_default();
        FileStore { path: path.to_path_buf(), values: Mutex::new(values) }
    }

    /// The app's store in its per-user folder: `<settings_dir>/preferences.json`.
    pub fn for_app(vendor: &str, app: &str) -> Option<FileStore> {
        Some(FileStore::open(&glance_platform::settings_dir(vendor, app)?.join("preferences.json")))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn save(&self, values: &BTreeMap<String, Value>) {
        let map: serde_json::Map<String, serde_json::Value> = values.iter().map(|(k, v)| (k.clone(), to_json(v))).collect();
        let Ok(text) = serde_json::to_vec_pretty(&serde_json::Value::Object(map)) else { return };
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = self.path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

impl Store for FileStore {
    fn get(&self, key: &str) -> Option<Value> {
        self.values.lock().ok()?.get(key).cloned()
    }
    fn set(&self, key: &str, value: Value) {
        if let Ok(mut m) = self.values.lock() {
            m.insert(key.to_string(), value);
            self.save(&m);
        }
    }
    fn remove(&self, key: &str) {
        if let Ok(mut m) = self.values.lock()
            && m.remove(key).is_some()
        {
            self.save(&m);
        }
    }
}

/// The store an app with bundle id `bundle_id` uses on this system: its preferences domain on
/// macOS, `%APPDATA%\<vendor>\<app>\preferences.json` elsewhere.
pub fn app_store(bundle_id: &str, vendor: &str, app: &str) -> Result<Box<dyn Store>, String> {
    #[cfg(target_os = "macos")]
    {
        let _ = (vendor, app);
        glance_platform::Defaults::for_domain(bundle_id).map(|d| Box::new(d) as Box<dyn Store>).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = bundle_id;
        FileStore::for_app(vendor, app).map(|f| Box::new(f) as Box<dyn Store>).ok_or_else(|| "no per-user settings folder (APPDATA is not set)".into())
    }
}

/// Swift's `bool(forKey:)`: a boolean or number is itself (non-zero is true); a string is true
/// when, after leading spaces, an optional sign and leading zeros, it starts with Y, y, T, t or a
/// digit 1–9 (`NSString.boolValue`); anything else, or no value, is false.
pub fn read_bool<S: Store + ?Sized>(store: &S, key: &str) -> bool {
    match store.get(key) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(i)) => i != 0,
        Some(Value::Real(r)) => r != 0.0,
        Some(Value::String(s)) => {
            let rest = s.trim_start().trim_start_matches(['+', '-']).trim_start_matches('0');
            rest.chars().next().is_some_and(|c| matches!(c, 'Y' | 'y' | 'T' | 't' | '1'..='9'))
        }
        _ => false,
    }
}

/// Swift's `string(forKey:)` for the family's keys: the string stored, else `None`. (Swift also
/// turns a stored number into its decimal text; no family key is read that way, and such text is
/// never a valid value of a string key.)
pub fn read_string<S: Store + ?Sized>(store: &S, key: &str) -> Option<String> {
    match store.get(key) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

/// Swift's `stringArray(forKey:)`: an array whose elements are all strings; any other element, or
/// another type, is `None`.
pub fn read_string_array<S: Store + ?Sized>(store: &S, key: &str) -> Option<Vec<String>> {
    match store.get(key) {
        Some(Value::Array(items)) => items.into_iter().map(|v| if let Value::String(s) = v { Some(s) } else { None }).collect(),
        _ => None,
    }
}

/// Swift Glance's bundle id: its preferences domain. Only the swap step uses it (never a test).
pub const GLANCE_BUNDLE_ID: &str = "com.amritus.glance";

/// What is painted behind the picture (Swift's `DXVRenderer.BackgroundMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackgroundMode {
    #[default]
    Black,
    Checkerboard,
}

impl BackgroundMode {
    /// The raw value Swift stores.
    pub fn stored_name(self) -> &'static str {
        match self {
            BackgroundMode::Black => "black",
            BackgroundMode::Checkerboard => "checkerboard",
        }
    }
}

/// Swift Glance's settings keys.
pub mod keys {
    /// String `"black"` / `"checkerboard"`; missing or unknown is black.
    pub const BACKGROUND: &str = "Glance.backgroundMode";
    /// [String], each `"<64-hex clip key>=<premultiplied|straight>"`, newest last.
    pub const ALPHA_TYPE_BY_CLIP: &str = "Glance.alphaTypeByClip";
    /// Bool: the first-launch "Make Glance the default for .mov files?" prompt was shown.
    pub const DEFAULT_PROMPT_SHOWN: &str = "Glance.defaultHandlerPromptShown";
}

/// At most this many remembered Alpha Type choices; the oldest goes first.
pub const ALPHA_MEMORY_CAPACITY: usize = 2000;

/// Swift Glance's three persisted values over a [`Store`].
pub struct GlanceSettings<S: Store + ?Sized> {
    store: Box<S>,
}

impl<S: Store + ?Sized> GlanceSettings<S> {
    pub fn new(store: Box<S>) -> GlanceSettings<S> {
        GlanceSettings { store }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    /// `PlayerModel.backgroundMode`'s initial value: the stored raw value, black when missing or
    /// unknown.
    pub fn background(&self) -> BackgroundMode {
        match read_string(&*self.store, keys::BACKGROUND).as_deref() {
            Some("checkerboard") => BackgroundMode::Checkerboard,
            _ => BackgroundMode::Black,
        }
    }

    /// Persisted on every change (`didSet`).
    pub fn set_background(&self, mode: BackgroundMode) {
        self.store.set(keys::BACKGROUND, Value::String(mode.stored_name().into()));
    }

    /// `AlphaTypeMemory.choice(for:)`: the newest line for `clip_key`, if its value is an Alpha Type.
    pub fn alpha_choice(&self, clip_key: &str) -> Option<AlphaView> {
        let prefix = format!("{clip_key}=");
        let lines = read_string_array(&*self.store, keys::ALPHA_TYPE_BY_CLIP).unwrap_or_default();
        let line = lines.iter().rev().find(|l| l.starts_with(&prefix))?;
        AlphaView::from_stored_name(line.get(prefix.len()..)?)
    }

    /// `AlphaTypeMemory.remember(_:for:)`: drop the clip's older lines, append the new one, keep
    /// the newest [`ALPHA_MEMORY_CAPACITY`]. (Swift starts from an empty list when the stored value
    /// is not an array of strings; so does this.)
    pub fn remember_alpha(&self, clip_key: &str, view: AlphaView) {
        let prefix = format!("{clip_key}=");
        let mut lines: Vec<String> =
            read_string_array(&*self.store, keys::ALPHA_TYPE_BY_CLIP).unwrap_or_default().into_iter().filter(|l| !l.starts_with(&prefix)).collect();
        lines.push(format!("{prefix}{}", view.stored_name()));
        if lines.len() > ALPHA_MEMORY_CAPACITY {
            lines.drain(..lines.len() - ALPHA_MEMORY_CAPACITY);
        }
        self.store.set(keys::ALPHA_TYPE_BY_CLIP, Value::Array(lines.into_iter().map(Value::String).collect()));
    }

    /// `DefaultHandlerControl.hasUserBeenAsked`.
    pub fn default_prompt_shown(&self) -> bool {
        read_bool(&*self.store, keys::DEFAULT_PROMPT_SHOWN)
    }

    /// `DefaultHandlerControl.markPromptShown`.
    pub fn mark_default_prompt_shown(&self) {
        self.store.set(keys::DEFAULT_PROMPT_SHOWN, Value::Bool(true));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    fn memory() -> GlanceSettings<MemoryStore> {
        GlanceSettings::new(Box::new(MemoryStore::default()))
    }

    #[test]
    fn the_background_is_black_unless_checkerboard_is_stored() {
        let s = memory();
        assert_eq!(s.background(), BackgroundMode::Black);
        s.set_background(BackgroundMode::Checkerboard);
        assert_eq!(s.store().get(keys::BACKGROUND), Some(Value::String("checkerboard".into())));
        assert_eq!(s.background(), BackgroundMode::Checkerboard);
        for odd in [Value::String("Checkerboard".into()), Value::String(String::new()), Value::Int(1), Value::Bool(true)] {
            s.store().set(keys::BACKGROUND, odd.clone());
            assert_eq!(s.background(), BackgroundMode::Black, "{odd:?}");
        }
    }

    #[test]
    fn alpha_choices_follow_swifts_memory_newest_last_capped() {
        let s = memory();
        let key = |i: usize| format!("{i:064x}");
        assert_eq!(s.alpha_choice(&key(1)), None);
        s.remember_alpha(&key(1), AlphaView::Straight);
        s.remember_alpha(&key(2), AlphaView::Premultiplied);
        s.remember_alpha(&key(1), AlphaView::Premultiplied);
        assert_eq!(
            read_string_array(s.store(), keys::ALPHA_TYPE_BY_CLIP).unwrap(),
            vec![format!("{}=premultiplied", key(2)), format!("{}=premultiplied", key(1))]
        );
        assert_eq!(s.alpha_choice(&key(1)), Some(AlphaView::Premultiplied));
        // The capacity: 2,000 lines, the oldest dropped.
        for i in 10..(10 + ALPHA_MEMORY_CAPACITY) {
            s.remember_alpha(&key(i), AlphaView::Straight);
        }
        let lines = read_string_array(s.store(), keys::ALPHA_TYPE_BY_CLIP).unwrap();
        assert_eq!(lines.len(), ALPHA_MEMORY_CAPACITY);
        assert_eq!(s.alpha_choice(&key(1)), None, "the oldest went first");
        assert_eq!(s.alpha_choice(&key(10 + ALPHA_MEMORY_CAPACITY - 1)), Some(AlphaView::Straight));
        // A line whose value is not an Alpha Type is no choice; a list with a non-string is empty.
        s.store().set(keys::ALPHA_TYPE_BY_CLIP, Value::Array(vec![Value::String(format!("{}=sideways", key(3)))]));
        assert_eq!(s.alpha_choice(&key(3)), None);
        s.store().set(keys::ALPHA_TYPE_BY_CLIP, Value::Array(vec![Value::String(format!("{}=straight", key(3))), Value::Int(1)]));
        assert_eq!(s.alpha_choice(&key(3)), None);
    }

    #[test]
    fn booleans_read_as_swift_reads_them() {
        let store = MemoryStore::default();
        let cases = [
            (Value::Bool(true), true),
            (Value::Int(2), true),
            (Value::Int(0), false),
            (Value::Real(0.5), true),
            (Value::String("YES".into()), true),
            (Value::String("  true".into()), true),
            (Value::String("007".into()), true),
            (Value::String("-0".into()), false),
            (Value::String("no".into()), false),
            (Value::Data(vec![1]), false),
        ];
        for (v, want) in cases {
            store.set("k", v.clone());
            assert_eq!(read_bool(&store, "k"), want, "{v:?}");
        }
        assert!(!read_bool(&store, "missing"));
        let s = memory();
        assert!(!s.default_prompt_shown());
        s.mark_default_prompt_shown();
        assert!(s.default_prompt_shown());
        assert_eq!(s.store().get(keys::DEFAULT_PROMPT_SHOWN), Some(Value::Bool(true)));
    }

    #[test]
    fn the_file_store_keeps_every_type_across_a_reopen() {
        let dir = std::env::temp_dir().join(format!("glance-settings-test-{}", std::process::id()));
        let path = dir.join("preferences.json");
        let mut dict = BTreeMap::new();
        dict.insert("w".to_string(), Value::Int(1920));
        let cases = [
            ("s", Value::String("checkerboard".into())),
            ("b", Value::Bool(true)),
            ("i", Value::Int(64)),
            ("r", Value::Real(0.6)),
            ("d", Value::Data(vec![0x7b, 0x7d, 0])),
            ("a", Value::Array(vec![Value::Array(vec![Value::String("k".into()), Value::String("straight".into())])])),
            ("m", Value::Dict(dict)),
        ];
        {
            let f = FileStore::open(&path);
            for (k, v) in &cases {
                f.set(k, v.clone());
            }
            f.remove("i");
        }
        let f = FileStore::open(&path);
        for (k, v) in &cases {
            assert_eq!(f.get(k), if *k == "i" { None } else { Some(v.clone()) }, "{k}");
        }
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(FileStore::open(&path).get("s"), None, "an unreadable file is an empty store");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
