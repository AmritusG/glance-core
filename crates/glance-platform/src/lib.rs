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
//! - [`note_recent`]: the OS recent-documents list (`NSDocumentController`);
//! - [`install_menu_key_monitor`]: menu shortcuts without a modifier (Space, A) reach the menu;
//! - [`alert`]: an app-modal `NSAlert` in this process (Esc answers the second of two buttons);
//! - [`set_single_window`], [`finish_app_menus`]: Swift Glance's single
//!   window (no tabs, not listed in the Window menu) and the system menu items SwiftUI gives it;
//! - [`install_terminate_guard`] / [`set_unsaved_work`]: a quit through AppKit waits for an editor's question
//!   ([`PlatformEvent::QuitRequested`]); [`set_frame_autosave`]: the window's frame saved and restored by name;
//!   [`pointer_in_window`]: where a drop landed; [`open_path`]: open a file or folder as Finder does (v0.4.0); [`reveal_in_finder`] (v0.5.0);
//! - [`movie_files`]: a folder's movies in Finder's order ([`localized_standard_compare`],
//!   [`conforms_to_movie`]), hidden files left out.
//!
//! **Windows** (written and compile-checked; run at the Windows milestone, C4):
//! [`SingleInstance`] (a named mutex plus a named pipe that hands a second launch's paths to the
//! first), [`settings_dir`] (the one documented per-user place), [`MediaKeys`] on the System
//! Media Transport Controls, [`alert`] (a message box owned by the app's active window) and
//! [`movie_files`] (`StrCmpLogicalW` order, [`MOVIE_EXTENSIONS`], hidden attribute).
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
pub use macos::{
    Defaults, MediaKeys, alert, conforms_to_movie, finish_app_menus, install_menu_key_monitor, install_open_handler, install_terminate_guard, is_default_for,
    localized_standard_compare, movie_files, note_recent, open_path, pointer_in_window, reveal_in_finder, set_default_for, set_frame_autosave,
    set_single_window, set_subtitle, set_unsaved_work,
};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{MediaKeys, SingleInstance, alert, attach_menu, conforms_to_movie, localized_standard_compare, movie_files};

/// Off macOS the quit guard is the app's own close handling: nothing to tell the OS.
#[cfg(not(target_os = "macos"))]
pub fn set_unsaved_work(_unsaved: bool) {}

#[cfg(not(any(target_os = "macos", windows)))]
mod other;
#[cfg(not(any(target_os = "macos", windows)))]
pub use other::{MediaKeys, movie_files};

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
    /// The OS asked the app to quit (the Dock, a quit Apple Event, log out) while it had unsaved work
    /// ([`set_unsaved_work`]); the quit was held back so the app can ask about the work and quit itself.
    QuitRequested,
}

/// How an alert looks (`NSAlert.Style`; on Windows the message box's icon).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertStyle {
    Informational,
    Warning,
}

/// Movie file extensions, for an open panel that filters by extension and for the folder scan
/// where the OS has no type test (Windows). On macOS the panel's and the scan's own test is the
/// OS's (`public.movie` and its kin); this list stands in for it: the QuickTime / MPEG-4 family,
/// AVI, Matroska / WebM (shown "not supported" when opened, as Swift does), MPEG transport /
/// program streams, DV, 3GPP, Flash and Windows Media video.
pub const MOVIE_EXTENSIONS: [&str; 18] =
    ["mov", "qt", "mp4", "m4v", "avi", "mkv", "webm", "mk3d", "mpg", "mpeg", "m2v", "mts", "m2ts", "ts", "dv", "3gp", "flv", "wmv"];

/// Sort `items` with the OS's name order. A merge sort of our own: the OS's comparison is not
/// promised to be a total order (Apple and Microsoft both say it may change between releases;
/// macOS 27 already compares digit runs past 2^64 by their value modulo 2^64), and std's sorts may
/// panic on a comparison that is not one. This one always returns, every item once.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub(crate) fn sort_by_os<T: Clone>(items: &[T], cmp: &impl Fn(&T, &T) -> std::cmp::Ordering) -> Vec<T> {
    if items.len() <= 1 {
        return items.to_vec();
    }
    let (left, right) = items.split_at(items.len() / 2);
    let (left, right) = (sort_by_os(left, cmp), sort_by_os(right, cmp));
    let mut out = Vec::with_capacity(items.len());
    let (mut l, mut r) = (left.into_iter().peekable(), right.into_iter().peekable());
    while let (Some(a), Some(b)) = (l.peek(), r.peek()) {
        let next = if cmp(b, a) == std::cmp::Ordering::Less { r.next() } else { l.next() };
        out.extend(next);
    }
    out.extend(l);
    out.extend(r);
    out
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
    fn the_os_sort_keeps_every_item_even_when_the_order_is_inconsistent() {
        let v: Vec<u32> = (0..50).rev().collect();
        assert_eq!(sort_by_os(&v, &|a, b| a.cmp(b)), (0..50).collect::<Vec<_>>());
        // Stable: equal keys keep their order.
        let pairs = [(1, 'a'), (0, 'b'), (1, 'c'), (0, 'd')];
        assert_eq!(sort_by_os(&pairs, &|a, b| a.0.cmp(&b.0)), vec![(0, 'b'), (0, 'd'), (1, 'a'), (1, 'c')]);
        // A comparison that is no order at all: no panic, the same items back.
        let mut got = sort_by_os(&v, &|a, b| if (a + b) % 3 == 0 { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater });
        got.sort_unstable();
        assert_eq!(got, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn the_settings_folder_is_per_user_and_per_app() {
        let dir = settings_dir("Amritus", "Glance").unwrap();
        assert!(dir.ends_with("Amritus/Glance") || dir.ends_with(r"Amritus\Glance"), "{}", dir.display());
    }
}
