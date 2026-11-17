// SPDX-License-Identifier: MIT
//! Where a selection of files goes, and the pure, side-effect-free plan for it.

use crate::error::ArenaError;
use crate::media::FolderGroup;
use crate::snapshot::CompositionSnapshot;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Where a selection of files should go. Layers are addressed by Arena's stable layer id and
/// resolved to an index against a fresh snapshot right before loading, so a reordered comp never
/// hits the wrong layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Destination {
    /// Fill consecutive columns starting at `column` (1-based).
    Slot { layer_id: i64, column: i64 },
    /// Each file into the next empty slot of the layer, left to right.
    NextFree { layer_id: i64 },
    /// Add one new column per file at the end of the deck, load there.
    AppendColumns { layer_id: i64 },
    /// Add a layer at the top (end of the list) and load from column 1.
    NewLayer,
    /// Same as `NewLayer`, then the layer is renamed (Folder as New Layer).
    NewNamedLayer { name: String },
    /// Several folders: one new layer per folder, named after it, files from column 1; the first
    /// folder in Sort By order is the top-most new layer. Loose files are ignored.
    LayerPerFolder,
}

impl Destination {
    pub fn layer_id(&self) -> Option<i64> {
        match self {
            Destination::Slot { layer_id, .. } | Destination::NextFree { layer_id } | Destination::AppendColumns { layer_id } => Some(*layer_id),
            Destination::NewLayer | Destination::NewNamedLayer { .. } | Destination::LayerPerFolder => None,
        }
    }

    /// The name a new layer gets, if any.
    pub fn new_layer_name(&self) -> Option<&str> {
        match self {
            Destination::NewNamedLayer { name } => Some(name),
            _ => None,
        }
    }
}

/// Swift's synthesised shape: `{"slot":{"layerID":5,"column":3}}`, `{"newLayer":{}}`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum DestinationRepr {
    Slot {
        #[serde(rename = "layerID")]
        layer_id: i64,
        column: i64,
    },
    NextFree {
        #[serde(rename = "layerID")]
        layer_id: i64,
    },
    AppendColumns {
        #[serde(rename = "layerID")]
        layer_id: i64,
    },
    NewLayer {},
    NewNamedLayer {
        name: String,
    },
    LayerPerFolder {},
}

impl Serialize for Destination {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let r = match self.clone() {
            Destination::Slot { layer_id, column } => DestinationRepr::Slot { layer_id, column },
            Destination::NextFree { layer_id } => DestinationRepr::NextFree { layer_id },
            Destination::AppendColumns { layer_id } => DestinationRepr::AppendColumns { layer_id },
            Destination::NewLayer => DestinationRepr::NewLayer {},
            Destination::NewNamedLayer { name } => DestinationRepr::NewNamedLayer { name },
            Destination::LayerPerFolder => DestinationRepr::LayerPerFolder {},
        };
        r.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Destination {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match DestinationRepr::deserialize(d)? {
            DestinationRepr::Slot { layer_id, column } => Destination::Slot { layer_id, column },
            DestinationRepr::NextFree { layer_id } => Destination::NextFree { layer_id },
            DestinationRepr::AppendColumns { layer_id } => Destination::AppendColumns { layer_id },
            DestinationRepr::NewLayer {} => Destination::NewLayer,
            DestinationRepr::NewNamedLayer { name } => Destination::NewNamedLayer { name },
            DestinationRepr::LayerPerFolder {} => Destination::LayerPerFolder,
        })
    }
}

/// What to do with a file whose target slot already holds a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictResolution {
    Replace,
    Skip,
    Cancel,
}

impl ConflictResolution {
    /// "replace" / "skip" / "cancel" (log lines).
    pub fn raw_value(self) -> &'static str {
        match self {
            ConflictResolution::Replace => "replace",
            ConflictResolution::Skip => "skip",
            ConflictResolution::Cancel => "cancel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub column: usize,
    pub path: String,
    /// The clip name currently in the slot, when occupied.
    pub replacing: Option<String>,
}

/// What to add, and which file goes where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadPlan {
    /// Layer index the files land on (after `add_layer`, if any).
    pub layer_index: usize,
    pub add_layer: bool,
    /// Columns to append at the end of the deck before loading.
    pub add_columns: usize,
    pub assignments: Vec<Assignment>,
}

impl LoadPlan {
    pub fn conflicts(&self) -> Vec<Assignment> {
        self.assignments.iter().filter(|a| a.replacing.is_some()).cloned().collect()
    }

    /// Applies the user's answer for occupied slots (`None` = cancelled).
    pub fn resolving(&self, resolution: ConflictResolution) -> Option<LoadPlan> {
        match resolution {
            ConflictResolution::Replace => Some(self.clone()),
            ConflictResolution::Cancel => None,
            ConflictResolution::Skip => {
                let mut p = self.clone();
                p.assignments.retain(|a| a.replacing.is_none());
                Some(p)
            }
        }
    }
}

