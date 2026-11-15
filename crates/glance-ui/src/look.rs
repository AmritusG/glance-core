//! Glance's look as numbers, quoted from Swift Glance a9dc601 (`glance/Sources/Glance/`):
//! `TransportBar.swift`, `ScrubBar.swift`, `PlayerScreen.swift`, `GlanceApp.swift`. Sizes are in
//! points. SwiftUI's `Color(white: w)` is the sRGB grey `w`, here `round(w * 255)`; `opacity(a)`
//! over the bar is premultiplied white at `a`.

use egui::Color32;

/// `Color(white: w)` as an opaque egui colour.
pub fn grey(w: f32) -> Color32 {
    Color32::from_gray((w * 255.0).round() as u8)
}

/// `Color.white.opacity(a)` (premultiplied).
pub fn white(a: f32) -> Color32 {
    Color32::from_white_alpha((a * 255.0).round() as u8)
}

// --- Window (GlanceApp.swift:44,64; PlayerScreen.swift:171; TransportBar.swift:21) ---

/// Default content size: 960 × (576 + 40).
pub const DEFAULT_SIZE: (f64, f64) = (960.0, 616.0);
/// Minimum content size: the bar's minimum width × 180.
pub const MIN_SIZE: (f64, f64) = (500.0, 180.0);
/// On each clip open the width may grow to this share of the screen's width.
pub const MAX_SCREEN_SHARE: f64 = 0.95;

// --- Transport bar (TransportBar.swift:14-21,24,100-114) ---

pub const BAR_HEIGHT: f32 = 40.0;
/// `HStack(spacing: 2)`, `.padding(.horizontal, 10)`.
pub const BAR_SPACING: f32 = 2.0;
pub const BAR_PADDING: f32 = 10.0;
/// Bar and scrub row background, `Color(white: 0.08)`.
pub const BAR_GREY: f32 = 0.08;
/// The bar's 1-pt top hairline, `Color.white.opacity(0.06)`.
pub const HAIRLINE_WHITE: f32 = 0.06;
/// Separators: 1 × 18, `Color.white.opacity(0.08)`, `.padding(.horizontal, 4)`.
pub const SEPARATOR_SIZE: (f32, f32) = (1.0, 18.0);
pub const SEPARATOR_WHITE: f32 = 0.08;
pub const SEPARATOR_PADDING: f32 = 4.0;

// --- Icon buttons (TransportBar.swift:155-170) ---

/// The button: 28 × 28, rounded 5, hover fill white 12 %.
pub const ICON_BOX: f32 = 28.0;
pub const ICON_CORNER: f32 = 5.0;
pub const HOVER_WHITE: f32 = 0.12;
/// SF Symbol at 13 pt, weight medium, `Color(white: 0.78)`; lit: the active colour.
pub const ICON_POINT_SIZE: f32 = 13.0;
pub const ICON_GREY: f32 = 0.78;
/// Previous / next clip without a sibling: `.opacity(0.3)`.
pub const DIMMED: f32 = 0.3;
/// The loop button lit: `.blue` — the system blue, dark appearance (the bar is dark; macOS Dark
/// is this Mac's appearance).
pub const ACTIVE_BLUE: Color32 = Color32::from_rgb(10, 132, 255);

// --- Volume and readouts (TransportBar.swift:71-98,195-215) ---

/// `Slider … .controlSize(.mini).frame(width: 90).tint(Color(white: 0.75))`.
pub const VOLUME_WIDTH: f32 = 90.0;
pub const VOLUME_TINT: f32 = 0.75;
/// The Alpha Type control's `.padding(.leading, 6)`; `Spacer(minLength: 8)` after it.
pub const ALPHA_LEADING: f32 = 6.0;
pub const SPACER_MIN: f32 = 8.0;
/// The readout: 10 pt monospaced, `Color(white: 0.5)`, truncating at the tail.
pub const READOUT_SIZE: f32 = 10.0;
pub const READOUT_GREY: f32 = 0.5;
/// "Alpha Type: …" at 11 pt; the hint at 10 pt grey 0.5; a stated type at 11 pt grey 0.45.
pub const ALPHA_SIZE: f32 = 11.0;
pub const HINT_SIZE: f32 = 10.0;
pub const HINT_GREY: f32 = 0.5;
pub const STATED_GREY: f32 = 0.45;
/// `HStack(spacing: 5)` between the Alpha Type menu and the hint.
pub const ALPHA_HINT_SPACING: f32 = 5.0;

// --- Scrub row (ScrubBar.swift:12-31; PlayerScreen.swift:36-38) ---

pub const SCRUB_HEIGHT: f32 = 36.0;
/// `HStack(spacing: 12)`, `.padding(.horizontal, 12)`.
pub const SCRUB_SPACING: f32 = 12.0;
pub const SCRUB_PADDING: f32 = 12.0;
/// Time and frame labels: 11 pt monospaced, white 85 %; widths 64 (right-aligned) and 56
/// (left-aligned).
pub const SCRUB_LABEL_SIZE: f32 = 11.0;
pub const SCRUB_LABEL_WHITE: f32 = 0.85;
pub const TIME_WIDTH: f32 = 64.0;
pub const FRAME_WIDTH: f32 = 56.0;

// --- Sliders (AppKit's NSSlider, linear, as drawn on macOS 14+; approximations for the look review) ---

/// Regular slider (the scrubber): track 4 pt, knob 20 pt; mini (volume): track 3 pt, knob 12 pt.
pub const SCRUB_TRACK: f32 = 4.0;
pub const SCRUB_KNOB: f32 = 20.0;
pub const MINI_TRACK: f32 = 3.0;
pub const MINI_KNOB: f32 = 12.0;
/// The unfilled track: white 10 % over the bar (grey 44, measured on Swift Glance's scrubber in
/// its window capture, docs/data/c3-look/stats.txt); the knob: white.
pub const TRACK_WHITE: f32 = 0.1;

// --- "Not supported" overlay (PlayerScreen.swift:20-35) ---

/// Headline (13 pt semibold) over caption (10 pt), `VStack(spacing: 6)`, `.padding(20)`, white;
/// the caption `.secondary` (white 55 %).
pub const HEADLINE_SIZE: f32 = 13.0;
pub const CAPTION_SIZE: f32 = 10.0;
pub const MESSAGE_SPACING: f32 = 6.0;
pub const MESSAGE_PADDING: f32 = 20.0;
pub const SECONDARY_WHITE: f32 = 0.55;
/// The caption under every "not supported" message.
pub const NOT_SUPPORTED_CAPTION: &str = "Glance plays DXV, HAP and the formats macOS plays (H.264, HEVC, ProRes, Motion JPEG).";

/// The egui visuals of a Glance window: dark, black behind everything, tooltips and menus as
/// egui draws them (the Swift app's are the OS's: a look-review item).
pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::BLACK;
    visuals.window_fill = grey(0.15);
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.spacing.item_spacing = egui::vec2(BAR_SPACING, 0.0);
        s.spacing.button_padding = egui::Vec2::ZERO;
        s.interaction.tooltip_delay = 0.5;
    });
}
