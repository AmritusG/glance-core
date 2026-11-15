//! Swift Glance's menu bar in muda, item for item and shortcut for shortcut
//! (`glance/Sources/Glance/GlanceApp.swift:67-164`, plus what SwiftUI and AppKit add by themselves:
//! the app menu, Edit, Window, Help, and View ▸ Enter Full Screen). The menu bar only names what was
//! chosen ([`MenuCommand`]); the app applies it, so menu, keys and bar drive the same calls.
//!
//! Space and A are menu shortcuts without a modifier, as in Swift (`.keyboardShortcut(.space,
//! modifiers: [])`): the menu takes the key, so it fires once (Swift: "Space lives only in the
//! menu shortcut, or it fires twice"). Mute and Loop have no menu shortcut (⌘M is Minimize).
//!
//! On Windows the same menus hang in the window (`glance_platform::attach_menu`), without the
//! macOS app menu; its items move to File (Exit) and Help (About).

use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

/// What a Glance menu item asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCommand {
    /// File ▸ Open… (⌘O)
    Open,
    /// Glance ▸ Set Glance as Default for .mov Files…
    SetAsDefault,
    /// View ▸ Default Window Size (⌘0)
    DefaultWindowSize,
    /// View ▸ Background ▸ Black / Checkerboard
    BackgroundBlack,
    BackgroundCheckerboard,
    /// View ▸ Alpha Type ▸ Premultiplied / Straight
    AlphaPremultiplied,
    AlphaStraight,
    /// View ▸ Toggle Alpha Type (A)
    ToggleAlphaType,
    /// Playback ▸ Play/Pause (Space)
    PlayPause,
    /// Playback ▸ Previous Clip (⌘←) / Next Clip (⌘→)
    PreviousClip,
    NextClip,
    /// Playback ▸ Restart
    Restart,
    /// Playback ▸ Volume Up (⌘↑) / Volume Down (⌘↓)
    VolumeUp,
    VolumeDown,
    /// Playback ▸ Mute / Loop
    Mute,
    Loop,
}

impl MenuCommand {
    pub const ALL: [MenuCommand; 16] = [
        MenuCommand::Open,
        MenuCommand::SetAsDefault,
        MenuCommand::DefaultWindowSize,
        MenuCommand::BackgroundBlack,
        MenuCommand::BackgroundCheckerboard,
        MenuCommand::AlphaPremultiplied,
        MenuCommand::AlphaStraight,
        MenuCommand::ToggleAlphaType,
        MenuCommand::PlayPause,
        MenuCommand::PreviousClip,
        MenuCommand::NextClip,
        MenuCommand::Restart,
        MenuCommand::VolumeUp,
        MenuCommand::VolumeDown,
        MenuCommand::Mute,
        MenuCommand::Loop,
    ];

    /// The muda id of the item.
    pub fn id(self) -> &'static str {
        match self {
            MenuCommand::Open => "glance.open",
            MenuCommand::SetAsDefault => "glance.set-as-default",
            MenuCommand::DefaultWindowSize => "glance.default-window-size",
            MenuCommand::BackgroundBlack => "glance.background.black",
            MenuCommand::BackgroundCheckerboard => "glance.background.checkerboard",
            MenuCommand::AlphaPremultiplied => "glance.alpha.premultiplied",
            MenuCommand::AlphaStraight => "glance.alpha.straight",
            MenuCommand::ToggleAlphaType => "glance.alpha.toggle",
            MenuCommand::PlayPause => "glance.play-pause",
            MenuCommand::PreviousClip => "glance.previous-clip",
            MenuCommand::NextClip => "glance.next-clip",
            MenuCommand::Restart => "glance.restart",
            MenuCommand::VolumeUp => "glance.volume-up",
            MenuCommand::VolumeDown => "glance.volume-down",
            MenuCommand::Mute => "glance.mute",
            MenuCommand::Loop => "glance.loop",
        }
    }

    pub fn from_id(id: &str) -> Option<MenuCommand> {
        MenuCommand::ALL.into_iter().find(|c| c.id() == id)
    }

    /// The item's title, as Swift Glance writes it.
    pub fn title(self, app: &str) -> String {
        match self {
            MenuCommand::Open => "Open…".into(),
            MenuCommand::SetAsDefault => format!("Set {app} as Default for .mov Files…"),
            MenuCommand::DefaultWindowSize => "Default Window Size".into(),
            MenuCommand::BackgroundBlack => "Black".into(),
            MenuCommand::BackgroundCheckerboard => "Checkerboard".into(),
            MenuCommand::AlphaPremultiplied => "Premultiplied".into(),
            MenuCommand::AlphaStraight => "Straight".into(),
            MenuCommand::ToggleAlphaType => "Toggle Alpha Type".into(),
            MenuCommand::PlayPause => "Play/Pause".into(),
            MenuCommand::PreviousClip => "Previous Clip".into(),
            MenuCommand::NextClip => "Next Clip".into(),
            MenuCommand::Restart => "Restart".into(),
            MenuCommand::VolumeUp => "Volume Up".into(),
            MenuCommand::VolumeDown => "Volume Down".into(),
            MenuCommand::Mute => "Mute".into(),
            MenuCommand::Loop => "Loop".into(),
        }
    }

    /// The item's shortcut (⌘ on macOS is Ctrl on Windows).
    pub fn shortcut(self) -> Option<Accelerator> {
        let cmd = muda::accelerator::CMD_OR_CTRL;
        let key = |m: Modifiers, c: Code| Some(Accelerator::new(m, c));
        match self {
            MenuCommand::Open => key(cmd, Code::KeyO),
            MenuCommand::DefaultWindowSize => key(cmd, Code::Digit0),
            MenuCommand::ToggleAlphaType => key(Modifiers::empty(), Code::KeyA),
            MenuCommand::PlayPause => key(Modifiers::empty(), Code::Space),
            MenuCommand::PreviousClip => key(cmd, Code::ArrowLeft),
            MenuCommand::NextClip => key(cmd, Code::ArrowRight),
            MenuCommand::VolumeUp => key(cmd, Code::ArrowUp),
            MenuCommand::VolumeDown => key(cmd, Code::ArrowDown),
            _ => None,
        }
    }
}

