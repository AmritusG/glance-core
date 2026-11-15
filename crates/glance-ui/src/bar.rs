//! Swift Glance's transport bar, scrub row, Alpha Type control and "not supported" overlay in
//! egui (`glance/Sources/Glance/TransportBar.swift`, `ScrubBar.swift`, `PlayerScreen.swift`), and
//! its text formatters (`glance/Sources/GlancePlayback/ClipTitle.swift`). The widgets only draw
//! and report what was pressed ([`BarAction`], [`ScrubAction`]); the app applies it to
//! glance-playback's clock, so keys, menus and buttons drive the same calls (Swift's facade).

use crate::icons::{self, Icon};
use crate::look::{self, grey, white};
use egui::{Align, Align2, Color32, FontFamily, FontId, Layout, Rect, Sense, Stroke, Ui, pos2, vec2};
use glance_media::AlphaView;

// ---------------------------------------------------------------------------------------------
// Text (ClipTitle.swift, ScrubBar.swift)

/// `fpsLabel`: a whole number when within 0.05 of one (23.976 → "24fps"), else one decimal.
pub fn fps_label(fps: f64) -> String {
    let whole = fps.round();
    if (fps - whole).abs() < 0.05 { format!("{}fps", whole as i64) } else { format!("{fps:.1}fps") }
}

/// `durationLabel`: `M:SS`, `H:MM:SS` from one hour; `0:00` for nothing.
pub fn duration_label(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "0:00".into();
    }
    let total = seconds.round() as i64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// `videoInfoSubtitle`: `W×H · NNfps · M:SS` (the window subtitle and the bar's readout).
pub fn info_line(width: u32, height: u32, fps: f64, duration: f64) -> String {
    format!("{width}×{height} · {} · {}", fps_label(fps), duration_label(duration))
}

/// `clipTitleSuffix`: `[<codec>]  [LOOP]`, two spaces between, each part only when it applies.
pub fn title_suffix(codec: Option<&str>, looping: bool) -> String {
    let mut parts = Vec::new();
    if let Some(c) = codec {
        parts.push(format!("[{c}]"));
    }
    if looping {
        parts.push("[LOOP]".to_string());
    }
    parts.join("  ")
}

/// The window title: `Glance` without a file, else `<file name>  <suffix>` (PlayerScreen.swift:77-84).
pub fn window_title(app: &str, file: Option<&str>, suffix: &str) -> String {
    match file {
        None => app.to_string(),
        Some(f) if suffix.is_empty() => f.to_string(),
        Some(f) => format!("{f}  {suffix}"),
    }
}

/// The scrub row's time: `M:SS.mmm` (`%d:%06.3f`); `0:00.000` for nothing.
pub fn time_label(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00.000".into();
    }
    let mins = (seconds as i64) / 60;
    let secs = seconds - (mins * 60) as f64;
    format!("{mins}:{secs:06.3}")
}

// ---------------------------------------------------------------------------------------------
// Pieces

fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// `IconButton`: 28 × 28, hover fill, the icon in grey 0.78 or the active colour.
fn icon_button(ui: &mut Ui, icon: Icon, help: &str, enabled: bool, active: Option<Color32>) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(look::ICON_BOX, look::ICON_BOX), if enabled { Sense::click() } else { Sense::hover() });
    let opacity = if enabled { 1.0 } else { look::DIMMED };
    if enabled && response.hovered() {
        ui.painter().rect_filled(rect, look::ICON_CORNER, white(look::HOVER_WHITE));
    }
    let color = active.unwrap_or_else(|| grey(look::ICON_GREY)).gamma_multiply(opacity);
    icons::draw(ui.painter(), rect, icon, color);
    let response = response.on_hover_text(help);
    enabled && response.clicked()
}

fn separator(ui: &mut Ui) {
    ui.add_space(look::SEPARATOR_PADDING);
    let (rect, _) = ui.allocate_exact_size(vec2(look::SEPARATOR_SIZE.0, look::BAR_HEIGHT), Sense::hover());
    ui.painter().rect_filled(Rect::from_center_size(rect.center(), vec2(look::SEPARATOR_SIZE.0, look::SEPARATOR_SIZE.1)), 0.0, white(look::SEPARATOR_WHITE));
    ui.add_space(look::SEPARATOR_PADDING);
}

