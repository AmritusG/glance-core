// SPDX-License-Identifier: MIT
//! The small, menu-ready view of Arena's current deck, built from `GET /composition` (or a
//! websocket composition push).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionSnapshot {
    pub composition_name: String,
    pub deck_name: String,
    /// Index 0 = Arena layer 1 (bottom).
    pub layers: Vec<LayerInfo>,
    /// Index 0 = column 1.
    pub columns: Vec<ColumnInfo>,
}

impl CompositionSnapshot {
    pub fn new(composition_name: impl Into<String>, deck_name: impl Into<String>, layers: Vec<LayerInfo>, columns: Vec<ColumnInfo>) -> Self {
        CompositionSnapshot { composition_name: composition_name.into(), deck_name: deck_name.into(), layers, columns }
    }

    /// Same menu layout: names, ids, occupancy and clip names. Play state (the menu's play mark)
    /// is ignored: Arena's autopilot changes it on every push.
    pub fn same_menu(a: Option<&CompositionSnapshot>, b: Option<&CompositionSnapshot>) -> bool {
        let (a, b) = match (a, b) {
            (Some(a), Some(b)) => (a, b),
            (None, None) => return true,
            _ => return false,
        };
        if a.composition_name != b.composition_name || a.deck_name != b.deck_name || a.columns != b.columns || a.layers.len() != b.layers.len() {
            return false;
        }
        for (la, lb) in a.layers.iter().zip(&b.layers) {
            if la.index != lb.index || la.id != lb.id || la.raw_name != lb.raw_name || la.name_param_id != lb.name_param_id || la.slots.len() != lb.slots.len()
            {
                return false;
            }
            for (sa, sb) in la.slots.iter().zip(&lb.slots) {
                if sa.clip_id != sb.clip_id || sa.clip_name != sb.clip_name || sa.is_occupied() != sb.is_occupied() || sa.column != sb.column {
                    return false;
                }
            }
        }
        true
    }

    pub fn layer(&self, id: i64) -> Option<&LayerInfo> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn layer_index(&self, id: i64) -> Option<usize> {
        self.layer(id).map(|l| l.index)
    }

    /// Highest column holding a clip in any layer (0 when the deck is empty).
    pub fn last_occupied_column(&self) -> usize {
        self.layers.iter().flat_map(|l| &l.slots).filter(|s| s.is_occupied()).map(|s| s.column).max().unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerInfo {
    /// 1-based.
    pub index: usize,
    pub id: i64,
    /// "Layer #" = default; `#` shows the position.
    pub raw_name: String,
    #[serde(rename = "nameParamID", skip_serializing_if = "Option::is_none", default)]
    pub name_param_id: Option<i64>,
    /// `slots[i].column == i + 1`.
    pub slots: Vec<SlotInfo>,
}

impl LayerInfo {
    pub fn new(index: usize, id: i64, raw_name: impl Into<String>, name_param_id: Option<i64>, slots: Vec<SlotInfo>) -> Self {
        LayerInfo { index, id, raw_name: raw_name.into(), name_param_id, slots }
    }

    pub fn display_name(&self) -> String {
        ArenaNames::display(&self.raw_name, self.index)
    }

    /// True when the user gave the layer a real name (not "Layer #" / "Layer N").
    pub fn has_custom_name(&self) -> bool {
        ArenaNames::is_custom(&self.raw_name, "Layer", self.index)
    }

    pub fn slot(&self, column: usize) -> Option<&SlotInfo> {
        if column >= 1 { self.slots.get(column - 1) } else { None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub index: usize,
    pub id: i64,
    pub raw_name: String,
    #[serde(rename = "nameParamID", skip_serializing_if = "Option::is_none", default)]
    pub name_param_id: Option<i64>,
}

impl ColumnInfo {
    pub fn new(index: usize, id: i64, raw_name: impl Into<String>, name_param_id: Option<i64>) -> Self {
        ColumnInfo { index, id, raw_name: raw_name.into(), name_param_id }
    }

    pub fn display_name(&self) -> String {
        ArenaNames::display(&self.raw_name, self.index)
    }

    pub fn has_custom_name(&self) -> bool {
        ArenaNames::is_custom(&self.raw_name, "Column", self.index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotInfo {
    pub column: usize,
    #[serde(rename = "clipID")]
    pub clip_id: i64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub clip_name: Option<String>,
    /// Clip `connected.value`: Empty / Disconnected / Previewing / Connected / Connected &
    /// previewing.
    pub state: String,
}

impl SlotInfo {
    pub fn new(column: usize, clip_id: i64, clip_name: Option<String>, state: impl Into<String>) -> Self {
        SlotInfo { column, clip_id, clip_name, state: state.into() }
    }

    /// `Empty` is the only state meaning "nothing loaded". Offline clips with no video track
    /// still read `Disconnected` and count as occupied.
    pub fn is_occupied(&self) -> bool {
        self.state != "Empty"
    }

    pub fn is_playing(&self) -> bool {
        self.state.starts_with("Connected")
    }
}

/// Arena's names: it stores the literal `#` and shows the position in its place.
pub struct ArenaNames;

impl ArenaNames {
    pub fn display(raw: &str, index: usize) -> String {
        raw.replace('#', &index.to_string())
    }

    pub fn is_custom(raw: &str, kind: &str, index: usize) -> bool {
        let shown = Self::display(raw, index);
        !(raw == format!("{kind} #") || shown == format!("{kind} {index}") || raw.is_empty())
    }
}