fn from_column(files: &[String], first: usize) -> Vec<Assignment> {
    files.iter().enumerate().map(|(i, p)| Assignment { column: first + i, path: p.clone(), replacing: None }).collect()
}

pub struct LoadPlanner;

impl LoadPlanner {
    pub fn plan(files: &[String], destination: &Destination, snap: &CompositionSnapshot) -> Result<LoadPlan, ArenaError> {
        if files.is_empty() {
            return Err(ArenaError::NoFiles);
        }
        let total = snap.columns.len();
        match destination {
            Destination::LayerPerFolder => Err(ArenaError::BadResponse("one layer per folder is planned with planLayers".to_string())),
            Destination::NewLayer | Destination::NewNamedLayer { .. } => Ok(LoadPlan {
                layer_index: snap.layers.len() + 1,
                add_layer: true,
                add_columns: files.len().saturating_sub(total),
                assignments: from_column(files, 1),
            }),
            Destination::AppendColumns { layer_id } => {
                let layer = snap.layer(*layer_id).ok_or(ArenaError::LayerGone { layer_id: *layer_id })?;
                Ok(LoadPlan { layer_index: layer.index, add_layer: false, add_columns: files.len(), assignments: from_column(files, total + 1) })
            }
            Destination::NextFree { layer_id } => {
                let layer = snap.layer(*layer_id).ok_or(ArenaError::LayerGone { layer_id: *layer_id })?;
                let mut free: Vec<usize> = (1..=total.max(1)).filter(|c| *c <= total && !layer.slot(*c).is_some_and(|s| s.is_occupied())).collect();
                let mut added = 0;
                while free.len() < files.len() {
                    added += 1;
                    free.push(total + added);
                }
                let assignments = files.iter().zip(free).map(|(p, c)| Assignment { column: c, path: p.clone(), replacing: None }).collect();
                Ok(LoadPlan { layer_index: layer.index, add_layer: false, add_columns: added, assignments })
            }
            Destination::Slot { layer_id, column } => {
                let layer = snap.layer(*layer_id).ok_or(ArenaError::LayerGone { layer_id: *layer_id })?;
                let start = usize::try_from((*column).max(1)).unwrap_or(1);
                let assignments: Vec<Assignment> = files
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let c = start + i;
                        let replacing = layer.slot(c).filter(|s| s.is_occupied()).map(|s| s.clip_name.clone().unwrap_or_else(|| "(unnamed clip)".to_string()));
                        Assignment { column: c, path: p.clone(), replacing }
                    })
                    .collect();
                let last = start + files.len() - 1;
                Ok(LoadPlan { layer_index: layer.index, add_layer: false, add_columns: last.saturating_sub(total), assignments })
            }
        }
    }

    /// One new layer per non-empty folder. All layers are added with an empty-body
    /// `POST /composition/layers/add` ("added at the end", the end being the top in Arena), so
    /// they take indexes base+1 … base+N; folder k (0-based, in Sort By order) is given index
    /// base+N-k: the first folder ends top-most and the stack reads top-down like the menu.
    pub fn plan_layers(groups: &[FolderGroup], snap: &CompositionSnapshot) -> Result<LayersPlan, ArenaError> {
        let filled: Vec<&FolderGroup> = groups.iter().filter(|g| !g.files.is_empty()).collect();
        if filled.is_empty() {
            return Err(ArenaError::NoFiles);
        }
        let base = snap.layers.len();
        let n = filled.len();
        let layers = filled
            .iter()
            .enumerate()
            .map(|(k, g)| LayerPlan { name: g.name(), folder: g.folder.clone(), layer_index: base + n - k, assignments: from_column(&g.files, 1) })
            .collect();
        let widest = filled.iter().map(|g| g.files.len()).max().unwrap_or(0);
        Ok(LayersPlan {
            base_layer_count: base,
            layers,
            add_columns: widest.saturating_sub(snap.columns.len()),
            empty_folders: groups.iter().filter(|g| g.files.is_empty()).map(|g| g.folder.clone()).collect(),
        })
    }
}

/// One new layer of a [`LayersPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerPlan {
    pub name: String,
    pub folder: String,
    /// Index after all of the plan's layers are added.
    pub layer_index: usize,
    pub assignments: Vec<Assignment>,
}

/// Folders as New Layers: what to add and which file goes where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayersPlan {
    /// Layers in the comp before adding.
    pub base_layer_count: usize,
    /// In Sort By order (first = top-most = highest index).
    pub layers: Vec<LayerPlan>,
    /// Columns to append (columns are composition-wide).
    pub add_columns: usize,
    /// Folders that expanded to no media file: no layer.
    pub empty_folders: Vec<String>,
}
