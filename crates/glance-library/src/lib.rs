//! MaCrate's library, byte-compatible with Swift MaCrate 0.16.4 and iCrate: the `library.sqlite` schema and its
//! twelve GRDB migrations, every write and query path, the attached `measurements.sqlite`, the per-clip sidecar
//! `.<file>.crate-meta.json`, the per-root `.crate-stashes.json`, the content id `clip_id`, the container header
//! reader (duration, frame rate, display size), the folder enumeration and the library-root rules.
//!
//! On top of them, the services MaCrate runs (wave M3): the [`indexer`] (launch rebuild, Refresh, FSEvents and external
//! sidecars, with [`hashing`]), the FSEvents batch filter ([`watch_filter`]), the [`duplicates`] registry, the root list and
//! folder sets ([`roots_store`]), the sidebar's [`folder_tree`] and the poster [`thumbnails`].

pub mod clip_id;
pub mod enumerate;
pub mod header;
pub mod roots;
pub mod thumbkey;

pub mod duplicates;
pub mod folder_tree;
pub mod hashing;
pub mod indexer;
pub mod roots_store;
pub mod thumbnails;
pub mod watch_filter;

pub mod alpha_probe;
pub mod brief_import;
pub mod dump;
pub mod error;
pub mod filter;
pub mod json;
pub mod measurements;
pub mod query;
pub mod schema;
pub mod search;
pub mod sidecar;
pub mod sorting;
pub mod stash;
pub mod store;
pub mod unicode;
#[rustfmt::skip]
mod unicode_tables;

pub use error::{LibraryError, PresetError, Result};
pub use filter::{ClipFilter, ClipSort, SavedPreset};
pub use sidecar::ClipMetadata;
pub use stash::{Stash, StashEntry};
pub use store::{ClipRow, Location, MetadataStore};