fn item(app: &str, c: MenuCommand) -> MenuItem {
    MenuItem::with_id(c.id(), c.title(app), true, c.shortcut())
}

fn check(app: &str, c: MenuCommand, checked: bool) -> CheckMenuItem {
    CheckMenuItem::with_id(c.id(), c.title(app), true, checked, c.shortcut())
}

/// The menu bar and the items whose state follows the app (checks, enabled).
pub struct GlanceMenu {
    pub menu: Menu,
    background: [CheckMenuItem; 2],
    alpha: [CheckMenuItem; 2],
    toggle_alpha: MenuItem,
    window_menu: Submenu,
    help_menu: Submenu,
}

fn err(e: muda::Error) -> String {
    e.to_string()
}

impl GlanceMenu {
    /// Build the menu bar of an app named `app` (Glance's menus; the name goes into the app menu,
    /// About and "Set … as Default"). Build it on the main thread, once the event loop exists.
    pub fn new(app: &str, version: &str) -> Result<GlanceMenu, String> {
        let s = PredefinedMenuItem::separator;
        let background = [check(app, MenuCommand::BackgroundBlack, true), check(app, MenuCommand::BackgroundCheckerboard, false)];
        let alpha = [check(app, MenuCommand::AlphaPremultiplied, true), check(app, MenuCommand::AlphaStraight, false)];
        let toggle_alpha = item(app, MenuCommand::ToggleAlphaType);
        for i in alpha.iter() {
            i.set_enabled(false);
        }
        toggle_alpha.set_enabled(false);
        let about = AboutMetadata { name: Some(app.into()), version: Some(version.into()), ..Default::default() };

        let background_menu = Submenu::with_items("Background", true, &[&background[0], &background[1]]).map_err(err)?;
        let alpha_menu = Submenu::with_items("Alpha Type", true, &[&alpha[0], &alpha[1]]).map_err(err)?;
        let view = Submenu::with_items(
            "View",
            true,
            &[&item(app, MenuCommand::DefaultWindowSize), &background_menu, &alpha_menu, &toggle_alpha, &s(), &PredefinedMenuItem::fullscreen(None)],
        )
        .map_err(err)?;
        let playback = Submenu::with_items(
            "Playback",
            true,
            &[
                &item(app, MenuCommand::PlayPause),
                &s(),
                &item(app, MenuCommand::PreviousClip),
                &item(app, MenuCommand::NextClip),
                &item(app, MenuCommand::Restart),
                &s(),
                &item(app, MenuCommand::VolumeUp),
                &item(app, MenuCommand::VolumeDown),
                &item(app, MenuCommand::Mute),
                &item(app, MenuCommand::Loop),
            ],
        )
        .map_err(err)?;
        let edit = Submenu::with_items(
            "Edit",
            true,
            &[
                &PredefinedMenuItem::undo(None),
                &PredefinedMenuItem::redo(None),
                &s(),
                &PredefinedMenuItem::cut(None),
                &PredefinedMenuItem::copy(None),
                &PredefinedMenuItem::paste(None),
                &PredefinedMenuItem::select_all(None),
            ],
        )
        .map_err(err)?;
        let window_menu = Submenu::with_items(
            "Window",
            true,
            &[&PredefinedMenuItem::minimize(None), &PredefinedMenuItem::zoom(None), &s(), &PredefinedMenuItem::bring_all_to_front(None)],
        )
        .map_err(err)?;
        let menu = Menu::new();
        let help_menu;
        if cfg!(target_os = "macos") {
            let app_menu = Submenu::with_items(
                app,
                true,
                &[
                    &PredefinedMenuItem::about(None, Some(about)),
                    &s(),
                    &item(app, MenuCommand::SetAsDefault),
                    &s(),
                    &PredefinedMenuItem::services(None),
                    &s(),
                    &PredefinedMenuItem::hide(None),
                    &PredefinedMenuItem::hide_others(None),
                    &PredefinedMenuItem::show_all(None),
                    &s(),
                    &PredefinedMenuItem::quit(None),
                ],
            )
            .map_err(err)?;
            let file = Submenu::with_items("File", true, &[&item(app, MenuCommand::Open), &s(), &PredefinedMenuItem::close_window(None)]).map_err(err)?;
            help_menu = Submenu::new("Help", true);
            menu.append_items(&[&app_menu, &file, &edit, &view, &playback, &window_menu, &help_menu]).map_err(err)?;
        } else {
            let file = Submenu::with_items(
                "File",
                true,
                &[&item(app, MenuCommand::Open), &item(app, MenuCommand::SetAsDefault), &s(), &PredefinedMenuItem::quit(Some("Exit"))],
            )
            .map_err(err)?;
            help_menu = Submenu::with_items("Help", true, &[&PredefinedMenuItem::about(None, Some(about))]).map_err(err)?;
            menu.append_items(&[&file, &edit, &view, &playback, &window_menu, &help_menu]).map_err(err)?;
        }
        Ok(GlanceMenu { menu, background, alpha, toggle_alpha, window_menu, help_menu })
    }

