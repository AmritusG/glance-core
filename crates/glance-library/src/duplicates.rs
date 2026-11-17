//! `DuplicateRegistry` (CrateKit/DuplicateRegistry.swift): the `clip_id`s that appear at two or more paths — the
//! cell badge and the "Has duplicates" filter. Refreshed after each settled indexer pass and from "Refresh
//! Duplicate Markers".

use crate::error::Result;
use crate::store::MetadataStore;
use std::collections::HashSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DuplicateRegistry {
    duplicate_clip_ids: HashSet<String>,
}

impl DuplicateRegistry {
    /// Empty until the first [`DuplicateRegistry::refresh`].
    pub fn new() -> DuplicateRegistry {
        DuplicateRegistry::default()
    }

    /// The snapshot.
    pub fn duplicate_clip_ids(&self) -> &HashSet<String> {
        &self.duplicate_clip_ids
    }

    /// `isDuplicate(clipID:)`: `false` for `None` (a path with no row).
    pub fn is_duplicate(&self, clip_id: Option<&str>) -> bool {
        clip_id.is_some_and(|id| self.duplicate_clip_ids.contains(id))
    }

    /// `refresh()`: replace the set from the database (one query). Returns whether it changed (Swift publishes on
    /// every refresh; the app may skip a redraw when nothing changed).
    pub fn refresh(&mut self, store: &MetadataStore) -> Result<bool> {
        let ids: HashSet<String> = store.clip_ids_with_duplicates()?.into_iter().collect();
        let changed = ids != self.duplicate_clip_ids;
        self.duplicate_clip_ids = ids;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    // DuplicateRegistryTests, same rows and expectations.
    use super::*;
    use crate::store::{ClipRow, Location};

    fn row(clip_id: &str, path: &str) -> ClipRow {
        ClipRow::new(clip_id, path, 1024, 1_700_000_000.0, 1_700_000_000.0, 1_700_000_000.0)
    }

    fn store() -> MetadataStore {
        MetadataStore::open_with(Location::InMemory, Box::new(Default::default)).expect("in-memory store")
    }

    #[test]
    fn empty_on_init() {
        let r = DuplicateRegistry::new();
        assert!(r.duplicate_clip_ids().is_empty());
        assert!(!r.is_duplicate(Some("anything:0")));
        assert!(!r.is_duplicate(None));
    }

    #[test]
    fn refresh_flags_cross_path_clip_ids() {
        let mut s = store();
        s.upsert(&row("shared:1024", "/tmp/local/clip.mov"), &[]).expect("upsert");
        s.upsert(&row("shared:1024", "/tmp/external/clip.mov"), &[]).expect("upsert");
        s.upsert(&row("unique:512", "/tmp/local/solo.mov"), &[]).expect("upsert");
        let mut r = DuplicateRegistry::new();
        r.refresh(&s).expect("refresh");
        assert_eq!(r.duplicate_clip_ids(), &["shared:1024".to_string()].into_iter().collect());
        assert!(r.is_duplicate(Some("shared:1024")));
        assert!(!r.is_duplicate(Some("unique:512")));
        assert!(!r.is_duplicate(Some("never-existed:0")));
    }

    #[test]
    fn refresh_drops_stale_entries() {
        let mut s = store();
        s.upsert(&row("x:1", "/tmp/a.mov"), &[]).expect("upsert");
        s.upsert(&row("x:1", "/tmp/b.mov"), &[]).expect("upsert");
        let mut r = DuplicateRegistry::new();
        r.refresh(&s).expect("refresh");
        assert!(r.is_duplicate(Some("x:1")));
        s.delete_row("/tmp/b.mov").expect("delete");
        r.refresh(&s).expect("refresh");
        assert!(r.duplicate_clip_ids().is_empty());
        assert!(!r.is_duplicate(Some("x:1")));
    }

    #[test]
    fn refresh_is_idempotent() {
        let mut s = store();
        s.upsert(&row("y:1", "/tmp/c.mov"), &[]).expect("upsert");
        s.upsert(&row("y:1", "/tmp/d.mov"), &[]).expect("upsert");
        let mut r = DuplicateRegistry::new();
        assert!(r.refresh(&s).expect("refresh"));
        let first = r.duplicate_clip_ids().clone();
        assert!(!r.refresh(&s).expect("refresh"));
        assert_eq!(&first, r.duplicate_clip_ids());
    }
}
