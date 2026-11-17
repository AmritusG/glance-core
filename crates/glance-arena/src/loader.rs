// SPDX-License-Identifier: MIT
//! Fresh snapshot → plan → ask about occupied slots → add layer / columns → open each file →
//! optional trigger → verify each slot over REST. Blocking; run it off the UI thread.

use crate::client::ArenaClient;
use crate::error::ArenaError;
use crate::event_log::EventLog;
use crate::media::{ArenaMediaTypes, FolderGroup, last_path_component};
use crate::menu::SendMenuModel;
use crate::planner::{Assignment, ConflictResolution, Destination, LayersPlan, LoadPlan, LoadPlanner};
use crate::remap::{PathRemap, remap};
use crate::settings::TriggerScope;
use crate::snapshot::SlotInfo;
use std::ops::RangeInclusive;

/// How long an open's read-back waits for Arena to put the clip in the slot ([`ArenaLoader`]'s read-back).
pub const READ_BACK_SETTLE: std::time::Duration = std::time::Duration::from_millis(1000);
/// How often the read-back asks.
pub const READ_BACK_POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// One slot's result, verified by a REST GET after the open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotResult {
    pub layer_index: usize,
    pub column: usize,
    pub path: String,
    pub error: Option<ArenaError>,
    /// The slot as Arena reports it after loading (the read-back).
    pub after: Option<SlotInfo>,
}

impl SlotResult {
    pub fn ok(&self) -> bool {
        self.error.is_none() && self.after.as_ref().is_some_and(SlotInfo::is_occupied)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadOutcome {
    pub plan: LoadPlan,
    pub layer_id: Option<i64>,
    pub results: Vec<SlotResult>,
    pub triggered: bool,
    pub skipped: usize,
}

impl LoadOutcome {
    pub fn ok_count(&self) -> usize {
        self.results.iter().filter(|r| r.ok()).count()
    }

    /// "Loaded 3/3 into Layer 2, columns 3-5, skipped 1 occupied, triggered".
    pub fn summary(&self) -> String {
        let mut s = format!("Loaded {}/{} into Layer {}", self.ok_count(), self.results.len(), self.plan.layer_index);
        if let (Some(first), Some(last)) = (self.results.first(), self.results.last()) {
            if first.column == last.column {
                s += &format!(", column {}", first.column);
            } else {
                s += &format!(", columns {}-{}", first.column, last.column);
            }
        }
        if self.skipped > 0 {
            s += &format!(", skipped {} occupied", self.skipped);
        }
        if self.triggered {
            s += ", triggered";
        }
        s
    }
}

/// Result of Folders as New Layers: one [`LoadOutcome`] per new layer, top-most first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayersOutcome {
    pub plan: LayersPlan,
    pub layers: Vec<LoadOutcome>,
}

impl LayersOutcome {
    pub fn summary(&self) -> String {
        let ok: usize = self.layers.iter().map(LoadOutcome::ok_count).sum();
        let total: usize = self.layers.iter().map(|l| l.results.len()).sum();
        let names: Vec<String> = self
            .plan
            .layers
            .iter()
            .zip(&self.layers)
            .map(|(p, o)| format!("{} \u{201C}{}\u{201D} {}/{}", p.layer_index, p.name, o.ok_count(), o.results.len()))
            .collect();
        let n = self.layers.len();
        let mut s = format!("Loaded {ok}/{total} into {n} new layer{}, top first: {}", if n == 1 { "" } else { "s" }, names.join(", "));
        if !self.plan.empty_folders.is_empty() {
            s += &format!("; {} empty folder(s), no layer", self.plan.empty_folders.len());
        }
        if self.layers.iter().any(|l| l.triggered) {
            s += ", triggered";
        }
        s
    }
}

/// Runs loads against Arena.
#[derive(Debug, Clone)]
pub struct ArenaLoader {
    pub client: ArenaClient,
    pub remaps: Vec<PathRemap>,
    pub log: Option<EventLog>,
}

impl ArenaLoader {
    pub fn new(client: ArenaClient, remaps: Vec<PathRemap>, log: Option<EventLog>) -> Self {
        ArenaLoader { client, remaps, log }
    }

    fn log(&self, text: impl Into<String>) {
        if let Some(l) = &self.log {
            l.add(text);
        }
    }

