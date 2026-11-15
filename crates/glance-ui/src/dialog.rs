//! Dialogs through rfd (the OS's own panels: `NSOpenPanel` / `NSAlert` on macOS, the common
//! dialogs on Windows), with Swift Glance's settings and words
//! (`glance/Sources/Glance/GlanceApp.swift:68-80`, `DefaultHandlerControl.swift:81-149`). Modal,
//! on the main thread.

use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::path::PathBuf;

/// Movie file extensions for the open panel. Swift's panel allows the content types
/// `public.movie`, `com.apple.quicktime-movie` and `public.mpeg-4`, which the OS resolves to every
/// movie type it knows; rfd filters by extension, so this list stands in for `public.movie`: the
/// QuickTime / MPEG-4 family, AVI, Matroska / WebM (shown "not supported" when opened, as Swift
/// does), MPEG transport / program streams, DV, 3GPP, Flash and Windows Media video.
pub const MOVIE_EXTENSIONS: [&str; 18] =
    ["mov", "qt", "mp4", "m4v", "avi", "mkv", "webm", "mk3d", "mpg", "mpeg", "m2v", "mts", "m2ts", "ts", "dv", "3gp", "flv", "wmv"];

/// File ▸ Open…: one movie file, folders not choosable.
pub fn open_movie() -> Option<PathBuf> {
    FileDialog::new().add_filter("Movie", &MOVIE_EXTENSIONS).pick_file()
}

/// An informational alert with one OK button.
pub fn inform(title: &str, text: &str) {
    MessageDialog::new().set_level(MessageLevel::Info).set_title(title).set_description(text).set_buttons(MessageButtons::Ok).show();
}

/// A warning alert with one OK button.
pub fn warn(title: &str, text: &str) {
    MessageDialog::new().set_level(MessageLevel::Warning).set_title(title).set_description(text).set_buttons(MessageButtons::Ok).show();
}

/// An informational question with two buttons; `true` when the first (`yes`) is chosen.
pub fn ask(title: &str, text: &str, yes: &str, no: &str) -> bool {
    let answer = MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title(title)
        .set_description(text)
        .set_buttons(MessageButtons::OkCancelCustom(yes.into(), no.into()))
        .show();
    matches!(answer, MessageDialogResult::Ok) || matches!(answer, MessageDialogResult::Custom(ref c) if c == yes)
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
