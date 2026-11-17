//! `ClipHashingService` (CrateKit/ClipHashingService.swift): hashes the indexer's cache misses four at a time, one
//! session at a time. A new session cancels the one in flight and waits for it to drain; a cancelled session stops
//! seeding new hashes, lets the ones running finish, and returns what completed — so a pass whose hashing was
//! cancelled by a later pass (an FSEvent arriving during the launch rebuild) leaves its un-hashed clips out, as
//! Swift does.

use crate::clip_id::clip_id;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

/// How many files are hashed at once.
pub const CONCURRENT_HASHES: usize = 4;

/// One hashed file (`ClipHashingService.Output`): the content id and the size and modification date read just
/// before hashing (`attributesOfItem`, which does not follow a symbolic link).
#[derive(Clone, Debug, PartialEq)]
pub struct HashOutput {
    pub clip_id: String,
    pub file_size: i64,
    /// Seconds since 1970 ([`crate::enumerate::foundation_mtime`]).
    pub file_mtime: f64,
}

/// Why a file was not hashed (`ClipHashingService.HashError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HashError {
    FileMissing,
    Unreadable(String),
}

impl std::fmt::Display for HashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HashError::FileMissing => f.write_str("fileMissing"),
            HashError::Unreadable(s) => write!(f, "unreadable({s})"),
        }
    }
}

pub type HashResult = Result<HashOutput, HashError>;

#[derive(Default)]
struct Session {
    cancelled: AtomicBool,
    done: Mutex<bool>,
    finished: Condvar,
}

impl Session {
    fn finish(&self) {
        *self.done.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.finished.notify_all();
    }

    fn wait(&self) {
        let mut done = self.done.lock().unwrap_or_else(PoisonError::into_inner);
        while !*done {
            done = self.finished.wait(done).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// The hashing service; share it between indexer passes (`Arc`).
#[derive(Default)]
pub struct HashingService {
    session: Mutex<Option<Arc<Session>>>,
    /// Serialises sessions: a new one starts only after the previous one drained.
    turn: Mutex<()>,
}

impl std::fmt::Debug for HashingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HashingService").finish_non_exhaustive()
    }
}

impl HashingService {
    pub fn new() -> HashingService {
        HashingService::default()
    }

    /// Cancel the session in flight, if any (it returns what it completed). Does not wait.
    pub fn cancel(&self) {
        if let Some(s) = self.session.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
            s.cancelled.store(true, Ordering::SeqCst);
        }
    }

    /// `indexFolder(urls:progress:)`: hash every path, at most [`CONCURRENT_HASHES`] at once; `progress` gets the
    /// running count of completed files after each one (from the calling thread). Cancels and waits out the
    /// previous session first. The map holds an entry per completed path; a cancelled session's map lacks the
    /// paths it never started.
    pub fn index_folder(&self, paths: &[String], mut progress: impl FnMut(usize)) -> HashMap<String, HashResult> {
        let session = Arc::new(Session::default());
        {
            let mut slot = self.session.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(previous) = slot.as_ref() {
                previous.cancelled.store(true, Ordering::SeqCst);
            }
            *slot = Some(session.clone());
        }
        let _turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        let results = run_session(&session, paths, &mut progress);
        session.finish();
        let mut slot = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.as_ref().is_some_and(|s| Arc::ptr_eq(s, &session)) {
            *slot = None;
        }
        results
    }

    /// Wait for the session in flight (if any) to finish.
    pub fn wait_idle(&self) {
        let current = self.session.lock().unwrap_or_else(PoisonError::into_inner).clone();
        if let Some(s) = current {
            s.wait();
        }
    }
}

