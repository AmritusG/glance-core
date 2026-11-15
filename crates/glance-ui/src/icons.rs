//! The transport bar's icons, drawn as vector shapes. Swift Glance uses SF Symbols
//! (`TransportBar.swift:31-96`), which are Apple's and only drawable through AppKit / SwiftUI on
//! Apple systems; these are the same pictures drawn by egui, the same on every system, sized to an
//! SF Symbol at 13 pt (a glyph about 13 pt tall at most, centred in the 28-pt button).

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

/// Which SF Symbol an icon stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// `backward.end.fill`
    PreviousClip,
    /// `play.fill`
    Play,
    /// `pause.fill`
    Pause,
    /// `forward.end.fill`
    NextClip,
    /// `arrow.counterclockwise`
    Restart,
    /// `gobackward.10`
    Back10,
    /// `goforward.10`
    Forward10,
    /// `backward.frame.fill`
    StepBack,
    /// `forward.frame.fill`
    StepForward,
    /// `repeat`
    Loop,
    /// `speaker.slash.fill`
    SpeakerMuted,
    /// `speaker.wave.1.fill` … `speaker.wave.3.fill`
    Speaker(u8),
    /// `arrow.up.left.and.arrow.down.right`
    Fullscreen,
}

impl Icon {
    /// The SF Symbol's name, for the look review.
    pub fn sf_symbol(self) -> &'static str {
        match self {
            Icon::PreviousClip => "backward.end.fill",
            Icon::Play => "play.fill",
            Icon::Pause => "pause.fill",
            Icon::NextClip => "forward.end.fill",
            Icon::Restart => "arrow.counterclockwise",
            Icon::Back10 => "gobackward.10",
            Icon::Forward10 => "goforward.10",
            Icon::StepBack => "backward.frame.fill",
            Icon::StepForward => "forward.frame.fill",
            Icon::Loop => "repeat",
            Icon::SpeakerMuted => "speaker.slash.fill",
            Icon::Speaker(1) => "speaker.wave.1.fill",
            Icon::Speaker(2) => "speaker.wave.2.fill",
            Icon::Speaker(_) => "speaker.wave.3.fill",
            Icon::Fullscreen => "arrow.up.left.and.arrow.down.right",
        }
    }

    /// Swift's speaker icon by mute and level (`TransportBar.muteIcon`): slashed when muted or at
    /// most 0.0001, else one wave below 0.33, two below 0.66, three above.
    pub fn speaker(muted: bool, volume: f32) -> Icon {
        if muted || volume <= 0.0001 {
            Icon::SpeakerMuted
        } else if volume < 0.33 {
            Icon::Speaker(1)
        } else if volume < 0.66 {
            Icon::Speaker(2)
        } else {
            Icon::Speaker(3)
        }
    }
}

/// A triangle pointing right (`dir` 1) or left (`dir` -1), `h` tall, its tip at `tip_x`.
fn triangle(p: &Painter, c: Pos2, tip_x: f32, w: f32, h: f32, dir: f32, color: Color32) {
    let base = tip_x - dir * w;
    p.add(Shape::convex_polygon(vec![pos2(base, c.y - h / 2.0), pos2(tip_x, c.y), pos2(base, c.y + h / 2.0)], color, Stroke::NONE));
}

fn bar(p: &Painter, x: f32, c: Pos2, w: f32, h: f32, color: Color32) {
    p.rect_filled(Rect::from_center_size(pos2(x, c.y), vec2(w, h)), w / 2.5, color);
}

/// Points on a circle around `c`, radius `r`, from angle `a0` to `a1` (radians, 0 = right, y down).
fn arc(c: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = 24;
    (0..=n).map(|i| a0 + (a1 - a0) * i as f32 / n as f32).map(|a| pos2(c.x + r * a.cos(), c.y + r * a.sin())).collect()
}

/// An arrowhead at `at` pointing along `dir` (unit), `s` long.
fn head(p: &Painter, at: Pos2, dir: egui::Vec2, s: f32, color: Color32) {
    let n = vec2(-dir.y, dir.x);
    p.add(Shape::convex_polygon(vec![at + dir * s * 0.6, at - dir * s * 0.4 + n * s * 0.55, at - dir * s * 0.4 - n * s * 0.55], color, Stroke::NONE));
}

