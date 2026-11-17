// SPDX-License-Identifier: MIT
//! One-call "Send to Arena" for a host app: keeps a live composition, builds the menu, performs
//! loads (asking before overwriting occupied slots), keeps recents and a 500-line log.
//!
//! The host supplies what the kit cannot know or show through [`ControllerHooks`]: whether Arena
//! runs, the three dialogs, and two change notifications. Typical egui use:
//!
//! - at launch: [`Controller::start`] with the host's default mode (MaCrate: On Demand);
//! - Arena's process launching / quitting: [`Controller::arena_launched`] / `arena_terminated`;
//! - a context menu opening: [`Controller::nodes`] once for the selection (it wakes the store),
//!   kept until the `redraw` hook fires; [`Controller::menu_opening`] for any menu opening;
//! - an item picked: [`Controller::perform`];
//! - `redraw`: the menu layout or status changed (rebuild an open menu, repaint);
//!   `state_changed`: anything changed, play state included.
//!
//! Hooks are called from the kit's threads (the store thread, a load's worker thread); the dialog
//! hooks block that worker until the user answers.

use crate::client::{ArenaClient, RunningProbe};
use crate::error::{ArenaError, ArenaStatus};
use crate::event_log::EventLog;
use crate::loader::{ArenaLoader, LoadOutcome};
use crate::media::{MediaCollection, MediaCollector, MediaSelection, last_path_component};
use crate::menu::{MenuAction, MenuNode, SendMenuInput, SendMenuModel};
use crate::planner::{Assignment, ConflictResolution, Destination};
use crate::settings::{ArenaUpdateMode, RecentTarget, SendEhrSettings, SettingChange};
use crate::shared_store::SharedStore;
use crate::snapshot::CompositionSnapshot;
use crate::store::CompositionStore;
use crate::swift_json::Timestamp;
use std::ops::RangeInclusive;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

/// A dialog's texts: title, body, buttons (the first is the default; `cancel` is the one Escape
/// picks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub message: String,
    pub informative: String,
    pub buttons: Vec<String>,
    pub cancel: Option<usize>,
}

impl Prompt {
    /// "Replace the clip in column 3?" / "Replace 2 occupied slots?" with Replace / Skip
    /// Occupied / Cancel; the body lists up to 8 slots.
    pub fn replace(conflicts: &[Assignment]) -> Prompt {
        let message = match conflicts {
            [one] => format!("Replace the clip in column {}?", one.column),
            _ => format!("Replace {} occupied slots?", conflicts.len()),
        };
        let mut informative = conflicts
            .iter()
            .take(8)
            .map(|a| format!("Column {}: \u{201C}{}\u{201D} \u{2192} {}", a.column, a.replacing.clone().unwrap_or_default(), last_path_component(&a.path)))
            .collect::<Vec<_>>()
            .join("\n");
        if conflicts.len() > 8 {
            informative += "\n...";
        }
        Prompt { message, informative, buttons: vec!["Replace".to_string(), "Skip Occupied".to_string(), "Cancel".to_string()], cancel: Some(2) }
    }

    /// The answer for a button index of [`Prompt::replace`] (anything else = Cancel).
    pub fn replace_answer(button: usize) -> ConflictResolution {
        match button {
            0 => ConflictResolution::Replace,
            1 => ConflictResolution::Skip,
            _ => ConflictResolution::Cancel,
        }
    }

    /// "Load 120 files?" with Load 120 Files / Cancel.
    pub fn large_load(c: &MediaCollection, count: usize, per_folder: bool) -> Prompt {
        let names = c.folders.iter().map(|f| format!("\u{201C}{}\u{201D}", last_path_component(f))).collect::<Vec<_>>().join(", ");
        Prompt {
            message: format!("Load {count} files?"),
            informative: format!(
                "{names}: {}. {}",
                c.summary(),
                if per_folder { "One new layer per folder, each from column 1." } else { "They fill consecutive columns." }
            ),
            buttons: vec![format!("Load {count} Files"), "Cancel".to_string()],
            cancel: Some(1),
        }
    }

    /// "Send to Arena failed" with the error's line.
    pub fn error(e: &ArenaError) -> Prompt {
        Prompt { message: "Send to Arena failed".to_string(), informative: e.plain_line(), buttons: vec!["OK".to_string()], cancel: None }
    }
}

