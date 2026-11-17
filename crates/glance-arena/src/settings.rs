// SPDX-License-Identifier: MIT
//! The settings a host keeps in `settings.json`, the menu-picked changes to them, and the recent
//! targets in `recents.json`. Decoding tolerates missing keys (an older file keeps its values).

use crate::media::SortKey;
use crate::planner::Destination;
use crate::remap::PathRemap;
use crate::swift_json::Timestamp;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// How an occupied slot is offered in the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OccupiedSlotMode {
    /// Occupied slots are normal items; loading over one asks first (default).
    #[default]
    Ask,
    /// Occupied slots open a submenu with an explicit "Replace" item; no dialog.
    ReplaceItem,
}

/// Which new layers Trigger After Load connects when one pick makes several (Folders as New
/// Layers). One new layer is always triggered either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TriggerScope {
    /// The first clip of every new layer (default).
    #[default]
    AllNewLayers,
    /// Only the first clip of the top-most new layer.
    TopLayerOnly,
}

impl TriggerScope {
    pub const ALL: [TriggerScope; 2] = [TriggerScope::AllNewLayers, TriggerScope::TopLayerOnly];

    pub fn title(self) -> &'static str {
        match self {
            TriggerScope::AllNewLayers => "All New Layers",
            TriggerScope::TopLayerOnly => "Top Layer Only",
        }
    }

    /// Whether the new layer at `position` (0 = top-most) is triggered.
    pub fn triggers(self, position: usize) -> bool {
        self == TriggerScope::AllNewLayers || position == 0
    }
}

/// How the composition reaches the menus. Every load re-reads a fresh composition before acting,
/// so this only decides how current the menu labels are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArenaUpdateMode {
    /// Websocket open: Arena pushes the full composition on every change (default).
    #[default]
    Live,
    /// No websocket: one REST fetch when a menu opens (or Refresh).
    OnDemand,
    /// No websocket: a REST fetch every `update_interval` seconds, plus on menu open.
    Interval,
}

impl ArenaUpdateMode {
    pub const ALL: [ArenaUpdateMode; 3] = [ArenaUpdateMode::Live, ArenaUpdateMode::OnDemand, ArenaUpdateMode::Interval];

    /// The interval choices offered in the menus (seconds).
    pub const INTERVALS: [i64; 5] = [15, 30, 60, 120, 300];

    pub fn title(self) -> &'static str {
        match self {
            ArenaUpdateMode::Live => "Live",
            ArenaUpdateMode::OnDemand => "On Demand",
            ArenaUpdateMode::Interval => "Interval",
        }
    }

    /// "15 s", "1 min", "90 s".
    pub fn interval_title(seconds: i64) -> String {
        if seconds < 60 || seconds % 60 != 0 { format!("{seconds} s") } else { format!("{} min", seconds / 60) }
    }
}

/// Where Arena's webserver listens.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArenaEndpoint {
    pub host: String,
    #[serde(deserialize_with = "swift_int")]
    pub port: i64,
}

/// An `Int` as Swift's `JSONDecoder` reads it: an integer, or a number with no fraction.
fn swift_int<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let n = serde_json::Number::deserialize(d)?;
    if let Some(i) = n.as_i64() {
        return Ok(i);
    }
    match n.as_f64() {
        Some(f) if f.fract() == 0.0 && (-9.223_372_036_854_776e18..9.223_372_036_854_776e18).contains(&f) => Ok(f as i64),
        _ => Err(serde::de::Error::custom(format!("Parsed JSON number <{n}> does not fit in Int."))),
    }
}

/// An optional `Int` (`decodeIfPresent`): missing or null is `None`.
fn swift_int_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    let v: Option<serde_json::Value> = Option::deserialize(d)?;
    match v {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(v) => swift_int(v).map(Some).map_err(serde::de::Error::custom),
    }
}

impl Default for ArenaEndpoint {
    fn default() -> Self {
        ArenaEndpoint { host: "localhost".to_string(), port: 8080 }
    }
}

