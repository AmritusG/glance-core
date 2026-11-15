//! Swift Glance's window rules (`glance/Sources/Glance/PlayerScreen.swift:92-173`,
//! `GlanceApp.swift:44,64`): default content size 960 × 616, minimum 500 × 180, and on every clip
//! open the width snaps to the clip's aspect at the current height.

use crate::look;

/// The content width for a clip of `video` pixels in a window whose content is `content_height`
/// points tall (`fitWindowWidth`): the video area is the content height less the chrome below the
/// picture (`bottom_chrome`, 36 + 40 in Glance) and less any title-bar overlap; the width is that
/// height times the clip's aspect, at least `min_width` (and never under the bar's 500), at most
/// [`look::MAX_SCREEN_SHARE`] of `screen_width` — the minimum applied first, then the maximum, as
/// Swift does. The height never changes. `None` for a clip with no size.
pub fn fit_width(content_height: f64, title_bar_overlap: f64, bottom_chrome: f64, video: (u32, u32), min_width: f64, screen_width: f64) -> Option<f64> {
    let (w, h) = video;
    if w == 0 || h == 0 {
        return None;
    }
    let area = (content_height - title_bar_overlap.max(0.0) - bottom_chrome).max(1.0);
    let mut width = area * f64::from(w) / f64::from(h);
    let min = min_width.max(look::MIN_SIZE.0);
    if width < min {
        width = min;
    }
    let max = screen_width * look::MAX_SCREEN_SHARE;
    if width > max {
        width = max;
    }
    Some(width)
}

/// The chrome under Glance's picture: the scrub row and the transport bar.
pub const GLANCE_BOTTOM_CHROME: f64 = look::SCRUB_HEIGHT as f64 + look::BAR_HEIGHT as f64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_width_snaps_to_the_clip_and_the_height_stays() {
        // Default window: 616 tall, the video area 540: a 16:9 clip gives exactly 960.
        assert_eq!(fit_width(616.0, 0.0, GLANCE_BOTTOM_CHROME, (1920, 1080), 500.0, 1728.0), Some(960.0));
        // SwiftUI's title bar overlapping by 28 pt (Swift's measured case): 512-pt area.
        assert_eq!(fit_width(616.0, 28.0, GLANCE_BOTTOM_CHROME, (1920, 1080), 500.0, 1728.0), Some(512.0 * 16.0 / 9.0));
        // A tall clip: never narrower than the bar.
        assert_eq!(fit_width(616.0, 0.0, GLANCE_BOTTOM_CHROME, (908, 2276), 500.0, 1728.0), Some(500.0));
        // A wide one: at most 95 % of the screen.
        assert_eq!(fit_width(616.0, 0.0, GLANCE_BOTTOM_CHROME, (3840, 540), 500.0, 1000.0), Some(950.0));
        // The minimum first, then the maximum (Swift's order): a tiny screen wins.
        assert_eq!(fit_width(616.0, 0.0, GLANCE_BOTTOM_CHROME, (10, 1000), 500.0, 400.0), Some(380.0));
        assert_eq!(fit_width(616.0, 0.0, GLANCE_BOTTOM_CHROME, (0, 1080), 500.0, 1728.0), None);
        // A window shorter than its chrome keeps a 1-pt area.
        assert_eq!(fit_width(60.0, 0.0, GLANCE_BOTTOM_CHROME, (1000, 1), 500.0, 1e9), Some(1000.0));
    }
}