type Hook<T> = Arc<dyn Fn() -> T + Send + Sync>;
/// Answers the occupied-slots question.
pub type AskReplace = Arc<dyn Fn(&Prompt, &[Assignment]) -> ConflictResolution + Send + Sync>;
/// Shows a failed load.
pub type ShowError = Arc<dyn Fn(&Prompt, &ArenaError) + Send + Sync>;
/// Reads a file's "date added".
pub type DateAddedProbe = Arc<dyn Fn(&Path) -> Option<SystemTime> + Send + Sync>;

/// What the host provides.
#[derive(Clone)]
pub struct ControllerHooks {
    /// Is Arena's process running on this computer? (`None` = unknown.)
    pub is_arena_running: RunningProbe,
    /// Occupied slots would be overwritten: Replace / Skip Occupied / Cancel. Blocks the load.
    pub ask_replace: AskReplace,
    /// Folders expanded to more files than the threshold: load? Blocks the load.
    pub ask_large_load: Arc<dyn Fn(&Prompt) -> bool + Send + Sync>,
    /// A load failed (not called when cancelled).
    pub show_error: ShowError,
    /// The menu layout, the status, the settings, recents, busy or the last outcome changed.
    pub redraw: Hook<()>,
    /// Any change of snapshot (play state included), status, settings or recents.
    pub state_changed: Hook<()>,
    /// A file's "date added", for Sort By Date Added (`None` = unknown).
    pub date_added: Option<DateAddedProbe>,
}

impl Default for ControllerHooks {
    /// Unknown process state, every question answered Cancel, no notifications.
    fn default() -> Self {
        ControllerHooks {
            is_arena_running: Arc::new(|| None),
            ask_replace: Arc::new(|_, _| ConflictResolution::Cancel),
            ask_large_load: Arc::new(|_| false),
            show_error: Arc::new(|_, _| {}),
            redraw: Arc::new(|| {}),
            state_changed: Arc::new(|| {}),
            date_added: None,
        }
    }
}

struct State {
    snapshot: Option<CompositionSnapshot>,
    status: ArenaStatus,
    updated_at: Option<Timestamp>,
    settings: SendEhrSettings,
    recents: Vec<RecentTarget>,
    busy: bool,
    last_outcome: Option<String>,
    redraws: usize,
    menu_invalidations: usize,
    menu_built: bool,
    started: bool,
}

struct Inner {
    shared: SharedStore,
    log: EventLog,
    hooks: ControllerHooks,
    store: CompositionStore,
    state: Mutex<State>,
}

/// The host-facing controller. Cheap to clone (shared).
#[derive(Clone)]
pub struct Controller {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Controller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Controller").field("root", &self.inner.shared.root).finish_non_exhaustive()
    }
}

/// The result of a load, top-most layer for Folders as New Layers.
pub type LoadCompletion = Box<dyn FnOnce(Result<LoadOutcome, ArenaError>) + Send>;

