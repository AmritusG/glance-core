// SPDX-License-Identifier: MIT
//! The platform-neutral "Send to Arena" menu tree. A host renders it (egui, a native menu); every
//! title is the kit's.

use crate::error::{ArenaError, ArenaStatus};
use crate::media::SortKey;
use crate::planner::{Destination, LoadPlanner};
use crate::settings::{ArenaUpdateMode, RecentTarget, SendEhrSettings, SettingChange, TriggerScope};
use crate::snapshot::{CompositionSnapshot, LayerInfo, SlotInfo};
use crate::swift_json::Timestamp;
use std::ops::RangeInclusive;

/// What a menu item does when picked.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MenuAction {
    Load {
        destination: Destination,
        replace: bool,
    },
    /// Launch / show the host app (Finder extension only).
    OpenHost,
    /// Change a setting (Sort By, Include Subfolders, Trigger After Load, Arena Updates).
    Setting(SettingChange),
    /// Fetch the composition now (On Demand / Interval).
    Refresh,
}

impl MenuAction {
    fn load(destination: Destination, replace: bool) -> Self {
        MenuAction::Load { destination, replace }
    }
}

/// One menu entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuNode {
    Action {
        title: String,
        action: MenuAction,
        label: String,
    },
    Submenu {
        title: String,
        children: Vec<MenuNode>,
    },
    /// An item with a checkmark showing its current state.
    Toggle {
        title: String,
        is_on: bool,
        action: MenuAction,
    },
    /// A disabled line that explains something.
    Info(String),
    Separator,
}

impl MenuNode {
    pub fn title(&self) -> Option<&str> {
        match self {
            MenuNode::Action { title, .. } | MenuNode::Submenu { title, .. } | MenuNode::Toggle { title, .. } => Some(title),
            MenuNode::Info(t) => Some(t),
            MenuNode::Separator => None,
        }
    }

    fn action(title: impl Into<String>, action: MenuAction, label: impl Into<String>) -> Self {
        MenuNode::Action { title: title.into(), action, label: label.into() }
    }

    fn toggle(title: impl Into<String>, is_on: bool, action: MenuAction) -> Self {
        MenuNode::Toggle { title: title.into(), is_on, action }
    }

    fn submenu(title: impl Into<String>, children: Vec<MenuNode>) -> Self {
        MenuNode::Submenu { title: title.into(), children }
    }

    fn info(t: impl Into<String>) -> Self {
        MenuNode::Info(t.into())
    }
}

/// Everything the menu is built from.
#[derive(Debug, Clone)]
pub struct SendMenuInput {
    pub snapshot: Option<CompositionSnapshot>,
    pub status: ArenaStatus,
    pub host_alive: bool,
    pub recents: Vec<RecentTarget>,
    pub settings: SendEhrSettings,
    /// Media files selected directly.
    pub file_count: usize,
    pub unsupported_count: usize,
    /// Selected folders: they expand when loading, so their file count is not known while the
    /// menu is built.
    pub folder_names: Vec<String>,
    /// When the composition last arrived (On Demand / Interval show "Updated n s ago").
    pub updated_at: Option<Timestamp>,
    pub now: Timestamp,
}

impl SendMenuInput {
    pub fn new(snapshot: Option<CompositionSnapshot>, status: ArenaStatus, file_count: usize) -> Self {
        SendMenuInput {
            snapshot,
            status,
            host_alive: true,
            recents: Vec::new(),
            settings: SendEhrSettings::default(),
            file_count,
            unsupported_count: 0,
            folder_names: Vec::new(),
            updated_at: None,
            now: Timestamp::now(),
        }
    }

    /// File count when it is known (no folders).
    pub fn known_count(&self) -> Option<usize> {
        if self.folder_names.is_empty() { Some(self.file_count) } else { None }
    }

    pub fn has_items(&self) -> bool {
        self.file_count > 0 || !self.folder_names.is_empty()
    }
}

pub struct SendMenuModel;

impl SendMenuModel {
    pub const ROOT_TITLE: &'static str = "Send to Arena";
    const CHUNK: usize = 50;