/// An NSSlider-like linear slider over `0..=1`: a track, its filled part in `fill`, a white knob.
/// Returns the response (drag started / stopped) and the value under the pointer while pressed.
fn slider(ui: &mut Ui, width: f32, height: f32, value: f32, track: f32, knob: f32, fill: Color32) -> (egui::Response, Option<f32>) {
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let inset = knob / 2.0;
    let (x0, x1) = (rect.left() + inset, (rect.right() - inset).max(rect.left() + inset));
    let at = |v: f32| x0 + (x1 - x0) * v.clamp(0.0, 1.0);
    let pressed = response.is_pointer_button_down_on() || response.dragged();
    let new = if pressed { ui.input(|i| i.pointer.interact_pos()).map(|p| ((p.x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0)) } else { None };
    let shown = new.unwrap_or(value);
    let y = rect.center().y;
    let p = ui.painter();
    p.rect_filled(Rect::from_min_max(pos2(x0, y - track / 2.0), pos2(x1, y + track / 2.0)), track / 2.0, white(look::TRACK_WHITE));
    p.rect_filled(Rect::from_min_max(pos2(x0, y - track / 2.0), pos2(at(shown), y + track / 2.0)), track / 2.0, fill);
    p.circle(pos2(at(shown), y), knob / 2.0, Color32::WHITE, Stroke::new(0.5, Color32::from_black_alpha(60)));
    (response, new)
}

// ---------------------------------------------------------------------------------------------
// The Alpha Type control (TransportBar.swift:173-218)

/// What the bar's Alpha Type control shows for the clip.
#[derive(Clone, Debug, PartialEq)]
pub enum AlphaControl {
    /// Opaque clip: nothing.
    None,
    /// DXV with alpha: the menu button `Alpha Type: <current>` and the hint.
    Choosable { current: AlphaView, hint: Option<String>, samples: u64, share: f64 },
    /// The clip states its alpha: greyed `Alpha Type: <type> (<by>)`.
    Stated { view: AlphaView, by: String },
}

/// Swift's `AlphaType.label`.
pub fn alpha_label(view: AlphaView) -> &'static str {
    match view {
        AlphaView::Premultiplied => "Premultiplied",
        AlphaView::Straight => "Straight",
    }
}

/// The control's tooltip (TransportBar.swift:201).
pub const ALPHA_HELP: &str = "What this DXV clip's colour is (A to toggle). A DXV file doesn't say; Resolume Arena calls this \"Alpha Type\", default Premultiplied. Straight: Alley, DXVKit and GlEnc 1.6 files.";