    /// - `paths`: files in load order (see [`crate::MediaCollector`]); non-media are dropped.
    /// - `replace_confirmed`: the user picked an explicit "Replace" item: no question.
    /// - `confirm`: asked once when occupied slots would be overwritten.
    /// - `expected_layers`: new-layer picks: the indices the menu showed (logged if Arena's
    ///   differ).
    pub fn load(
        &self,
        paths: &[String],
        destination: &Destination,
        trigger: bool,
        replace_confirmed: bool,
        expected_layers: Option<RangeInclusive<usize>>,
        confirm: &dyn Fn(&[Assignment]) -> ConflictResolution,
    ) -> Result<LoadOutcome, ArenaError> {
        let files: Vec<String> = paths.iter().filter(|p| ArenaMediaTypes::is_supported_path(p)).cloned().collect();
        if files.is_empty() {
            return Err(ArenaError::NoFiles);
        }
        let mut snap = self.client.snapshot()?;
        let full_plan = LoadPlanner::plan(&files, destination, &snap)?;
        let mut plan = full_plan.clone();
        let conflicts = full_plan.conflicts();
        if !conflicts.is_empty() {
            let answer = if replace_confirmed { ConflictResolution::Replace } else { confirm(&conflicts) };
            let listed: Vec<String> = conflicts.iter().map(|a| format!("col {} \"{}\"", a.column, a.replacing.clone().unwrap_or_default())).collect();
            self.log(format!("Occupied slots {} -> {}", listed.join(", "), answer.raw_value()));
            plan = full_plan.resolving(answer).ok_or(ArenaError::Cancelled)?;
        }
        let skipped = full_plan.assignments.len() - plan.assignments.len();

        let mut layer_id = destination.layer_id();
        if plan.add_layer {
            self.client.add_layer()?;
            snap = self.client.snapshot()?;
            let new_layer = match snap.layers.last() {
                Some(l) if snap.layers.len() == plan.layer_index => l.clone(),
                _ => {
                    return Err(ArenaError::BadResponse(format!("new layer not found at index {} (layers: {})", plan.layer_index, snap.layers.len())));
                }
            };
            layer_id = Some(new_layer.id);
            self.log(format!("Added layer at top: index {}, id {}", new_layer.index, new_layer.id));
            if let Some(note) = SendMenuModel::layer_mismatch(expected_layers, &(new_layer.index..=new_layer.index)) {
                self.log(note);
            }
            if let Some(name) = destination.new_layer_name() {
                self.client.rename_layer(new_layer.id, name)?;
                self.log(format!("Named layer {} \u{201C}{name}\u{201D}", new_layer.index));
            }
        } else if let Some(id) = layer_id
            && snap.layer_index(id) != Some(plan.layer_index)
        {
            return Err(ArenaError::LayerGone { layer_id: id });
        }
        if plan.add_columns > 0 && !plan.assignments.is_empty() {
            let max_col = plan.assignments.iter().map(|a| a.column).max().unwrap_or(0);
            let needed = max_col.saturating_sub(snap.columns.len());
            for _ in 0..needed {
                self.client.add_column()?;
            }
            if needed > 0 {
                self.log(format!("Added {needed} column(s) at the end"));
            }
        }

        let results = self.open(&plan.assignments, plan.layer_index);
        let triggered = if trigger { self.trigger_first(&results)? } else { false };
        Ok(LoadOutcome { plan, layer_id, results, triggered, skipped })
    }

