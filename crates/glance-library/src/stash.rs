//! Stashes: named clip sets made on iCrate, kept in `.crate-stashes.json` at the library (drive) root — the same
//! bytes Swift MaCrate's `StashFile` and iCrate's `StashStore` write — and the shared reorder rule
//! (`StashOrdering.reorder`).

use crate::error::Result;
use crate::json::{self, Json};
use std::collections::HashSet;
use std::hash::Hash;
use std::path::{Path, PathBuf};

/// `StashFile.fileName`.
pub const STASH_FILE_NAME: &str = ".crate-stashes.json";

/// One clip in a stash: its content id and its path relative to the root (`StashEntry`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StashEntry {
    pub clip_id: String,
    pub relative_path_hint: String,
}

/// A stash (`Stash`): `id` is the iPad's upper-case UUID string, times are seconds since 1970, `clips` in stash
/// order.
#[derive(Clone, Debug, PartialEq)]
pub struct Stash {
    pub id: String,
    pub name: String,
    pub created_at: f64,
    pub updated_at: f64,
    pub clips: Vec<StashEntry>,
}

/// `StashFile.url(atRoot:)`.
pub fn stash_file_path(root: &Path) -> PathBuf {
    root.join(STASH_FILE_NAME)
}

/// The file's bytes: `{"stashes": [...]}` with the camelCase keys, pretty-printed with sorted keys.
pub fn encode_stash_file(stashes: &[Stash]) -> Result<Vec<u8>> {
    let items = stashes
        .iter()
        .map(|s| {
            Json::object([
                ("id", Json::string(s.id.clone())),
                ("name", Json::string(s.name.clone())),
                ("createdAt", Json::Double(s.created_at)),
                ("updatedAt", Json::Double(s.updated_at)),
                (
                    "clips",
                    Json::Array(
                        s.clips
                            .iter()
                            .map(|e| {
                                Json::object([("clipID", Json::string(e.clip_id.clone())), ("relativePathHint", Json::string(e.relative_path_hint.clone()))])
                            })
                            .collect(),
                    ),
                ),
            ])
        })
        .collect();
    Ok(json::to_pretty_sorted(&Json::object([("stashes", Json::Array(items))]))?.into_bytes())
}

/// Decode the file's bytes. Every key is required (synthesised `Codable`), unknown keys are ignored; anything
/// else is `None` — which `read_stash_file` turns into "no stashes", as both Swift apps do.
pub fn decode_stash_file(bytes: &[u8]) -> Option<Vec<Stash>> {
    let doc = json::parse(bytes)?;
    doc.get("stashes")?
        .as_array()?
        .iter()
        .map(|s| {
            Some(Stash {
                id: s.get("id")?.as_str()?.to_string(),
                name: s.get("name")?.as_str()?.to_string(),
                created_at: s.get("createdAt")?.as_f64()?,
                updated_at: s.get("updatedAt")?.as_f64()?,
                clips: s
                    .get("clips")?
                    .as_array()?
                    .iter()
                    .map(|e| {
                        Some(StashEntry {
                            clip_id: e.get("clipID")?.as_str()?.to_string(),
                            relative_path_hint: e.get("relativePathHint")?.as_str()?.to_string(),
                        })
                    })
                    .collect::<Option<Vec<_>>>()?,
            })
        })
        .collect()
}

/// `StashFile.read(atRoot:)`: the stashes at a root; none when the file is absent, unreadable or undecodable.
pub fn read_stash_file(root: &Path) -> Vec<Stash> {
    std::fs::read(stash_file_path(root)).ok().and_then(|b| decode_stash_file(&b)).unwrap_or_default()
}

/// `StashFile.write(_:toRoot:)`: replace the file atomically (a temporary file beside it, then a rename).
pub fn write_stash_file(stashes: &[Stash], root: &Path) -> Result<()> {
    let bytes = encode_stash_file(stashes)?;
    let target = stash_file_path(root);
    let temp = root.join(format!(".crate-stashes.{}.tmp", uuid::Uuid::new_v4().to_string().to_uppercase()));
    std::fs::write(&temp, bytes)?;
    if let Err(e) = std::fs::rename(&temp, &target) {
        let _ = std::fs::remove_file(&temp);
        return Err(e.into());
    }
    Ok(())
}

/// `StashOrdering.reorder`: the entries named by `visible_order` first, in that order (an id with no entry is
/// ignored; a repeated id takes the first matching entry), then every entry whose id is not named, in its original
/// order. Loss-free for the entries `visible_order` does not name.
pub fn reorder<T: Clone, Id: Eq + Hash>(entries: &[T], visible_order: &[Id], id: impl Fn(&T) -> Id) -> Vec<T> {
    let visible: HashSet<&Id> = visible_order.iter().collect();
    let mut out: Vec<T> = visible_order.iter().filter_map(|wanted| entries.iter().find(|e| id(e) == *wanted).cloned()).collect();
    out.extend(entries.iter().filter(|e| !visible.contains(&id(e))).cloned());
    out
}
