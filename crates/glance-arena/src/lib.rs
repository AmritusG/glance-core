// SPDX-License-Identifier: MIT
//! # glance-arena — "Send to Arena" for the Glance family
//!
//! Derived from SendEhrKit 1.1.2 (the Swift "Send to Arena" kit for Resolume Arena) by its author
//! Amrit Rosell, who relicensed it under the MIT licence for the Glance family (2026-11-14). The
//! behaviour, the menu texts, the error texts and the settings files are SendEhrKit's.
//!
//! What it does: keeps a small snapshot of Arena's composition (Live: websocket pushes; On Demand
//! / Interval: REST fetches), builds the platform-neutral "Send to Arena" menu tree from it, and
//! loads files into layers and columns over Arena's REST API (fresh composition → plan → ask about
//! occupied slots → add layer / columns → open → verify → optional trigger).
//!
//! Layers, from the wire up:
//! - [`ArenaClient`]: plain-HTTP REST calls (`/api/v1`), errors mapped to [`ArenaError`] texts.
//! - [`decoder`]: the composition JSON (18-29 MB) scanned in place for the few fields needed.
//! - [`CompositionStore`]: the update modes, the reconnect policy, rename subscriptions and change
//!   de-duplication, on its own thread.
//! - [`LoadPlanner`] / [`ArenaLoader`]: the pure plan and its execution.
//! - [`menu`]: the menu tree ([`MenuNode`]) with every title.
//! - [`Controller`]: the one-call host API (start with a default mode, menu nodes for a selection,
//!   perform a pick, dialogs and change notifications through [`ControllerHooks`]).
//! - [`SharedStore`]: `settings.json`, `recents.json`, `requests/` and `cache.json`, written the
//!   way Swift's `JSONEncoder` writes them ([`swift_json`]).
#![forbid(unsafe_code)]

pub mod client;
pub mod controller;
pub mod decoder;
pub mod error;
pub mod event_log;
pub mod file_url;
mod http;
pub mod loader;
pub mod media;
pub mod menu;
pub mod planner;
pub mod reconnect;
pub mod remap;
pub mod settings;
pub mod shared_store;
pub mod snapshot;
mod socket;
pub mod store;
pub mod swift_json;

pub use client::{ArenaClient, ArenaProduct, RunningProbe};
pub use controller::{AskReplace, Controller, ControllerHooks, DateAddedProbe, LoadCompletion, Prompt, ShowError};
pub use error::{ArenaError, ArenaStatus};
pub use event_log::{EventLog, LogEntry};
pub use file_url::ArenaFileURL;
pub use loader::{ArenaLoader, LayersOutcome, LoadOutcome, READ_BACK_POLL, READ_BACK_SETTLE, SlotResult};
pub use media::{ArenaMediaTypes, FolderGroup, MediaCollection, MediaCollector, MediaItem, MediaSelection, MediaSorter, SortKey};
pub use menu::{MenuAction, MenuNode, SendMenuInput, SendMenuModel};
pub use planner::{Assignment, ConflictResolution, Destination, LayerPlan, LayersPlan, LoadPlan, LoadPlanner};
pub use reconnect::ReconnectPolicy;
pub use remap::{PathRemap, remap};
pub use settings::{ArenaEndpoint, ArenaUpdateMode, OccupiedSlotMode, RecentTarget, SendEhrSettings, SettingChange, TriggerScope};
pub use shared_store::{CacheState, LoadRequest, SharedStore};
pub use snapshot::{ArenaNames, ColumnInfo, CompositionSnapshot, LayerInfo, SlotInfo};
pub use store::{Activity, CompositionStore, OnChange};
pub use swift_json::Timestamp;

/// The `Sec-WebSocket-Accept` value for a key (RFC 6455 4.2.2), for test servers.
#[doc(hidden)]
pub use socket::accept_key as websocket_accept_key;