    /// Folders as New Layers: one fresh snapshot to plan, N empty-body layer adds (each at the
    /// top), one snapshot to confirm the N new layers and read their ids, then per layer
    /// (top-most = first folder): rename by id, open from column 1, verify each slot. With
    /// `trigger`, the first loaded clip of each new layer `scope` selects is triggered.
    pub fn load_layers(
        &self,
        groups: &[FolderGroup],
        trigger: bool,
        scope: TriggerScope,
        expected_layers: Option<RangeInclusive<usize>>,
    ) -> Result<LayersOutcome, ArenaError> {
        let snap = self.client.snapshot()?;
        let plan = LoadPlanner::plan_layers(groups, &snap)?;
        for folder in &plan.empty_folders {
            self.log(format!("Folder \u{201C}{}\u{201D} has no media file - no layer", last_path_component(folder)));
        }
        for _ in &plan.layers {
            self.client.add_layer()?;
        }
        let snap = self.client.snapshot()?;
        let expected = plan.base_layer_count + plan.layers.len();
        if snap.layers.len() != expected {
            return Err(ArenaError::BadResponse(format!("expected {expected} layers after adding {}, found {}", plan.layers.len(), snap.layers.len())));
        }
        if let Some(note) = SendMenuModel::layer_mismatch(expected_layers, &((plan.base_layer_count + 1)..=expected)) {
            self.log(note);
        }
        let max_col = plan.layers.iter().flat_map(|l| &l.assignments).map(|a| a.column).max().unwrap_or(0);
        let needed = max_col.saturating_sub(snap.columns.len());
        for _ in 0..needed {
            self.client.add_column()?;
        }
        if needed > 0 {
            self.log(format!("Added {needed} column(s) at the end"));
        }

        let mut outcomes = Vec::new();
        for (position, layer) in plan.layers.iter().enumerate() {
            // position 0 = top-most
            let Some(info) = layer.layer_index.checked_sub(1).and_then(|i| snap.layers.get(i)) else {
                return Err(ArenaError::BadResponse(format!("new layer not found at index {}", layer.layer_index)));
            };
            self.client.rename_layer(info.id, &layer.name)?;
            self.log(format!("Added layer {} (id {}) \u{201C}{}\u{201D}: {} file(s)", layer.layer_index, info.id, layer.name, layer.assignments.len()));
            let results = self.open(&layer.assignments, layer.layer_index);
            let triggered = if trigger && scope.triggers(position) { self.trigger_first(&results)? } else { false };
            let single = LoadPlan { layer_index: layer.layer_index, add_layer: true, add_columns: 0, assignments: layer.assignments.clone() };
            outcomes.push(LoadOutcome { plan: single, layer_id: Some(info.id), results, triggered, skipped: 0 });
        }
        Ok(LayersOutcome { plan, layers: outcomes })
    }

    /// Opens each file into its slot and reads the slot back.
    fn open(&self, assignments: &[Assignment], layer: usize) -> Vec<SlotResult> {
        let mut results = Vec::new();
        for a in assignments {
            let arena_path = remap(&self.remaps, &a.path);
            let mut r = SlotResult { layer_index: layer, column: a.column, path: a.path.clone(), error: None, after: None };
            let done = self.client.open(layer, a.column, &arena_path).and_then(|()| self.read_back(layer, a.column));
            match done {
                Ok(after) => {
                    self.log(format!("open L{layer} C{} <- {arena_path} => {} \"{}\"", a.column, after.state, after.clip_name.clone().unwrap_or_default()));
                    r.after = Some(after);
                }
                Err(e) => {
                    self.log(format!("open L{layer} C{} <- {arena_path} FAILED: {}", a.column, e.plain_line()));
                    r.error = Some(e);
                }
            }
            results.push(r);
        }
        results
    }

    /// The slot after an open. Arena answers the open before the clip is in the slot (measured on Arena 7.28.1: the
    /// slot reads "Empty" for 13–64 ms after the answer in 46 of 48 opens), so a single read straight after the open
    /// — what SendEhrKit 1.1.2 does — counts loaded clips as failed and makes Trigger After Load into empty slots
    /// trigger nothing. The slot is read again every 20 ms until it is occupied, for at most [`READ_BACK_SETTLE`];
    /// a slot still empty then is reported as it reads (a real failure). Fixed on purpose; differs from Swift.
    fn read_back(&self, layer: usize, column: usize) -> Result<SlotInfo, ArenaError> {
        let started = std::time::Instant::now();
        loop {
            let slot = self.client.slot(layer, column)?;
            if slot.is_occupied() || started.elapsed() >= READ_BACK_SETTLE {
                return Ok(slot);
            }
            std::thread::sleep(READ_BACK_POLL);
        }
    }

    fn trigger_first(&self, results: &[SlotResult]) -> Result<bool, ArenaError> {
        let Some(first) = results.iter().find(|r| r.ok()) else { return Ok(false) };
        self.client.connect(first.layer_index, first.column)?;
        self.log(format!("Triggered L{} C{}", first.layer_index, first.column));
        Ok(true)
    }
}