impl ArenaEndpoint {
    pub fn new(host: impl Into<String>, port: i64) -> Self {
        ArenaEndpoint { host: host.into(), port }
    }

    /// The endpoint is this computer.
    pub fn is_local(&self) -> bool {
        matches!(self.host.to_lowercase().as_str(), "localhost" | "127.0.0.1" | "::1")
    }

    /// `http://host:port/api/v1`.
    pub fn api_base(&self) -> String {
        format!("http://{}:{}/api/v1", self.host_for_url(), self.port)
    }

    pub(crate) fn host_for_url(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') { format!("[{}]", self.host) } else { self.host.clone() }
    }
}

/// Settings shared by the menus and the loader. Decoding tolerates missing keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendEhrSettings {
    pub endpoint: ArenaEndpoint,
    pub remaps: Vec<PathRemap>,
    pub occupied_slot_mode: OccupiedSlotMode,
    /// Connect (trigger) the first loaded clip. Never on by default.
    pub trigger_after_load: bool,
    /// With Trigger After Load on and several new layers: all, or the top one only.
    pub trigger_scope: TriggerScope,
    /// Empty columns listed past the deck's last occupied column before the rest fold into
    /// "More Columns" submenus.
    pub extra_empty_columns: i64,
    /// Order of a multi-file selection / expanded folder in consecutive columns.
    pub sort_key: SortKey,
    pub sort_ascending: bool,
    /// Folders expand recursively when on; only their direct files when off.
    pub include_subfolders: bool,
    /// Ask once before loading when folders expand to more files than this.
    pub large_folder_threshold: i64,
    /// Live websocket, on-demand fetch, or a fetch every `update_interval` seconds.
    pub update_mode: ArenaUpdateMode,
    /// Seconds between fetches in Interval mode.
    pub update_interval: i64,
    /// True once the user picked a mode. False: the host's default applies at start (an older
    /// file without the key: false for Live, true for On Demand / Interval).
    pub update_mode_chosen: bool,
}

