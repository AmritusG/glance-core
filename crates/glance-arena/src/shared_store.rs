// SPDX-License-Identifier: MIT
//! The kit's files in one folder: `settings.json`, `recents.json`, `requests/` and `cache.json`.
//! Every write is atomic (write a temporary file, then rename). A host keeps its own folder,
//! `<Application Support>/SendEhrKit/<host id>/` ([`SharedStore::for_host`]).

use crate::error::ArenaStatus;
use crate::planner::Destination;
use crate::settings::{RecentTarget, SendEhrSettings, SettingChange};
use crate::snapshot::CompositionSnapshot;
use crate::swift_json::{self, Timestamp};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fs;
use std::io;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// What a host publishes for a menu that cannot ask it directly (a Finder extension).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheState {
    pub status: ArenaStatus,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub snapshot: Option<CompositionSnapshot>,
    #[serde(rename = "hostPID")]
    pub host_pid: i32,
    pub written_at: Timestamp,
    /// When the host last received the composition ("Updated n s ago").
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub updated_at: Option<Timestamp>,
}

/// A menu pick sent to the host: a load (`paths` = selected files and folders), or a settings
/// change (`setting` set, `paths` empty).
#[derive(Debug, Clone, PartialEq)]
pub struct LoadRequest {
    pub id: Uuid,
    pub paths: Vec<String>,
    pub destination: Destination,
    pub replace_confirmed: bool,
    pub label: String,
    pub created_at: Timestamp,
    pub setting: Option<SettingChange>,
    /// New-layer picks: the layer indices the menu showed.
    pub expected_layers: Option<RangeInclusive<usize>>,
}

impl LoadRequest {
    pub fn new(paths: Vec<String>, destination: Destination, replace_confirmed: bool, label: impl Into<String>) -> Self {
        LoadRequest {
            id: Uuid::new_v4(),
            paths,
            destination,
            replace_confirmed,
            label: label.into(),
            created_at: Timestamp::now(),
            setting: None,
            expected_layers: None,
        }
    }

    pub fn setting_change(change: SettingChange) -> Self {
        LoadRequest { setting: Some(change), ..LoadRequest::new(Vec::new(), Destination::NewLayer, false, change.log_line()) }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadRequestRepr {
    #[serde(with = "crate::swift_json::uuid_upper")]
    id: Uuid,
    paths: Vec<String>,
    destination: Destination,
    replace_confirmed: bool,
    label: String,
    created_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    setting: Option<SettingChange>,
    /// Swift's `ClosedRange<Int>`: `[lower, upper]`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    expected_layers: Option<(usize, usize)>,
}

impl Serialize for LoadRequest {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        LoadRequestRepr {
            id: self.id,
            paths: self.paths.clone(),
            destination: self.destination.clone(),
            replace_confirmed: self.replace_confirmed,
            label: self.label.clone(),
            created_at: self.created_at,
            setting: self.setting,
            expected_layers: self.expected_layers.as_ref().map(|r| (*r.start(), *r.end())),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for LoadRequest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = LoadRequestRepr::deserialize(d)?;
        let expected_layers = match r.expected_layers {
            Some((lo, hi)) if lo > hi => {
                return Err(serde::de::Error::custom("Cannot initialize ClosedRange with lowerBound > upperBound"));
            }
            Some((lo, hi)) => Some(lo..=hi),
            None => None,
        };
        Ok(LoadRequest {
            id: r.id,
            paths: r.paths,
            destination: r.destination,
            replace_confirmed: r.replace_confirmed,
            label: r.label,
            created_at: r.created_at,
            setting: r.setting,
            expected_layers,
        })
    }
}

/// The kit's folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedStore {
    pub root: PathBuf,
}

impl SharedStore {
    /// Folder under Application Support holding one folder per host id.
    pub const KIT_FOLDER: &'static str = "SendEhrKit";
    /// A cache older than this means "the host is not running" (seconds).
    pub const STALE_AFTER: f64 = 25.0;

