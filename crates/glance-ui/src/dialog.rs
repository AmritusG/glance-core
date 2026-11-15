//! Dialogs with Swift Glance's settings and words (`glance/Sources/Glance/GlanceApp.swift:68-80`,
//! `DefaultHandlerControl.swift:81-149`). Modal, on the main thread.
//!
//! The open panel is rfd's (`NSOpenPanel`, the common file dialog on Windows). Alerts are
//! [`glance_platform::alert`]: on macOS an app-modal `NSAlert` in the app's own process, with its
//! icon, as Swift runs it (rfd without a parent window hands the alert to UserNotificationCenter,
//! where it outlives the app; with one it is a sheet); on Windows a message box owned by the app's
//! window. A failed alert is not shown (the app goes on).

use glance_platform::AlertStyle;
use rfd::FileDialog;
use std::path::PathBuf;

/// Movie file extensions for the open panel (rfd filters by extension; Swift's panel allows
/// `public.movie`, `com.apple.quicktime-movie` and `public.mpeg-4`, which this list stands in for).
pub use glance_platform::MOVIE_EXTENSIONS;

/// File ▸ Open…: one movie file, folders not choosable.
pub fn open_movie() -> Option<PathBuf> {
    FileDialog::new().add_filter("Movie", &MOVIE_EXTENSIONS).pick_file()
}

/// An informational alert with one OK button.
pub fn inform(title: &str, text: &str) {
    let _ = glance_platform::alert(AlertStyle::Informational, title, text, &["OK"]);
}

/// A warning alert with one OK button.
pub fn warn(title: &str, text: &str) {
    let _ = glance_platform::alert(AlertStyle::Warning, title, text, &["OK"]);
}

/// An informational question with two buttons; `true` when the first (`yes`, the default) is chosen.
pub fn ask(title: &str, text: &str, yes: &str, no: &str) -> bool {
    glance_platform::alert(AlertStyle::Informational, title, text, &[yes, no]) == Ok(0)
}

/// Swift Glance's words for the default-handler flow, with the app's name in them.
pub mod default_handler {
    /// The first-launch question (`showFirstLaunchPromptIfNeeded`).
    pub fn prompt(app: &str) -> (String, String) {
        (
            format!("Make {app} the default for .mov files?"),
            format!(
                "{app} can open all .mov files. DXV and HAP files use {app}'s own GPU decoders; other formats use macOS's built-in decoders. You can change this anytime in Finder (Get Info → Open with)."
            ),
        )
    }
    pub const SET: &str = "Set as Default";
    pub const NOT_NOW: &str = "Not Now";

    /// The menu item, already the default.
    pub fn already(app: &str) -> String {
        format!("{app} is already the default for .mov files.")
    }

    /// The menu item, done.
    pub fn done(app: &str) -> (String, String) {
        (format!("{app} is now the default for .mov files."), format!("Double-click any .mov file in Finder to open it in {app}."))
    }

    /// The menu item, refused, with the OS's words and the manual way.
    pub fn failed(app: &str, why: &str) -> (String, String) {
        (
            format!("Couldn't set {app} as default."),
            format!("{why}\n\nYou can set it manually: right-click a .mov file in Finder → Get Info → Open with: {app} → Change All."),
        )
    }

    /// The content type the flow is about: QuickTime movies.
    pub const MOVIE_TYPE: &str = "com.apple.quicktime-movie";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_are_swift_glances() {
        let (t, body) = default_handler::prompt("Glance");
        assert_eq!(t, "Make Glance the default for .mov files?");
        assert!(body.starts_with("Glance can open all .mov files. DXV and HAP files use Glance's own GPU decoders"));
        assert_eq!(default_handler::already("Glance"), "Glance is already the default for .mov files.");
        assert_eq!(
            default_handler::failed("Glance", "x").1,
            "x\n\nYou can set it manually: right-click a .mov file in Finder → Get Info → Open with: Glance → Change All."
        );
        assert!(MOVIE_EXTENSIONS.contains(&"mov") && MOVIE_EXTENSIONS.contains(&"mp4"));
    }
}