impl Default for SendEhrSettings {
    fn default() -> Self {
        SendEhrSettings {
            endpoint: ArenaEndpoint::default(),
            remaps: Vec::new(),
            occupied_slot_mode: OccupiedSlotMode::Ask,
            trigger_after_load: false,
            trigger_scope: TriggerScope::AllNewLayers,
            extra_empty_columns: 8,
            sort_key: SortKey::Name,
            sort_ascending: true,
            include_subfolders: false,
            large_folder_threshold: 50,
            update_mode: ArenaUpdateMode::Live,
            update_interval: 60,
            update_mode_chosen: false,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsRepr {
    #[serde(default)]
    endpoint: Option<ArenaEndpoint>,
    #[serde(default)]
    remaps: Option<Vec<PathRemap>>,
    #[serde(default)]
    occupied_slot_mode: Option<OccupiedSlotMode>,
    #[serde(default)]
    trigger_after_load: Option<bool>,
    #[serde(default)]
    trigger_scope: Option<TriggerScope>,
    #[serde(default, deserialize_with = "swift_int_opt")]
    extra_empty_columns: Option<i64>,
    #[serde(default)]
    sort_key: Option<SortKey>,
    #[serde(default)]
    sort_ascending: Option<bool>,
    #[serde(default)]
    include_subfolders: Option<bool>,
    #[serde(default, deserialize_with = "swift_int_opt")]
    large_folder_threshold: Option<i64>,
    #[serde(default)]
    update_mode: Option<ArenaUpdateMode>,
    #[serde(default, deserialize_with = "swift_int_opt")]
    update_interval: Option<i64>,
    #[serde(default)]
    update_mode_chosen: Option<bool>,
}

impl<'de> Deserialize<'de> for SendEhrSettings {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // A keyed container: an object, never an array.
        let v = serde_json::Value::deserialize(d)?;
        if !v.is_object() {
            return Err(serde::de::Error::custom("Expected to decode Dictionary<String, Any> but found an array or a value instead."));
        }
        let r = SettingsRepr::deserialize(v).map_err(serde::de::Error::custom)?;
        let def = SendEhrSettings::default();
        let update_mode = r.update_mode.unwrap_or(def.update_mode);
        Ok(SendEhrSettings {
            endpoint: r.endpoint.unwrap_or(def.endpoint),
            remaps: r.remaps.unwrap_or(def.remaps),
            occupied_slot_mode: r.occupied_slot_mode.unwrap_or(def.occupied_slot_mode),
            trigger_after_load: r.trigger_after_load.unwrap_or(def.trigger_after_load),
            trigger_scope: r.trigger_scope.unwrap_or(def.trigger_scope),
            extra_empty_columns: r.extra_empty_columns.unwrap_or(def.extra_empty_columns),
            sort_key: r.sort_key.unwrap_or(def.sort_key),
            sort_ascending: r.sort_ascending.unwrap_or(def.sort_ascending),
            include_subfolders: r.include_subfolders.unwrap_or(def.include_subfolders),
            large_folder_threshold: r.large_folder_threshold.unwrap_or(def.large_folder_threshold),
            update_mode,
            update_interval: r.update_interval.unwrap_or(def.update_interval).max(1),
            // Files without the flag: every mode other than Live came from a pick.
            update_mode_chosen: r.update_mode_chosen.unwrap_or(update_mode != ArenaUpdateMode::Live),
        })
    }
}

impl SendEhrSettings {
    /// "Live", "On Demand", "Every 60 s".
    pub fn update_description(&self) -> String {
        if self.update_mode == ArenaUpdateMode::Interval {
            format!("Every {}", ArenaUpdateMode::interval_title(self.update_interval))
        } else {
            self.update_mode.title().to_string()
        }
    }

    /// "Name ↑", "Date Modified ↓".
    pub fn sort_description(&self) -> String {
        format!("{} {}", self.sort_key.title(), if self.sort_ascending { "\u{2191}" } else { "\u{2193}" })
    }

    /// One line of the load-relevant settings, logged per request.
    pub fn log_summary(&self) -> String {
        format!(
            "sort {}, subfolders {}, ask over {}, trigger {} ({})",
            self.sort_description(),
            if self.include_subfolders { "on" } else { "off" },
            self.large_folder_threshold,
            if self.trigger_after_load { "on" } else { "off" },
            self.trigger_scope.title()
        )
    }
}

/// One settings change picked from a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingChange {
    SortKey(SortKey),
    SortAscending(bool),
    TriggerAfterLoad(bool),
    TriggerScope(TriggerScope),
    IncludeSubfolders(bool),
    UpdateMode(ArenaUpdateMode),
    /// Picks an interval and switches to Interval.
    UpdateInterval(i64),
}

impl SettingChange {
    pub fn applied(self, s: &SendEhrSettings) -> SendEhrSettings {
        let mut s = s.clone();
        match self {
            SettingChange::SortKey(k) => s.sort_key = k,
            SettingChange::SortAscending(a) => s.sort_ascending = a,
            SettingChange::TriggerAfterLoad(on) => s.trigger_after_load = on,
            SettingChange::TriggerScope(scope) => s.trigger_scope = scope,
            SettingChange::IncludeSubfolders(on) => s.include_subfolders = on,
            SettingChange::UpdateMode(m) => {
                s.update_mode = m;
                s.update_mode_chosen = true;
            }
            SettingChange::UpdateInterval(n) => {
                s.update_interval = n.max(1);
                s.update_mode = ArenaUpdateMode::Interval;
                s.update_mode_chosen = true;
            }
        }
        s
    }