    /// A store at `root` (its `requests/` folder is created).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let s = SharedStore { root: root.into() };
        let _ = fs::create_dir_all(s.requests_dir());
        s
    }

    /// `<application_support>/SendEhrKit/<host_id>/`, the folder SendEhrKit's controller uses for a
    /// host app (its bundle id).
    pub fn for_host(application_support: &Path, host_id: &str) -> Self {
        Self::new(application_support.join(Self::KIT_FOLDER).join(host_id))
    }

    pub fn cache_path(&self) -> PathBuf {
        self.root.join("cache.json")
    }

    pub fn settings_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn recents_path(&self) -> PathBuf {
        self.root.join("recents.json")
    }

    pub fn requests_dir(&self) -> PathBuf {
        self.root.join("requests")
    }

    fn write<T: Serialize + ?Sized>(value: &T, path: &Path) -> io::Result<()> {
        let bytes = swift_json::to_vec(value).map_err(io::Error::other)?;
        write_atomic(path, &bytes)
    }

    fn read<T: DeserializeOwned>(path: &Path) -> Option<T> {
        let data = fs::read(path).ok()?;
        serde_json::from_slice(&data).ok()
    }

    pub fn write_cache(&self, c: &CacheState) -> io::Result<()> {
        Self::write(c, &self.cache_path())
    }

    /// Heartbeat without re-encoding: sets the cache file's modification date.
    pub fn touch_cache(&self, date: std::time::SystemTime) -> io::Result<()> {
        let f = fs::OpenOptions::new().write(true).open(self.cache_path())?;
        f.set_modified(date)
    }

    pub fn read_cache(&self) -> Option<CacheState> {
        Self::read(&self.cache_path())
    }

    pub fn write_settings(&self, s: &SendEhrSettings) -> io::Result<()> {
        Self::write(s, &self.settings_path())
    }

    /// The settings, or the defaults when the file is missing or unreadable.
    pub fn read_settings(&self) -> SendEhrSettings {
        Self::read(&self.settings_path()).unwrap_or_default()
    }

    pub fn write_recents(&self, r: &[RecentTarget]) -> io::Result<()> {
        Self::write(r, &self.recents_path())
    }

    pub fn read_recents(&self) -> Vec<RecentTarget> {
        Self::read(&self.recents_path()).unwrap_or_default()
    }

    pub fn submit(&self, request: &LoadRequest) -> io::Result<()> {
        let mut buf = [0u8; 36];
        let name = format!("{}.json", request.id.hyphenated().encode_upper(&mut buf));
        Self::write(request, &self.requests_dir().join(name))
    }

    /// Takes (reads and deletes) all pending requests, oldest first.
    pub fn take_requests(&self) -> Vec<LoadRequest> {
        let Ok(entries) = fs::read_dir(self.requests_dir()) else { return Vec::new() };
        let mut out: Vec<LoadRequest> = Vec::new();
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            if let Some(r) = Self::read::<LoadRequest>(&path) {
                out.push(r);
            }
            let _ = fs::remove_file(&path);
        }
        out.sort_by(|a, b| a.created_at.0.total_cmp(&b.created_at.0));
        out
    }

    /// True when the cache was written (or touched) recently by a live host.
    pub fn host_is_alive(&self, cache: Option<&CacheState>, now: Timestamp) -> bool {
        let Some(cache) = cache else { return false };
        let touched = fs::metadata(self.cache_path()).and_then(|m| m.modified()).map(Timestamp::from_system_time).ok();
        let latest = match touched {
            Some(t) if t.0 > cache.written_at.0 => t,
            _ => cache.written_at,
        };
        now.seconds_since(latest) < Self::STALE_AFTER
    }
}

/// Writes `bytes` to a temporary file next to `path`, then renames it over `path`.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", Uuid::new_v4().simple()));
    fs::write(&tmp, bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}