    /// The children of the "Send to Arena" submenu. Never empty: when Arena can't be used it is
    /// one disabled line saying why.
    pub fn build(input: &SendMenuInput) -> Vec<MenuNode> {
        if !input.host_alive {
            return vec![MenuNode::info("SendEhr is not running"), MenuNode::action("Open SendEhr", MenuAction::OpenHost, "Open SendEhr")];
        }
        if !input.has_items() {
            return vec![MenuNode::info(ArenaError::NoFiles.plain_line())];
        }
        let snap = match (&input.snapshot, input.status.is_connected()) {
            (Some(s), true) => s,
            _ => {
                let line = if input.status == ArenaStatus::Unknown { "Connecting to Arena...".to_string() } else { input.status.plain_line() };
                let mut out = vec![MenuNode::info(line)];
                out.extend(Self::freshness_nodes(input));
                return out;
            }
        };

        let mut items: Vec<MenuNode> = Vec::new();
        let known = input.known_count();
        let n = known.unwrap_or(input.file_count.max(2)); // folders: plural wording, ranges hidden
        let folders = input.folder_names.len();
        let sort_note = format!("sorted by {}", input.settings.sort_description());
        if folders > 0 {
            let mut what = if folders == 1 { format!("Folder \u{201C}{}\u{201D}", input.folder_names[0]) } else { format!("{folders} folders") };
            if input.file_count > 0 {
                what += &format!(" + {} file{}", input.file_count, if input.file_count == 1 { "" } else { "s" });
            }
            items.push(MenuNode::info(format!("{what} -> consecutive columns, {sort_note}")));
        } else if n > 1 {
            items.push(MenuNode::info(format!("{n} files -> consecutive columns from your pick, {sort_note}")));
        }
        if input.unsupported_count > 0 {
            items.push(MenuNode::info(format!("{} skipped (not a type Arena opens)", input.unsupported_count)));
        }

        let recent: Vec<MenuNode> = input.recents.iter().filter_map(|r| Self::recent_node(r, snap, &input.settings)).collect();
        if !recent.is_empty() {
            items.push(MenuNode::info("Recent"));
            items.extend(recent);
            items.push(MenuNode::Separator);
        } else if !items.is_empty() {
            items.push(MenuNode::Separator);
        }

        let new_index = snap.layers.len() + 1;
        items.push(MenuNode::action(format!("New Layer at Top (Layer {new_index})"), MenuAction::load(Destination::NewLayer, false), "New Layer"));
        if folders == 1 {
            let name = input.folder_names[0].clone();
            let dest = Destination::NewNamedLayer { name: name.clone() };
            let label = format!("Folder as New Layer \u{201C}{name}\u{201D}");
            let suffix = Self::layers_suffix(&dest, snap, 1);
            items.push(MenuNode::action(format!("{label}{suffix}"), MenuAction::load(dest, false), label));
        } else if folders > 1 {
            let label = format!("Folders as New Layers ({folders})");
            let suffix = Self::layers_suffix(&Destination::LayerPerFolder, snap, folders);
            items.push(MenuNode::action(format!("{label}{suffix}"), MenuAction::load(Destination::LayerPerFolder, false), label));
        }
        items.push(MenuNode::Separator);

        for layer in snap.layers.iter().rev() {
            // Arena shows the highest layer on top.
            items.push(MenuNode::submenu(Self::layer_title(layer), Self::layer_children(layer, snap, &input.settings, n, known.is_some())));
        }
        items.push(MenuNode::Separator);
        items.extend(Self::setting_nodes(&input.settings, folders > 0, folders > 1, false));
        items.push(MenuNode::Separator);
        let deck = if snap.deck_name.is_empty() { String::new() } else { format!("Deck \u{201C}{}\u{201D} \u{00B7} ", snap.deck_name) };
        items.push(MenuNode::info(format!("{deck}{}", snap.composition_name)));
        items.extend(Self::freshness_nodes(input));
        items
    }

    /// On Demand / Interval: "Updated n s ago" + Refresh (the labels can be one fetch behind; a
    /// load always re-reads a fresh composition). Nothing in Live.
    pub fn freshness_nodes(input: &SendMenuInput) -> Vec<MenuNode> {
        if input.settings.update_mode == ArenaUpdateMode::Live {
            return Vec::new();
        }
        vec![MenuNode::info(Self::updated_text(input.updated_at, input.now)), MenuNode::action("Refresh", MenuAction::Refresh, "Refresh")]
    }