/// Draw `icon` centred in `rect` in `color`; `u` is one point in pixels' worth of layout (1.0).
pub fn draw(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = 13.0 / 16.0; // a 16-unit design grid at 13 pt
    let stroke = Stroke::new(1.6 * s * 1.15, color);
    match icon {
        Icon::Play => triangle(p, c, c.x + 6.0 * s, 11.0 * s, 13.0 * s, 1.0, color),
        Icon::Pause => {
            bar(p, c.x - 3.0 * s, c, 3.6 * s, 12.5 * s, color);
            bar(p, c.x + 3.0 * s, c, 3.6 * s, 12.5 * s, color);
        }
        Icon::PreviousClip | Icon::NextClip => {
            let d = if icon == Icon::NextClip { 1.0 } else { -1.0 };
            triangle(p, c, c.x + d * 4.5 * s, 9.0 * s, 11.0 * s, d, color);
            bar(p, c.x + d * 6.0 * s, c, 2.2 * s, 11.0 * s, color);
        }
        Icon::StepBack | Icon::StepForward => {
            // A triangle beside a frame (a small rounded rectangle).
            let d = if icon == Icon::StepForward { 1.0 } else { -1.0 };
            triangle(p, c, c.x + d * 1.0 * s, 7.5 * s, 10.0 * s, d, color);
            let frame = Rect::from_center_size(pos2(c.x + d * 4.8 * s, c.y), vec2(4.6 * s, 10.0 * s));
            p.rect_stroke(frame, 1.2 * s, Stroke::new(1.4 * s, color), egui::StrokeKind::Inside);
        }
        Icon::Restart | Icon::Back10 | Icon::Forward10 => {
            let back = icon != Icon::Forward10;
            let r = 6.2 * s;
            let (a0, a1) = if back { (-1.35f32, 3.95f32) } else { (std::f32::consts::PI + 1.35, std::f32::consts::PI - 3.95) };
            let pts = arc(c, r, a0, a1);
            p.add(Shape::line(pts.clone(), stroke));
            if let Some(&start) = pts.first() {
                // The arrow at the top, pointing along the arc's direction at its start.
                let tangent = if back { vec2(-1.0, 0.0) } else { vec2(1.0, 0.0) };
                head(p, start + tangent * 0.8 * s, tangent, 5.0 * s, color);
            }
            if icon != Icon::Restart {
                let font = egui::FontId::new(6.4 * s, egui::FontFamily::Name(crate::fonts::SEMIBOLD.into()));
                p.text(pos2(c.x, c.y + 0.6 * s), egui::Align2::CENTER_CENTER, "10", font, color);
            }
        }
        Icon::Loop => {
            // Two arrows chasing round a rounded rectangle.
            let w = 6.5 * s;
            let h = 3.6 * s;
            p.add(Shape::line(vec![pos2(c.x - w, c.y + 0.5 * s), pos2(c.x - w, c.y - h), pos2(c.x + w - 1.5 * s, c.y - h)], stroke));
            head(p, pos2(c.x + w - 0.5 * s, c.y - h), vec2(1.0, 0.0), 4.6 * s, color);
            p.add(Shape::line(vec![pos2(c.x + w, c.y - 0.5 * s), pos2(c.x + w, c.y + h), pos2(c.x - w + 1.5 * s, c.y + h)], stroke));
            head(p, pos2(c.x - w + 0.5 * s, c.y + h), vec2(-1.0, 0.0), 4.6 * s, color);
        }
        Icon::SpeakerMuted | Icon::Speaker(_) => {
            let x0 = c.x - 7.0 * s;
            let body = vec![
                pos2(x0, c.y - 2.4 * s),
                pos2(x0 + 2.8 * s, c.y - 2.4 * s),
                pos2(x0 + 6.4 * s, c.y - 6.0 * s),
                pos2(x0 + 6.4 * s, c.y + 6.0 * s),
                pos2(x0 + 2.8 * s, c.y + 2.4 * s),
                pos2(x0, c.y + 2.4 * s),
            ];
            p.add(Shape::convex_polygon(body, color, Stroke::NONE));
            match icon {
                Icon::SpeakerMuted => {
                    p.line_segment([pos2(c.x - 8.0 * s, c.y - 7.0 * s), pos2(c.x + 7.5 * s, c.y + 7.0 * s)], Stroke::new(1.5 * s, color));
                }
                Icon::Speaker(n) => {
                    let o = pos2(x0 + 6.4 * s, c.y);
                    for k in 1..=n.min(3) {
                        let r = (2.6 + 2.6 * f32::from(k)) * s;
                        p.add(Shape::line(arc(o, r, -0.75, 0.75), Stroke::new(1.4 * s, color)));
                    }
                }
                _ => {}
            }
        }
        Icon::Fullscreen => {
            let d = 5.0 * s;
            let a = pos2(c.x - d, c.y - d);
            let b = pos2(c.x + d, c.y + d);
            p.line_segment([pos2(c.x - 1.5 * s, c.y - 1.5 * s), a + vec2(1.2, 1.2) * s], stroke);
            p.line_segment([pos2(c.x + 1.5 * s, c.y + 1.5 * s), b - vec2(1.2, 1.2) * s], stroke);
            p.add(Shape::convex_polygon(vec![a, a + vec2(4.5 * s, 0.0), a + vec2(0.0, 4.5 * s)], color, Stroke::NONE));
            p.add(Shape::convex_polygon(vec![b, b - vec2(4.5 * s, 0.0), b - vec2(0.0, 4.5 * s)], color, Stroke::NONE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_speaker_icon_follows_swifts_thresholds() {
        assert_eq!(Icon::speaker(true, 1.0), Icon::SpeakerMuted);
        assert_eq!(Icon::speaker(false, 0.0001), Icon::SpeakerMuted);
        assert_eq!(Icon::speaker(false, 0.00011), Icon::Speaker(1));
        assert_eq!(Icon::speaker(false, 0.329), Icon::Speaker(1));
        assert_eq!(Icon::speaker(false, 0.33), Icon::Speaker(2));
        assert_eq!(Icon::speaker(false, 0.659), Icon::Speaker(2));
        assert_eq!(Icon::speaker(false, 0.66), Icon::Speaker(3));
        assert_eq!(Icon::Speaker(3).sf_symbol(), "speaker.wave.3.fill");
    }
}