    pub fn log_line(self) -> String {
        match self {
            SettingChange::SortKey(k) => format!("Sort by {}", k.title()),
            SettingChange::SortAscending(a) => {
                format!("Sort {}", if a { "ascending" } else { "descending" })
            }
            SettingChange::TriggerAfterLoad(on) => {
                format!("Trigger After Load {}", if on { "ON" } else { "OFF" })
            }
            SettingChange::TriggerScope(scope) => format!("Trigger Scope {}", scope.title()),
            SettingChange::IncludeSubfolders(on) => {
                format!("Include Subfolders {}", if on { "ON" } else { "OFF" })
            }
            SettingChange::UpdateMode(m) => format!("Arena Updates {}", m.title()),
            SettingChange::UpdateInterval(n) => {
                format!("Arena Updates every {}", ArenaUpdateMode::interval_title(n))
            }
        }
    }
}

/// Swift's synthesised shape: `{"sortKey":{"_0":"name"}}`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SettingChangeRepr {
    SortKey {
        #[serde(rename = "_0")]
        v: SortKey,
    },
    SortAscending {
        #[serde(rename = "_0")]
        v: bool,
    },
    TriggerAfterLoad {
        #[serde(rename = "_0")]
        v: bool,
    },
    TriggerScope {
        #[serde(rename = "_0")]
        v: TriggerScope,
    },
    IncludeSubfolders {
        #[serde(rename = "_0")]
        v: bool,
    },
    UpdateMode {
        #[serde(rename = "_0")]
        v: ArenaUpdateMode,
    },
    UpdateInterval {
        #[serde(rename = "_0")]
        v: i64,
    },
}

impl Serialize for SettingChange {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let r = match *self {
            SettingChange::SortKey(v) => SettingChangeRepr::SortKey { v },
            SettingChange::SortAscending(v) => SettingChangeRepr::SortAscending { v },
            SettingChange::TriggerAfterLoad(v) => SettingChangeRepr::TriggerAfterLoad { v },
            SettingChange::TriggerScope(v) => SettingChangeRepr::TriggerScope { v },
            SettingChange::IncludeSubfolders(v) => SettingChangeRepr::IncludeSubfolders { v },
            SettingChange::UpdateMode(v) => SettingChangeRepr::UpdateMode { v },
            SettingChange::UpdateInterval(v) => SettingChangeRepr::UpdateInterval { v },
        };
        r.serialize(s)
    }
}

impl<'de> Deserialize<'de> for SettingChange {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match SettingChangeRepr::deserialize(d)? {
            SettingChangeRepr::SortKey { v } => SettingChange::SortKey(v),
            SettingChangeRepr::SortAscending { v } => SettingChange::SortAscending(v),
            SettingChangeRepr::TriggerAfterLoad { v } => SettingChange::TriggerAfterLoad(v),
            SettingChangeRepr::TriggerScope { v } => SettingChange::TriggerScope(v),
            SettingChangeRepr::IncludeSubfolders { v } => SettingChange::IncludeSubfolders(v),
            SettingChangeRepr::UpdateMode { v } => SettingChange::UpdateMode(v),
            SettingChangeRepr::UpdateInterval { v } => SettingChange::UpdateInterval(v),
        })
    }
}

/// One remembered pick, shown under "Recent" (last 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentTarget {
    pub destination: Destination,
    pub label: String,
    pub date: Timestamp,
}

impl RecentTarget {
    pub const LIMIT: usize = 5;

    pub fn new(destination: Destination, label: impl Into<String>) -> Self {
        RecentTarget { destination, label: label.into(), date: Timestamp::now() }
    }

    /// `new` in front, an older entry for the same destination dropped, at most [`Self::LIMIT`].
    /// New layers are not targets to repeat: the list is returned unchanged.
    pub fn pushing(new: RecentTarget, list: &[RecentTarget]) -> Vec<RecentTarget> {
        if new.destination.layer_id().is_none() {
            return list.to_vec();
        }
        let mut out: Vec<RecentTarget> = list.iter().filter(|r| r.destination != new.destination).cloned().collect();
        out.insert(0, new);
        out.truncate(Self::LIMIT);
        out
    }
}