fn run_session(session: &Session, paths: &[String], progress: &mut dyn FnMut(usize)) -> HashMap<String, HashResult> {
    let mut results = HashMap::with_capacity(paths.len());
    if paths.is_empty() {
        return results;
    }
    let next = Mutex::new(0usize);
    let (tx, rx) = std::sync::mpsc::channel::<(String, HashResult)>();
    std::thread::scope(|scope| {
        for _ in 0..CONCURRENT_HASHES.min(paths.len()) {
            let tx = tx.clone();
            let next = &next;
            scope.spawn(move || {
                loop {
                    // Stop seeding once cancelled; the hashes already running finish.
                    if session.cancelled.load(Ordering::SeqCst) {
                        break;
                    }
                    let i = {
                        let mut n = next.lock().unwrap_or_else(PoisonError::into_inner);
                        let i = *n;
                        *n += 1;
                        i
                    };
                    let Some(path) = paths.get(i) else { break };
                    if tx.send((path.clone(), compute(path))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut completed = 0;
        for (path, outcome) in rx {
            results.insert(path, outcome);
            completed += 1;
            progress(completed);
        }
    });
    results
}

/// `ClipHashingService.compute(for:)`: missing → `FileMissing`; else the attributes, then the hash.
pub fn compute(path: &str) -> HashResult {
    let p = std::path::Path::new(path);
    // `FileManager.fileExists` follows a symbolic link.
    if std::fs::metadata(p).is_err() {
        return Err(HashError::FileMissing);
    }
    let meta = std::fs::symlink_metadata(p).map_err(|e| HashError::Unreadable(e.to_string()))?;
    let file_size = i64::try_from(meta.len()).unwrap_or(0);
    let file_mtime = crate::enumerate::foundation_mtime(&meta).unwrap_or(0.0);
    let id = clip_id(p).map_err(|e| HashError::Unreadable(e.to_string()))?;
    Ok(HashOutput { clip_id: id, file_size, file_mtime })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("glance-hashsvc-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).ok();
        d
    }

    fn file(dir: &std::path::Path, name: &str, bytes: &[u8]) -> String {
        let p = dir.join(name);
        std::fs::write(&p, bytes).ok();
        p.to_string_lossy().into_owned()
    }

    // ClipHashingServiceTests.testHashesAllUrls
    #[test]
    fn hashes_all_paths() {
        let d = scratch("all");
        let paths: Vec<String> = (0..8u8)
            .map(|i| {
                let mut b = vec![i];
                b.extend(0..128u8);
                file(&d, &format!("clip{i}.mov"), &b)
            })
            .collect();
        let results = HashingService::new().index_folder(&paths, |_| {});
        assert_eq!(results.len(), paths.len());
        for p in &paths {
            let out = results.get(p).and_then(|r| r.as_ref().ok());
            assert!(out.is_some_and(|o| !o.clip_id.is_empty() && o.file_size > 0), "{p}");
        }
        std::fs::remove_dir_all(&d).ok();
    }

    // testResultsIndependentByURL
    #[test]
    fn results_are_per_path() {
        let d = scratch("indep");
        let a = file(&d, "a.mov", &[1, 2]);
        let b = file(&d, "b.mov", &[0xfe, 0xfd]);
        let r = HashingService::new().index_folder(&[a.clone(), b.clone()], |_| {});
        let id = |p: &String| r.get(p).and_then(|x| x.as_ref().ok()).map(|o| o.clip_id.clone());
        assert!(id(&a).is_some() && id(&b).is_some());
        assert_ne!(id(&a), id(&b));
        std::fs::remove_dir_all(&d).ok();
    }

    // testEmptyUrlListReturnsImmediately
    #[test]
    fn empty_list() {
        assert!(HashingService::new().index_folder(&[], |_| {}).is_empty());
    }

    // testMissingFileSurfacesAsFailure
    #[test]
    fn missing_file_is_file_missing() {
        let d = scratch("missing");
        let ghost = d.join("ghost.mov").to_string_lossy().into_owned();
        let r = HashingService::new().index_folder(std::slice::from_ref(&ghost), |_| {});
        assert_eq!(r.get(&ghost), Some(&Err(HashError::FileMissing)));
        std::fs::remove_dir_all(&d).ok();
    }

    // testProgressCallbackFiresOncePerCompletion
    #[test]
    fn progress_once_per_completion() {
        let d = scratch("progress");
        let paths: Vec<String> = (0..12u8).map(|i| file(&d, &format!("p{i}.mov"), &[i])).collect();
        let mut seen = Vec::new();
        HashingService::new().index_folder(&paths, |n| seen.push(n));
        assert_eq!(seen.len(), paths.len());
        assert_eq!(seen.last(), Some(&paths.len()));
        assert!(seen.windows(2).all(|w| w[1] >= w[0]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_cancelled_session_returns_what_it_completed() {
        let d = scratch("cancel");
        let paths: Vec<String> = (0..64u8).map(|i| file(&d, &format!("c{i}.mov"), &[i; 4096])).collect();
        let svc = HashingService::new();
        let r = svc.index_folder(&paths, |n| {
            if n == 1 {
                svc.cancel();
            }
        });
        // Seeding stops: the hashes already running finish, the rest never start.
        assert!(!r.is_empty() && r.len() < paths.len(), "{}", r.len());
        // The next session runs normally.
        assert_eq!(svc.index_folder(&paths, |_| {}).len(), paths.len());
        std::fs::remove_dir_all(&d).ok();
    }
}