impl Controller {
    /// A controller on the files in `shared` (settings and recents are read now).
    pub fn new(shared: SharedStore, hooks: ControllerHooks) -> Self {
        let settings = shared.read_settings();
        let recents = shared.read_recents();
        let inner = Arc::new_cyclic(|weak: &Weak<Inner>| {
            let log = EventLog::new();
            let client = ArenaClient::with_probe(settings.endpoint.clone(), Arc::clone(&hooks.is_arena_running));
            let store = CompositionStore::new(client, settings.update_mode, Duration::from_secs(u64::try_from(settings.update_interval).unwrap_or(1)));
            store.set_log(Some(log.clone()));
            let w = weak.clone();
            store.set_on_change(Some(Arc::new(move |snap, st, up| {
                if let Some(inner) = w.upgrade() {
                    Controller { inner }.receive(snap, st, up);
                }
            })));
            Inner {
                shared,
                log,
                hooks,
                store,
                state: Mutex::new(State {
                    snapshot: None,
                    status: ArenaStatus::Unknown,
                    updated_at: None,
                    settings,
                    recents,
                    busy: false,
                    last_outcome: None,
                    redraws: 0,
                    menu_invalidations: 0,
                    menu_built: false,
                    started: false,
                }),
            }
        });
        Controller { inner }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A store delivery. `updated_at` alone never redraws (an open menu rebuilding would fetch
    /// again in On Demand and loop).
    pub fn receive(&self, snap: Option<CompositionSnapshot>, st: ArenaStatus, up: Option<Timestamp>) {
        let redraw = {
            let mut s = self.state();
            let redraw = st != s.status || !CompositionSnapshot::same_menu(s.snapshot.as_ref(), snap.as_ref());
            if redraw {
                s.redraws += 1;
            }
            s.snapshot = snap;
            s.status = st;
            s.updated_at = up;
            redraw
        };
        if redraw {
            (self.inner.hooks.redraw)();
        }
        (self.inner.hooks.state_changed)();
    }

    fn make_client(&self, endpoint: &crate::settings::ArenaEndpoint) -> ArenaClient {
        ArenaClient::with_probe(endpoint.clone(), Arc::clone(&self.inner.hooks.is_arena_running))
    }

    /// A REST client for the current endpoint.
    pub fn client(&self) -> ArenaClient {
        let ep = self.state().settings.endpoint.clone();
        self.make_client(&ep)
    }

    /// Starts the updates (Live: websocket + poll; On Demand / Interval: REST fetches). Safe to
    /// call repeatedly. `default_mode`: this host's mode until the user picks one in a menu
    /// (`update_mode_chosen`); a pick always wins and is remembered.
    pub fn start(&self, default_mode: ArenaUpdateMode) {
        let (settings, switched) = {
            let mut s = self.state();
            if s.started {
                return;
            }
            s.started = true;
            let switched = !s.settings.update_mode_chosen && s.settings.update_mode != default_mode;
            if switched {
                self.inner.log.add(format!(
                    "Arena Updates: host default {} (no user pick; file said {})",
                    default_mode.title(),
                    s.settings.update_description()
                ));
                s.settings.update_mode = default_mode;
            }
            (s.settings.clone(), switched)
        };
        if switched {
            let _ = self.inner.shared.write_settings(&settings);
            self.inner.store.set_mode(default_mode, interval_of(&settings));
        }
        self.inner.log.add(format!("SendEhrKit start: {}:{}, Arena Updates {}", settings.endpoint.host, settings.endpoint.port, settings.update_description()));
        self.inner.store.start();
    }

    /// Arena's process launched on this computer.
    pub fn arena_launched(&self) {
        self.inner.store.arena_launched();
    }

    /// Arena's process quit on this computer.
    pub fn arena_terminated(&self) {
        self.inner.store.arena_terminated();
    }

    /// Any menu of the app opens. Outside Live (or Live disconnected), once a Send to Arena menu
    /// was built, the `redraw` hook fires so a cached menu is rebuilt (which wakes the store).
    pub fn menu_opening(&self) {
        let fire = {
            let mut s = self.state();
            let fire = s.menu_built && (s.settings.update_mode != ArenaUpdateMode::Live || !s.status.is_connected());
            if fire {
                s.menu_invalidations += 1;
            }
            fire
        };
        if fire {
            (self.inner.hooks.redraw)();
        }
    }

    /// A menu is about to show. Live: if disconnected, try to connect now. On Demand / Interval:
    /// fetch (at most once per 2 s).
    pub fn wake(&self) {
        let go = {
            let s = self.state();
            s.settings.update_mode != ArenaUpdateMode::Live || !s.status.is_connected()
        };
        if go {
            self.inner.store.wake();
        }
    }

    /// Fetch the composition now (the Refresh item).
    pub fn refresh(&self) {
        self.inner.log.add("Refresh");
        self.inner.store.refresh("Refresh");
    }

    pub fn update_settings(&self, new: SendEhrSettings) {
        let (endpoint_changed, updates_changed) = {
            let mut s = self.state();
            let e = new.endpoint != s.settings.endpoint;
            let u = new.update_mode != s.settings.update_mode || new.update_interval != s.settings.update_interval;
            s.settings = new.clone();
            (e, u)
        };
        let _ = self.inner.shared.write_settings(&new);
        if endpoint_changed {
            self.inner.log.add(format!("Endpoint -> {}:{}", new.endpoint.host, new.endpoint.port));
            self.inner.store.set_client(self.make_client(&new.endpoint));
        }
        if updates_changed {
            self.inner.store.set_mode(new.update_mode, interval_of(&new));
        }
        (self.inner.hooks.redraw)();
        (self.inner.hooks.state_changed)();
    }

    /// What the menu is built from for a selection of paths (files and folders).
    pub fn menu_input<S: AsRef<str>>(&self, paths: &[S]) -> SendMenuInput {
        let sel = MediaSelection::new(paths);
        let s = self.state();
        SendMenuInput {
            snapshot: s.snapshot.clone(),
            status: s.status.clone(),
            host_alive: true,
            recents: s.recents.clone(),
            settings: s.settings.clone(),
            file_count: sel.files.len(),
            unsupported_count: sel.unsupported,
            folder_names: sel.folders.iter().map(|f| last_path_component(f)).collect(),
            updated_at: s.updated_at,
            now: Timestamp::now(),
        }
    }

    /// The menu tree for `paths` (the children of "Send to Arena"). Building it wakes the store
    /// (and starts it in Live if the host never called `start`).
    pub fn nodes<S: AsRef<str>>(&self, paths: &[S]) -> Vec<MenuNode> {
        let started = self.state().started;
        if started {
            self.wake();
        } else {
            self.start(ArenaUpdateMode::Live);
        }
        self.state().menu_built = true;
        SendMenuModel::build(&self.menu_input(paths))
    }

    /// Runs a picked item. A load runs on a worker thread (returned); settings and Refresh at once.
    pub fn perform<S: AsRef<str>>(&self, action: &MenuAction, label: &str, paths: &[S]) -> Option<JoinHandle<()>> {
        match action {
            MenuAction::Load { destination, replace } => {
                let sel = MediaSelection::new(paths);
                let snap = self.state().snapshot.clone();
                let expected = SendMenuModel::expected_layers(destination, snap.as_ref(), sel.folders.len());
                let mut all = sel.folders.clone();
                all.extend(sel.files.iter().cloned());
                self.load(all, destination.clone(), *replace, label, expected, None)
            }
            MenuAction::Setting(change) => {
                self.apply(*change);
                None
            }
            MenuAction::Refresh => {
                self.refresh();
                None
            }
            MenuAction::OpenHost => None,
        }
    }

    /// One menu-picked setting.
    pub fn apply(&self, change: SettingChange) {
        self.inner.log.add(format!("Setting: {}", change.log_line()));
        let next = change.applied(&self.state().settings);
        self.update_settings(next);
    }

    /// Runs one load on a worker thread (`None`: no thread could be started); the result goes to the log, `last_outcome` and
    /// `completion`. `paths` may hold files and folders: folders expand (Include Subfolders),
    /// everything is ordered by the sort setting, and a large expansion is confirmed once.
    pub fn load(
        &self,
        paths: Vec<String>,
        destination: Destination,
        replace_confirmed: bool,
        label: &str,
        expected_layers: Option<RangeInclusive<usize>>,
        completion: Option<LoadCompletion>,
    ) -> Option<JoinHandle<()>> {
        let s = self.state().settings.clone();
        let loader = ArenaLoader::new(self.make_client(&s.endpoint), s.remaps.clone(), Some(self.inner.log.clone()));
        self.inner.log.add(format!(
            "Request: {} item(s) -> {label}{}, {}{}",
            paths.len(),
            if replace_confirmed { " (Replace)" } else { "" },
            s.log_summary(),
            expected_layers.as_ref().map(|r| format!(", menu showed {}", SendMenuModel::layers_text(r))).unwrap_or_default()
        ));
        self.set_busy(true);
        let this = self.clone();
        let label = label.to_string();
        let worker = move || {
            let result = this.run_load(&paths, &destination, replace_confirmed, &label, expected_layers, &s, &loader);
            match &result {
                Ok(_) => {}
                Err(e) => {
                    this.set_last_outcome(e.plain_line());
                    if *e == ArenaError::Cancelled {
                        this.inner.log.add("Cancelled - nothing loaded");
                    } else {
                        this.inner.log.add(format!("Load failed: {}", e.plain_line()));
                        (this.inner.hooks.show_error)(&Prompt::error(e), e);
                    }
                }
            }
            this.set_busy(false);
            if let Some(done) = completion {
                done(result);
            }
        };
        let handle = std::thread::Builder::new().name("arena-load".to_string()).spawn(worker).ok();
        if handle.is_none() {
            self.set_busy(false);
        }
        handle
    }

    #[allow(clippy::too_many_arguments)]
    fn run_load(
        &self,
        paths: &[String],
        destination: &Destination,
        replace_confirmed: bool,
        label: &str,
        expected_layers: Option<RangeInclusive<usize>>,
        s: &SendEhrSettings,
        loader: &ArenaLoader,
    ) -> Result<LoadOutcome, ArenaError> {
        let added = self.inner.hooks.date_added.clone();
        let collection = MediaCollector::collect_with(
            paths,
            s.include_subfolders,
            s.sort_key,
            s.sort_ascending,
            added.as_deref().map(|f| f as &(dyn Fn(&Path) -> Option<SystemTime> + Send + Sync)),
        );
        let log = &self.inner.log;
        if !collection.folders.is_empty() || collection.skipped_non_media > 0 {
            log.add(format!("Expanded: {}", collection.summary()));
        }
        let per_folder = *destination == Destination::LayerPerFolder;
        let count = if per_folder { collection.from_folders } else { collection.files.len() };
        if collection.needs_confirmation(s.large_folder_threshold, per_folder) {
            let yes = (self.inner.hooks.ask_large_load)(&Prompt::large_load(&collection, count, per_folder));
            log.add(format!("Large folder ({count} files) -> {}", if yes { "load" } else { "cancel" }));
            if !yes {
                return Err(ArenaError::Cancelled);
            }
        }
        if per_folder {
            if collection.loose_files() > 0 {
                log.add(format!("Folders as New Layers: ignored {} loose file(s) (not in a selected folder)", collection.loose_files()));
            }
            let outcome = loader.load_layers(&collection.groups, s.trigger_after_load, s.trigger_scope, expected_layers)?;
            let summary = outcome.summary();
            self.set_last_outcome(summary.clone());
            log.add(summary);
            self.inner.store.refresh("after load");
            // The top-most layer; a layers plan never has zero layers.
            return outcome.layers.into_iter().next().ok_or_else(|| ArenaError::BadResponse("no layer loaded".to_string()));
        }
        let ask = Arc::clone(&self.inner.hooks.ask_replace);
        let outcome =
            loader.load(&collection.files, destination, s.trigger_after_load, replace_confirmed, expected_layers, &|conflicts: &[Assignment]| {
                ask(&Prompt::replace(conflicts), conflicts)
            })?;
        let summary = outcome.summary();
        self.set_last_outcome(summary.clone());
        log.add(summary);
        self.remember(destination.clone(), label);
        self.inner.store.refresh("after load");
        Ok(outcome)
    }

    fn remember(&self, destination: Destination, label: &str) {
        let recents = {
            let mut s = self.state();
            s.recents = RecentTarget::pushing(RecentTarget::new(destination, label), &s.recents);
            s.recents.clone()
        };
        let _ = self.inner.shared.write_recents(&recents);
        (self.inner.hooks.redraw)();
        (self.inner.hooks.state_changed)();
    }

    fn set_busy(&self, busy: bool) {
        self.state().busy = busy;
        (self.inner.hooks.redraw)();
    }

    fn set_last_outcome(&self, text: String) {
        self.state().last_outcome = Some(text);
        (self.inner.hooks.redraw)();
    }

    // MARK: state

    pub fn settings(&self) -> SendEhrSettings {
        self.state().settings.clone()
    }

    pub fn recents(&self) -> Vec<RecentTarget> {
        self.state().recents.clone()
    }

    /// Latest composition (play state included).
    pub fn snapshot(&self) -> Option<CompositionSnapshot> {
        self.state().snapshot.clone()
    }

    pub fn status(&self) -> ArenaStatus {
        self.state().status.clone()
    }

    /// When the composition last arrived.
    pub fn updated_at(&self) -> Option<Timestamp> {
        self.state().updated_at
    }

    /// A load is running.
    pub fn busy(&self) -> bool {
        self.state().busy
    }

    /// The last load's summary or error line.
    pub fn last_outcome(&self) -> Option<String> {
        self.state().last_outcome.clone()
    }

    /// `redraw` notifications caused by the store (tests, diagnostics).
    pub fn redraws(&self) -> usize {
        self.state().redraws
    }

    /// Menu contents dropped because a menu opened (tests, diagnostics).
    pub fn menu_invalidations(&self) -> usize {
        self.state().menu_invalidations
    }

    pub fn log(&self) -> &EventLog {
        &self.inner.log
    }

    pub fn store(&self) -> &CompositionStore {
        &self.inner.store
    }

    pub fn shared(&self) -> &SharedStore {
        &self.inner.shared
    }
}

fn interval_of(s: &SendEhrSettings) -> Duration {
    Duration::from_secs(u64::try_from(s.update_interval).unwrap_or(1))
}