    /// "Updated just now", "Updated 12 s ago", "Updated 3 min ago", "Not updated yet".
    pub fn updated_text(updated_at: Option<Timestamp>, now: Timestamp) -> String {
        let Some(up) = updated_at else { return "Not updated yet".to_string() };
        let s = (now.seconds_since(up) as i64).max(0);
        if s < 1 {
            "Updated just now".to_string()
        } else if s < 120 {
            format!("Updated {s} s ago")
        } else if s < 7200 {
            format!("Updated {} min ago", s / 60)
        } else {
            format!("Updated {} h ago", s / 3600)
        }
    }

    /// Menu bar form: "Arena Updates: Live / On Demand / Interval (1 min)" checkmarked, then the
    /// "Interval (1 min)" submenu with the choices.
    pub fn inline_update_nodes(s: &SendEhrSettings) -> Vec<MenuNode> {
        let MenuNode::Submenu { children, .. } = Self::update_nodes(s) else { return Vec::new() };
        let mut out = vec![MenuNode::Separator];
        for node in children.iter().take(3) {
            out.push(match node {
                MenuNode::Toggle { title, is_on, action } => MenuNode::toggle(format!("Arena Updates: {title}"), *is_on, action.clone()),
                other => other.clone(),
            });
        }
        out.push(MenuNode::submenu(format!("Interval ({})", ArenaUpdateMode::interval_title(s.update_interval)), children.iter().skip(4).cloned().collect()));
        out
    }

    /// The "Arena Updates (Live)" submenu: the three modes, then the interval choices.
    pub fn update_nodes(s: &SendEhrSettings) -> MenuNode {
        let mut children: Vec<MenuNode> = ArenaUpdateMode::ALL
            .iter()
            .map(|m| {
                let title = if *m == ArenaUpdateMode::Interval {
                    format!("Interval ({})", ArenaUpdateMode::interval_title(s.update_interval))
                } else {
                    m.title().to_string()
                };
                MenuNode::toggle(title, s.update_mode == *m, MenuAction::Setting(SettingChange::UpdateMode(*m)))
            })
            .collect();
        children.push(MenuNode::Separator);
        children.extend(ArenaUpdateMode::INTERVALS.iter().map(|n| {
            MenuNode::toggle(
                format!("Every {}", ArenaUpdateMode::interval_title(*n)),
                s.update_mode == ArenaUpdateMode::Interval && s.update_interval == *n,
                MenuAction::Setting(SettingChange::UpdateInterval(*n)),
            )
        }));
        MenuNode::submenu(format!("Arena Updates ({})", s.update_description()), children)
    }

    /// The layer indices a new-layer pick will create, read from `snap` (the cache at menu
    /// time): one layer above the current top, or one per folder for Folders as New Layers. The
    /// load re-reads a fresh snapshot and logs when Arena's real indices differ.
    pub fn expected_layers(destination: &Destination, snap: Option<&CompositionSnapshot>, folder_count: usize) -> Option<RangeInclusive<usize>> {
        let base = snap?.layers.len();
        match destination {
            Destination::NewLayer | Destination::NewNamedLayer { .. } => Some((base + 1)..=(base + 1)),
            Destination::LayerPerFolder => {
                if folder_count > 0 {
                    Some((base + 1)..=(base + folder_count))
                } else {
                    None
                }
            }
            Destination::Slot { .. } | Destination::NextFree { .. } | Destination::AppendColumns { .. } => None,
        }
    }

    /// "Layer 24" or "Layers 24–26".
    pub fn layers_text(r: &RangeInclusive<usize>) -> String {
        if r.start() == r.end() { format!("Layer {}", r.start()) } else { format!("Layers {}\u{2013}{}", r.start(), r.end()) }
    }

    fn layers_suffix(destination: &Destination, snap: &CompositionSnapshot, folder_count: usize) -> String {
        Self::expected_layers(destination, Some(snap), folder_count).map(|r| format!(" - {}", Self::layers_text(&r))).unwrap_or_default()
    }

    /// The log line when the layers Arena made differ from what the menu showed.
    pub fn layer_mismatch(expected: Option<RangeInclusive<usize>>, actual: &RangeInclusive<usize>) -> Option<String> {
        let expected = expected?;
        if &expected == actual {
            return None;
        }
        Some(format!("Menu showed {} (cache at menu time); Arena made {}", Self::layers_text(&expected), Self::layers_text(actual)))
    }

    pub fn layer_title(layer: &LayerInfo) -> String {
        if layer.has_custom_name() { format!("Layer {} \u{201C}{}\u{201D}", layer.index, layer.display_name()) } else { format!("Layer {}", layer.index) }
    }

