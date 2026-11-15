//! # glance-platform — the family's one island of OS calls
//!
//! Everything the apps need from the operating system that no portable crate gives them, in one
//! crate, so the rest of the workspace keeps `unsafe` forbidden (CLAUDE.md rule 3). Every unsafe
//! block carries a `// SAFETY:` comment (`clippy::undocumented_unsafe_blocks` is denied here).
//!
//! **macOS** (what Swift Glance does through AppKit / Foundation / MediaPlayer):
//! - [`Defaults`]: a preferences domain (`NSUserDefaults`) by bundle id — the same store, keys
//!   and value encodings the Swift apps use, so settings survive the swap in both directions;
//! - [`install_open_handler`]: files sent by the OS (Finder double-click, Open With, a drop on the
//!   Dock icon, `open`) at cold launch and while running, as [`PlatformEvent::Open`];
//! - [`is_default_for`] / [`set_default_for`]: the "Set as Default" flow (`NSWorkspace`);
//! - [`MediaKeys`]: the transport media keys and the Now Playing surface (`MPRemoteCommandCenter`,
//!   `MPNowPlayingInfoCenter`), as [`PlatformEvent::Media`];
//! - [`note_recent`]: the OS recent-documents list (`NSDocumentController`).
//!
//! **Windows** (written and compile-checked; run at the Windows milestone, C4):
//! [`SingleInstance`] (a named mutex plus a named pipe that hands a second launch's paths to the
//! first), [`settings_dir`] (the one documented per-user place), and [`MediaKeys`] on the System
//! Media Transport Controls.
//!
//! Events reach the app through one queue ([`take_events`]); [`set_waker`] lets the event loop be
//! woken when one arrives (an `EventLoopProxy` in the apps).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;

mod value;
pub use value::Value;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{Defaults, MediaKeys, install_open_handler, is_default_for, note_recent, set_default_for, set_subtitle};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{MediaKeys, SingleInstance, attach_menu};

#[cfg(not(any(target_os = "macos", windows)))]
mod other;
#[cfg(not(any(target_os = "macos", windows)))]
pub use other::MediaKeys;

/// A transport command from the OS (media keys, the Now Playing widget, Windows' media flyout).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MediaCommand {
    TogglePlayPause,
    Play,
    Pause,
    /// The previous / next track keys: Glance's previous / next clip in the folder.
    Previous,
    Next,
    /// Scrubbing in the Now Playing surface: a position in seconds.
    Seek(f64),
}

/// Something the OS sent the app.
#[derive(Clone, Debug, PartialEq)]
pub enum PlatformEvent {
    /// Files to open, in the order the OS gave them (Swift Glance opens the last).
    Open(Vec<PathBuf>),
    Media(MediaCommand),
    /// The answer to [`set_default_for`]: `Err` holds the OS's words.
    DefaultHandlerSet(Result<(), String>),
}

/// Why an OS call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformError {
    /// Not available on this system (or in this build).
    Unsupported(&'static str),
    /// Called off the main thread where the OS requires the main thread.
    NotMainThread,
    /// The OS refused or failed, in its words.
    Os(String),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlatformError::Unsupported(what) => write!(f, "{what} is not supported on this system"),
            PlatformError::NotMainThread => f.write_str("must be called on the main thread"),
            PlatformError::Os(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for PlatformError {}

type Waker = Box<dyn Fn() + Send>;

static EVENTS: Mutex<VecDeque<PlatformEvent>> = Mutex::new(VecDeque::new());
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);

/// Queue an event and wake the app. Called from OS callbacks (any thread).
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub(crate) fn post(event: PlatformEvent) {
    if let Ok(mut q) = EVENTS.lock() {
        q.push_back(event);
    }
    if let Ok(w) = WAKER.lock()
        && let Some(wake) = w.as_ref()
    {
        wake();
    }
}

/// Everything the OS sent since the last call, oldest first.
pub fn take_events() -> Vec<PlatformEvent> {
    EVENTS.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default()
}

/// Called (on any thread) whenever an event is queued: the app wakes its event loop here.
pub fn set_waker(wake: impl Fn() + Send + 'static) {
    if let Ok(mut w) = WAKER.lock() {
        *w = Some(Box::new(wake));
    }
}

/// The per-user folder where an app's settings live on systems without a preferences domain: on
/// Windows `%APPDATA%\<vendor>\<app>` (the roaming application-data folder, the one documented
/// place; `None` when `APPDATA` is not set). On macOS the preferences domain is the store
/// ([`Defaults`]); this folder is then `~/Library/Application Support/<vendor>/<app>`, for files
/// that are not settings.
pub fn settings_dir(vendor: &str, app: &str) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)?
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join("Library").join("Application Support")
    };
    Some(base.join(vendor).join(app))
}

/// What the Now Playing surface shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NowPlaying {
    /// The file name; `None` clears the surface (no clip).
    pub title: Option<String>,
    /// Seconds; shown when finite and above 0.
    pub duration: f64,
    pub elapsed: f64,
    pub playing: bool,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn events_queue_in_order_and_wake_the_app() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        set_waker(move || {
            w.fetch_add(1, Ordering::SeqCst);
        });
        post(PlatformEvent::Open(vec!["/a.mov".into(), "/b.mov".into()]));
        post(PlatformEvent::Media(MediaCommand::Seek(1.5)));
        let got = take_events();
        assert_eq!(got, vec![PlatformEvent::Open(vec!["/a.mov".into(), "/b.mov".into()]), PlatformEvent::Media(MediaCommand::Seek(1.5))]);
        assert!(take_events().is_empty());
        assert_eq!(woken.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn the_settings_folder_is_per_user_and_per_app() {
        let dir = settings_dir("Amritus", "Glance").unwrap();
        assert!(dir.ends_with("Amritus/Glance") || dir.ends_with(r"Amritus\Glance"), "{}", dir.display());
    }
}
