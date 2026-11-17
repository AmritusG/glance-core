//! MaCrate's OS calls on macOS (v0.6.0): bookmarks, names, FSEvents, the Keychain.
#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("glance-platform-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[test]
fn a_plain_bookmark_resolves_to_its_folder_and_follows_a_rename() {
    let dir = scratch("bookmark");
    let folder = dir.join("Clips A");
    std::fs::create_dir_all(&folder).ok();
    let blob = glance_platform::bookmark_create(&folder).unwrap();
    assert!(blob.len() > 100, "a bookmark blob is several hundred bytes");
    let resolved = glance_platform::bookmark_resolve(&blob).unwrap();
    assert_eq!(resolved.path.canonicalize().unwrap(), folder.canonicalize().unwrap());
    let renamed = dir.join("Clips B");
    std::fs::rename(&folder, &renamed).unwrap();
    let after = glance_platform::bookmark_resolve(&blob).unwrap();
    assert_eq!(after.path.canonicalize().unwrap(), renamed.canonicalize().unwrap(), "bookmarks follow the folder (file id)");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn garbage_is_not_a_bookmark() {
    assert!(glance_platform::bookmark_resolve(b"not a bookmark").is_err());
}

#[test]
fn case_insensitive_compare_is_foundations() {
    use std::cmp::Ordering::*;
    assert_eq!(glance_platform::localized_case_insensitive_compare("abc", "ABC"), Equal);
    assert_eq!(glance_platform::localized_case_insensitive_compare("a", "B"), Less);
    assert_eq!(glance_platform::localized_case_insensitive_compare("clip10", "clip9"), Less, "not numeric");
    assert_eq!(glance_platform::precomposed("e\u{301}"), "\u{e9}");
}

#[test]
fn an_fsevents_stream_reports_a_new_file_in_its_folder() {
    let dir = scratch("fsevents").canonicalize().unwrap();
    let (tx, rx) = mpsc::channel();
    let watch = glance_platform::FolderWatch::start(std::slice::from_ref(&dir), 0.5, move |batch| {
        let _ = tx.send(batch);
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    std::fs::write(dir.join("new.mov"), b"x").unwrap();
    let mut seen = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if let Ok(batch) = rx.recv_timeout(Duration::from_millis(200))
            && batch.iter().any(|e| e.path.ends_with("new.mov"))
        {
            seen = true;
            break;
        }
    }
    drop(watch);
    assert!(seen, "the created file was reported within 5 s");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_watch_is_fine_and_drops_cleanly() {
    let w = glance_platform::FolderWatch::start(&[], 0.5, |_| {}).unwrap();
    drop(w);
}

/// Writes to the login keychain, so only when asked: `GLANCE_KEYCHAIN_TEST=1`.
#[test]
fn the_keychain_stores_reads_and_deletes_a_generic_password() {
    if std::env::var("GLANCE_KEYCHAIN_TEST").as_deref() != Ok("1") {
        eprintln!("skipped: set GLANCE_KEYCHAIN_TEST=1");
        return;
    }
    let (service, account) = ("com.amritus.glancecore.test", "probe");
    glance_platform::keychain_delete(service, account).unwrap();
    assert_eq!(glance_platform::keychain_get(service, account).unwrap(), None);
    glance_platform::keychain_set(service, account, "value-1").unwrap();
    glance_platform::keychain_set(service, account, "value-2").unwrap();
    assert_eq!(glance_platform::keychain_get(service, account).unwrap().as_deref(), Some("value-2"));
    glance_platform::keychain_delete(service, account).unwrap();
    assert_eq!(glance_platform::keychain_get(service, account).unwrap(), None);
}