    pub fn column_title(column: usize, snap: &CompositionSnapshot) -> String {
        match column.checked_sub(1).and_then(|i| snap.columns.get(i)) {
            Some(info) if info.has_custom_name() => {
                format!("Column {column} \u{201C}{}\u{201D}", info.display_name())
            }
            _ => format!("Column {column}"),
        }
    }

    fn slot_suffix(slot: Option<&SlotInfo>) -> String {
        match slot {
            Some(s) if s.is_occupied() => {
                let name = s.clip_name.clone().unwrap_or_else(|| "(unnamed clip)".to_string());
                format!("{}{name}", if s.is_playing() { "\u{25B6} " } else { "\u{25CF} " })
            }
            _ => "(empty)".to_string(),
        }
    }

    fn free_columns(layer: &LayerInfo, snap: &CompositionSnapshot, count: usize) -> Vec<usize> {
        let files = vec!["x.mov".to_string(); count];
        LoadPlanner::plan(&files, &Destination::NextFree { layer_id: layer.id }, snap)
            .map(|p| p.assignments.iter().map(|a| a.column).collect())
            .unwrap_or_default()
    }

    fn range(cols: &[usize]) -> String {
        let (Some(f), Some(l)) = (cols.first(), cols.last()) else { return String::new() };
        if cols.len() == 1 {
            return f.to_string();
        }
        if l + 1 - f == cols.len() {
            return format!("{f}\u{2013}{l}");
        }
        cols.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
    }

    /// Sort By (one shared setting), Include Subfolders (folders only), Trigger After Load, the
    /// Trigger Scope pair (only where several new layers can be made: 2+ folders, always in a
    /// menu bar), Arena Updates. `updates_inline`: the three modes as top-level checkmarked items
    /// (+ an "Interval" submenu) instead of the "Arena Updates (…)" submenu.
    pub fn setting_nodes(s: &SendEhrSettings, folders: bool, trigger_scope: bool, updates_inline: bool) -> Vec<MenuNode> {
        let mut sort: Vec<MenuNode> =
            SortKey::ALL.iter().map(|k| MenuNode::toggle(k.title(), s.sort_key == *k, MenuAction::Setting(SettingChange::SortKey(*k)))).collect();
        sort.push(MenuNode::Separator);
        sort.push(MenuNode::toggle("Ascending", s.sort_ascending, MenuAction::Setting(SettingChange::SortAscending(true))));
        sort.push(MenuNode::toggle("Descending", !s.sort_ascending, MenuAction::Setting(SettingChange::SortAscending(false))));
        let mut out = vec![MenuNode::submenu(format!("Sort By ({})", s.sort_description()), sort)];
        if folders {
            out.push(MenuNode::toggle(
                "Include Subfolders",
                s.include_subfolders,
                MenuAction::Setting(SettingChange::IncludeSubfolders(!s.include_subfolders)),
            ));
        }
        out.push(MenuNode::toggle("Trigger After Load", s.trigger_after_load, MenuAction::Setting(SettingChange::TriggerAfterLoad(!s.trigger_after_load))));
        if trigger_scope {
            out.extend(
                TriggerScope::ALL
                    .iter()
                    .map(|t| MenuNode::toggle(format!("Trigger: {}", t.title()), s.trigger_scope == *t, MenuAction::Setting(SettingChange::TriggerScope(*t)))),
            );
        }
        if updates_inline {
            out.extend(Self::inline_update_nodes(s));
        } else {
            out.push(Self::update_nodes(s));
        }
        out
    }