    /// Make this the app's menu bar (macOS; on Windows `glance_platform::attach_menu` with the
    /// window). Call once the event loop runs.
    #[cfg(target_os = "macos")]
    pub fn attach_to_app(&self) {
        self.menu.init_for_nsapp();
        self.window_menu.set_as_windows_menu_for_nsapp();
        self.help_menu.set_as_help_menu_for_nsapp();
    }

    /// The check beside View ▸ Background.
    pub fn set_background(&self, checkerboard: bool) {
        self.background[0].set_checked(!checkerboard);
        self.background[1].set_checked(checkerboard);
    }

    /// View ▸ Alpha Type: enabled for DXV clips with alpha only, the check on the current type.
    pub fn set_alpha(&self, choosable: bool, straight: bool) {
        for i in &self.alpha {
            i.set_enabled(choosable);
        }
        self.alpha[0].set_checked(!straight);
        self.alpha[1].set_checked(straight);
        self.toggle_alpha.set_enabled(choosable);
    }

    /// The Glance commands chosen since the last call (other ids — the predefined items — are
    /// handled by the OS and not reported).
    pub fn take_commands() -> Vec<MenuCommand> {
        let mut out = Vec::new();
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            if let Some(c) = MenuCommand::from_id(e.id().as_ref()) {
                out.push(c);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_items_and_shortcuts_are_swift_glances() {
        for c in MenuCommand::ALL {
            assert_eq!(MenuCommand::from_id(c.id()), Some(c));
        }
        assert_eq!(MenuCommand::SetAsDefault.title("Glance"), "Set Glance as Default for .mov Files…");
        assert_eq!(MenuCommand::Open.title("Glance"), "Open…");
        let none = Modifiers::empty();
        let cmd = muda::accelerator::CMD_OR_CTRL;
        assert_eq!(MenuCommand::PlayPause.shortcut(), Some(Accelerator::new(none, Code::Space)));
        assert_eq!(MenuCommand::ToggleAlphaType.shortcut(), Some(Accelerator::new(none, Code::KeyA)));
        assert_eq!(MenuCommand::DefaultWindowSize.shortcut(), Some(Accelerator::new(cmd, Code::Digit0)));
        assert_eq!(MenuCommand::VolumeDown.shortcut(), Some(Accelerator::new(cmd, Code::ArrowDown)));
        // ⌘M is Minimize: Mute and Loop have none; Restart has none in Swift either.
        for c in [MenuCommand::Mute, MenuCommand::Loop, MenuCommand::Restart, MenuCommand::SetAsDefault] {
            assert_eq!(c.shortcut(), None, "{c:?}");
        }
    }
}