fn alpha_control(ui: &mut Ui, control: &AlphaControl, actions: &mut Vec<BarAction>) {
    match control {
        AlphaControl::None => {}
        AlphaControl::Choosable { current, hint, samples, share } => {
            ui.add_space(look::ALPHA_LEADING);
            let text = egui::RichText::new(format!("Alpha Type: {}", alpha_label(*current))).font(font(look::ALPHA_SIZE)).color(grey(look::ICON_GREY));
            let response = ui.add(egui::Button::new(text).frame(false)).on_hover_text(ALPHA_HELP);
            egui::Popup::menu(&response).show(|ui| {
                for view in [AlphaView::Premultiplied, AlphaView::Straight] {
                    let label = if view == *current { format!("✓ {}", alpha_label(view)) } else { format!("    {}", alpha_label(view)) };
                    if ui.button(label).clicked() {
                        actions.push(BarAction::SetAlpha(view));
                    }
                }
            });
            if let Some(h) = hint {
                ui.add_space(look::ALPHA_HINT_SPACING);
                ui.label(egui::RichText::new(h).font(font(look::HINT_SIZE)).color(grey(look::HINT_GREY))).on_hover_text(format!(
                    "A reading of the first frames (DXV spec 8.5): {samples} semi-transparent sample pixels, {:.1} % brighter than their alpha. Premultiplied colour can't be. A hint only: it never changes the setting.",
                    share * 100.0
                ));
            }
        }
        AlphaControl::Stated { view, by } => {
            ui.add_space(look::ALPHA_LEADING);
            ui.label(egui::RichText::new(format!("Alpha Type: {} ({by})", alpha_label(*view))).font(font(look::ALPHA_SIZE)).color(grey(look::STATED_GREY)))
                .on_hover_text(format!("{by} states its alpha; Glance reads it as stated."));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The transport bar (TransportBar.swift:23-107)

/// What the bar shows.
#[derive(Clone, Debug, PartialEq)]
pub struct BarState {
    pub paused: bool,
    pub looping: bool,
    pub muted: bool,
    /// 0…1.
    pub volume: f32,
    /// The clip's folder holds other playable files (else previous / next are dimmed).
    pub has_siblings: bool,
    pub alpha: AlphaControl,
    /// The info line, also the window subtitle.
    pub readout: String,
}

/// What was pressed in the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BarAction {
    PreviousClip,
    TogglePlay,
    NextClip,
    Restart,
    /// Seek by this many seconds (the ±10 s buttons).
    SeekBy(f64),
    Step {
        forward: bool,
    },
    ToggleLoop,
    ToggleMute,
    SetVolume(f32),
    SetAlpha(AlphaView),
    ToggleFullscreen,
}

/// Draw the 40-pt bar across `ui`'s width; returns what was pressed, in order.
pub fn transport_bar(ui: &mut Ui, state: &BarState) -> Vec<BarAction> {
    let mut actions = Vec::new();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), look::BAR_HEIGHT), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, grey(look::BAR_GREY));
    ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(rect.width(), 1.0)), 0.0, white(look::HAIRLINE_WHITE));
    let inner = rect.shrink2(vec2(look::BAR_PADDING, 0.0));
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
    row.spacing_mut().item_spacing = vec2(look::BAR_SPACING, 0.0);
    let ui = &mut row;
    let sib = state.has_siblings;
    if icon_button(ui, Icon::PreviousClip, "Previous Clip ([)", sib, None) {
        actions.push(BarAction::PreviousClip);
    }
    let (icon, help) = if state.paused { (Icon::Play, "Play (space)") } else { (Icon::Pause, "Pause (space)") };
    if icon_button(ui, icon, help, true, None) {
        actions.push(BarAction::TogglePlay);
    }
    if icon_button(ui, Icon::NextClip, "Next Clip (])", sib, None) {
        actions.push(BarAction::NextClip);
    }
    if icon_button(ui, Icon::Restart, "Restart", true, None) {
        actions.push(BarAction::Restart);
    }
    if icon_button(ui, Icon::Back10, "Back 10s", true, None) {
        actions.push(BarAction::SeekBy(-10.0));
    }
    if icon_button(ui, Icon::Forward10, "Forward 10s", true, None) {
        actions.push(BarAction::SeekBy(10.0));
    }
    separator(ui);
    if icon_button(ui, Icon::StepBack, "Step back (,)", true, None) {
        actions.push(BarAction::Step { forward: false });
    }
    if icon_button(ui, Icon::StepForward, "Step forward (.)", true, None) {
        actions.push(BarAction::Step { forward: true });
    }
    separator(ui);
    if icon_button(ui, Icon::Loop, "Loop (L)", true, state.looping.then_some(look::ACTIVE_BLUE)) {
        actions.push(BarAction::ToggleLoop);
    }
    if icon_button(ui, Icon::speaker(state.muted, state.volume), if state.muted { "Unmute (m)" } else { "Mute (m)" }, true, None) {
        actions.push(BarAction::ToggleMute);
    }
    let (response, value) = slider(ui, look::VOLUME_WIDTH, look::BAR_HEIGHT, state.volume, look::MINI_TRACK, look::MINI_KNOB, grey(look::VOLUME_TINT));
    response.on_hover_text("Volume");
    if let Some(v) = value
        && (v - state.volume).abs() > f32::EPSILON
    {
        actions.push(BarAction::SetVolume(v));
    }
    alpha_control(ui, &state.alpha, &mut actions);
    // The readout takes what is left and truncates first; the full-screen button sits at the end.
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if icon_button(ui, Icon::Fullscreen, "Fullscreen (f)", true, None) {
            actions.push(BarAction::ToggleFullscreen);
        }
        let room = (ui.available_width() - look::SPACER_MIN).max(0.0);
        ui.allocate_ui_with_layout(vec2(room, look::BAR_HEIGHT), Layout::right_to_left(Align::Center), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(&state.readout).font(mono(look::READOUT_SIZE)).color(grey(look::READOUT_GREY))).truncate());
        });
    });
    actions
}

// ---------------------------------------------------------------------------------------------
// The scrub row (ScrubBar.swift)

/// What the scrub row shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrubState {
    pub time: f64,
    pub duration: f64,
    pub frame: u32,
}

/// What the scrubber did: a drag begins (pause), moves (seek), ends (resume if it was playing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrubAction {
    Begin,
    /// Seek to this time in seconds.
    SeekTo(f64),
    End,
}

/// The scrubber's drag, kept by the app between frames: while dragging the knob follows the
/// pointer, never the clock (ScrubBar.swift:82-89).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrubDrag {
    value: Option<f64>,
}

impl ScrubDrag {
    pub fn dragging(&self) -> bool {
        self.value.is_some()
    }
}