    fn layer_children(layer: &LayerInfo, snap: &CompositionSnapshot, settings: &SendEhrSettings, n: usize, ranges_known: bool) -> Vec<MenuNode> {
        let mut out = Vec::new();
        let plural = if n > 1 { "s" } else { "" };
        let free = Self::free_columns(layer, snap, n);
        out.push(MenuNode::action(
            format!("Next Free Column{plural}{}", if ranges_known { format!(" ({})", Self::range(&free)) } else { String::new() }),
            MenuAction::load(Destination::NextFree { layer_id: layer.id }, false),
            format!("{} \u{203A} Next Free Column", Self::layer_title(layer)),
        ));
        let total = snap.columns.len();
        let appended: Vec<usize> = ((total + 1)..=(total + n)).collect();
        out.push(MenuNode::action(
            format!("Append as New Column{plural}{}", if ranges_known { format!(" ({})", Self::range(&appended)) } else { String::new() }),
            MenuAction::load(Destination::AppendColumns { layer_id: layer.id }, false),
            format!("{} \u{203A} Append as New Column", Self::layer_title(layer)),
        ));
        out.push(MenuNode::Separator);

        let layer_last = layer.slots.iter().filter(|s| s.is_occupied()).map(|s| s.column).max().unwrap_or(0);
        let last = snap.last_occupied_column().max(layer_last);
        // Signed: a negative "extra empty columns" setting lists fewer columns (never below 0).
        let shown_signed = (i64::try_from(last).unwrap_or(i64::MAX)).saturating_add(settings.extra_empty_columns).min(i64::try_from(total).unwrap_or(i64::MAX));
        let shown = usize::try_from(shown_signed.max(0)).unwrap_or(0);
        for c in 1..=shown.max(total.min(1)) {
            out.push(Self::column_node(layer, c, snap, settings));
        }
        if shown < total {
            let mut start = shown + 1;
            let mut chunks: Vec<MenuNode> = Vec::new();
            while start <= total {
                let end = total.min(start + Self::CHUNK - 1);
                chunks.push(MenuNode::submenu(
                    format!("Columns {start}\u{2013}{end}"),
                    (start..=end).map(|c| Self::column_node(layer, c, snap, settings)).collect(),
                ));
                start = end + 1;
            }
            if chunks.len() == 1 {
                out.extend(chunks);
            } else {
                out.push(MenuNode::submenu(format!("More Columns ({}\u{2013}{total})", shown + 1), chunks));
            }
        }
        out
    }

    fn column_node(layer: &LayerInfo, c: usize, snap: &CompositionSnapshot, settings: &SendEhrSettings) -> MenuNode {
        let slot = layer.slot(c);
        let title = format!("{} \u{00B7} {}", Self::column_title(c, snap), Self::slot_suffix(slot));
        let label = format!("{} \u{203A} {}", Self::layer_title(layer), Self::column_title(c, snap));
        let dest = Destination::Slot { layer_id: layer.id, column: i64::try_from(c).unwrap_or(i64::MAX) };
        if settings.occupied_slot_mode == crate::settings::OccupiedSlotMode::ReplaceItem
            && let Some(s) = slot
            && s.is_occupied()
        {
            return MenuNode::submenu(
                title,
                vec![MenuNode::action(
                    format!("Replace \u{201C}{}\u{201D}", s.clip_name.clone().unwrap_or_else(|| "clip".to_string())),
                    MenuAction::load(dest, true),
                    label,
                )],
            );
        }
        MenuNode::action(title, MenuAction::load(dest, false), label)
    }

    fn recent_node(r: &RecentTarget, snap: &CompositionSnapshot, settings: &SendEhrSettings) -> Option<MenuNode> {
        let layer = snap.layer(r.destination.layer_id()?)?;
        match &r.destination {
            Destination::Slot { column, .. } => {
                let column = usize::try_from(*column).ok()?;
                if column > snap.columns.len() {
                    return None;
                }
                let slot = layer.slot(column);
                let title = format!("{} \u{203A} {} \u{00B7} {}", Self::layer_title(layer), Self::column_title(column, snap), Self::slot_suffix(slot));
                if settings.occupied_slot_mode == crate::settings::OccupiedSlotMode::ReplaceItem
                    && let Some(s) = slot
                    && s.is_occupied()
                {
                    return Some(MenuNode::submenu(
                        title,
                        vec![MenuNode::action(
                            format!("Replace \u{201C}{}\u{201D}", s.clip_name.clone().unwrap_or_else(|| "clip".to_string())),
                            MenuAction::load(r.destination.clone(), true),
                            r.label.clone(),
                        )],
                    ));
                }
                Some(MenuNode::action(title, MenuAction::load(r.destination.clone(), false), r.label.clone()))
            }
            Destination::NextFree { .. } => Some(MenuNode::action(
                format!("{} \u{203A} Next Free Column", Self::layer_title(layer)),
                MenuAction::load(r.destination.clone(), false),
                r.label.clone(),
            )),
            Destination::AppendColumns { .. } => Some(MenuNode::action(
                format!("{} \u{203A} Append as New Column", Self::layer_title(layer)),
                MenuAction::load(r.destination.clone(), false),
                r.label.clone(),
            )),
            Destination::NewLayer | Destination::NewNamedLayer { .. } | Destination::LayerPerFolder => None,
        }
    }
}