/// Draw the 36-pt scrub row across `ui`'s width.
pub fn scrub_row(ui: &mut Ui, state: &ScrubState, drag: &mut ScrubDrag) -> Vec<ScrubAction> {
    let mut actions = Vec::new();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), look::SCRUB_HEIGHT), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, grey(look::BAR_GREY));
    let inner = rect.shrink2(vec2(look::SCRUB_PADDING, 0.0));
    let label = white(look::SCRUB_LABEL_WHITE);
    let p = ui.painter().clone();
    let time_rect = Rect::from_min_size(inner.min, vec2(look::TIME_WIDTH, inner.height()));
    let frame_rect = Rect::from_min_size(pos2(inner.right() - look::FRAME_WIDTH, inner.top()), vec2(look::FRAME_WIDTH, inner.height()));
    let duration = state.duration.max(0.001);
    let shown = drag.value.unwrap_or(state.time);
    p.text(time_rect.right_center(), Align2::RIGHT_CENTER, time_label(shown), mono(look::SCRUB_LABEL_SIZE), label);
    p.text(frame_rect.left_center(), Align2::LEFT_CENTER, state.frame.to_string(), mono(look::SCRUB_LABEL_SIZE), label);
    let slider_rect = Rect::from_min_max(
        pos2(time_rect.right() + look::SCRUB_SPACING, inner.top()),
        pos2((frame_rect.left() - look::SCRUB_SPACING).max(time_rect.right() + look::SCRUB_SPACING), inner.bottom()),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(slider_rect).layout(Layout::left_to_right(Align::Center)));
    let (response, value) =
        slider(&mut child, slider_rect.width(), slider_rect.height(), (shown / duration) as f32, look::SCRUB_TRACK, look::SCRUB_KNOB, look::ACTIVE_BLUE);
    if (response.drag_started() || response.is_pointer_button_down_on()) && !drag.dragging() {
        actions.push(ScrubAction::Begin);
        drag.value = Some(shown);
    }
    if let Some(v) = value {
        let t = f64::from(v) * duration;
        if drag.value != Some(t) || actions.contains(&ScrubAction::Begin) {
            drag.value = Some(t);
            actions.push(ScrubAction::SeekTo(t));
        }
    }
    if drag.dragging() && !response.is_pointer_button_down_on() && !response.dragged() {
        drag.value = None;
        actions.push(ScrubAction::End);
    }
    actions
}

// ---------------------------------------------------------------------------------------------
// The "not supported" overlay (PlayerScreen.swift:20-35)

/// Centre `message` (headline) and Glance's caption over `rect`, white, not clickable.
pub fn not_supported(ui: &Ui, rect: Rect, message: &str, caption: &str) {
    let p = ui.painter_at(rect);
    let width = (rect.width() - 2.0 * look::MESSAGE_PADDING).max(1.0);
    let galley = |text: &str, font: FontId, color: Color32| ui.fonts_mut(|f| f.layout(text.to_string(), font, color, width));
    let head = galley(message, FontId::new(look::HEADLINE_SIZE, FontFamily::Name(crate::fonts::SEMIBOLD.into())), Color32::WHITE);
    let cap = galley(caption, font(look::CAPTION_SIZE), white(look::SECONDARY_WHITE));
    let total = head.size().y + look::MESSAGE_SPACING + cap.size().y;
    let top = rect.center().y - total / 2.0;
    let head_h = head.size().y;
    p.galley(pos2(rect.center().x - head.size().x / 2.0, top), head, Color32::WHITE);
    p.galley(pos2(rect.center().x - cap.size().x / 2.0, top + head_h + look::MESSAGE_SPACING), cap, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_is_swift_glances() {
        assert_eq!(fps_label(23.976), "24fps");
        assert_eq!(fps_label(29.97), "30fps");
        assert_eq!(fps_label(25.0), "25fps");
        assert_eq!(fps_label(12.5), "12.5fps");
        assert_eq!(duration_label(0.0), "0:00");
        assert_eq!(duration_label(59.6), "1:00");
        assert_eq!(duration_label(3725.0), "1:02:05");
        assert_eq!(info_line(1920, 1080, 29.97, 18.65), "1920×1080 · 30fps · 0:19");
        assert_eq!(title_suffix(Some("DXV3 HQ"), true), "[DXV3 HQ]  [LOOP]");
        assert_eq!(title_suffix(None, true), "[LOOP]");
        assert_eq!(title_suffix(Some("HAP"), false), "[HAP]");
        assert_eq!(window_title("Glance", None, ""), "Glance");
        assert_eq!(window_title("Glance", Some("a.mov"), "[HAP]  [LOOP]"), "a.mov  [HAP]  [LOOP]");
        assert_eq!(time_label(0.0), "0:00.000");
        assert_eq!(time_label(61.5), "1:01.500");
        assert_eq!(time_label(9.0004), "0:09.000");
        assert_eq!(time_label(f64::NAN), "0:00.000");
    }
}
