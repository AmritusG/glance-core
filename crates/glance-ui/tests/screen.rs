//! What reaches the screen, through the real widget drawn offscreen (`headless::Stage`): at 1:1 the
//! exact picture; window-scaled (1.5x, 0.5x) within 1 of a software bilinear model; the
//! checkerboard in points, only under the picture. Needs a GPU; skips without one, unless
//! `GLANCE_REQUIRE_GPU=1`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use glance_gpu::{ClipView, Presenter, expected_picture};
use glance_media::{AlphaView, Decoded, PixelFormat, Plane, Shading, route};
use glance_ui::Background;
use glance_ui::headless::{Stage, distance, on_screen};

#[path = "../../../tests/support/movie.rs"]
mod support;
use support::{Kind, Scratch, fixtures};

struct Bench {
    stage: Stage,
    presenter: Presenter,
    view: ClipView,
}

fn bench() -> Option<Bench> {
    let g = glance_gpu::verify::test_gpu().unwrap()?;
    Some(Bench { stage: Stage::new(&g), presenter: Presenter::new(&g.device), view: ClipView::new(&g.device) })
}

impl Bench {
    /// `frame` read as `alpha` on a screen of `screen` pixels: (screen, the picture's rectangle).
    fn screen(&mut self, frame: &Decoded, alpha: AlphaView, screen: [u32; 2], ppp: f32, background: Background) -> (Vec<u8>, [f32; 4]) {
        let gpu = self.stage.gpu().clone();
        let picture = self.view.show(&gpu, &self.presenter, frame, alpha).unwrap().clone();
        self.stage.show(&picture, screen, ppp, background).unwrap()
    }
}

/// A frame of `kind` at `w` x `h` from random blocks, or frame `index` of a fixture.
fn random_frame(scratch: &Scratch, kind: Kind, w: u32, h: u32) -> Decoded {
    route(&scratch.file(&format!("{kind:?}_{w}x{h}.mov"), &kind.movie(w, h, 1))).unwrap().open().unwrap().decode(0).unwrap()
}

fn fixture_frame(name: &str, index: u32) -> Decoded {
    route(&fixtures().join(name)).unwrap().open().unwrap().decode(index).unwrap()
}

/// The Alpha Type a variant is shown with here: HAP alpha is straight, DXV alpha both ways.
fn views(kind: Kind) -> &'static [AlphaView] {
    match kind {
        Kind::Dxt5 | Kind::Yg10 | Kind::LegacyDxt5 => &[AlphaView::Premultiplied, AlphaView::Straight],
        Kind::Hap5 | Kind::HapM | Kind::HapA | Kind::Hap7 => &[AlphaView::Straight],
        _ => &[AlphaView::Premultiplied],
    }
}

fn scaled(size: (u32, u32), scale: f32) -> [u32; 2] {
    [((size.0 as f32 * scale).round() as u32).max(1), ((size.1 as f32 * scale).round() as u32).max(1)]
}

/// At 1:1 over black the screen is the picture, colour for colour: the window adds nothing.
#[test]
fn at_one_to_one_the_screen_is_the_exact_picture() {
    let Some(mut b) = bench() else { return };
    let scratch = Scratch::new("screen-1to1");
    let mut checked = 0;
    for kind in Kind::ALL {
        for (w, h) in [(17, 33), (64, 36)] {
            let frame = random_frame(&scratch, kind, w, h);
            for &alpha in views(kind) {
                let picture = expected_picture(&frame, alpha).unwrap();
                for ppp in [1.0, 2.0] {
                    // The same pixels at either display scale: the clip fills the screen.
                    let (screen, rect) = b.screen(&frame, alpha, [w, h], ppp, Background::Black);
                    assert_eq!(rect, [0.0, 0.0, w as f32, h as f32], "{kind:?}");
                    let same = screen.as_chunks::<4>().0.iter().zip(picture.as_chunks::<4>().0).all(|(s, p)| s[..3] == p[..3]);
                    assert!(same, "{kind:?} {w}x{h} {alpha:?} at {ppp}x: the screen is not the picture");
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, (12 + 3) * 2 * 2);
    // Negative control: the next frame's picture is not this screen.
    let (a, c) = (fixture_frame("tiles_hapm.mov", 0), fixture_frame("tiles_hapm.mov", 5));
    let (screen, _) = b.screen(&a, AlphaView::Straight, [128, 72], 1.0, Background::Black);
    let other = expected_picture(&c, AlphaView::Straight).unwrap();
    assert!(screen.as_chunks::<4>().0.iter().zip(other.as_chunks::<4>().0).any(|(s, p)| s[..3] != p[..3]));
}

/// Window-scaled, the screen is within 1 of the software model: the picture sampled bilinearly
/// and blended over the background. Random blocks are the hardest content there is for a filter
/// (every texel differs from its neighbour by up to the full range).
#[test]
fn window_scaled_screens_are_within_1_of_the_bilinear_model() {
    let Some(mut b) = bench() else { return };
    let scratch = Scratch::new("screen-scaled");
    let mut rows = Vec::new();
    let mut check = |b: &mut Bench, what: String, frame: &Decoded, alpha: AlphaView, scale: f32, ppp: f32, background: Background| {
        let size = frame.size();
        let screen_size = scaled(size, scale);
        let (screen, rect) = b.screen(frame, alpha, screen_size, ppp, background);
        let model = on_screen(&expected_picture(frame, alpha).unwrap(), size, rect, screen_size, ppp, background);
        let (worst, beyond) = distance(&screen, &model, 1.0);
        rows.push((format!("{what} {alpha:?} x{scale} at {ppp}x {background:?}"), worst, beyond));
    };
    for kind in Kind::ALL {
        for (w, h) in [(17, 33), (64, 36), (100, 36)] {
            let frame = random_frame(&scratch, kind, w, h);
            for &alpha in views(kind) {
                for scale in [1.5, 0.5] {
                    check(&mut b, format!("{kind:?} {w}x{h}"), &frame, alpha, scale, 1.0, Background::Black);
                }
            }
        }
    }
    // Encoded footage, both backgrounds, both display scales; and the two delivery sizes.
    for (name, alpha) in [
        ("tiles_dxt5.mov", AlphaView::Premultiplied),
        ("tiles_dxt5.mov", AlphaView::Straight),
        ("tiles_yg10.mov", AlphaView::Straight),
        ("tiles_hapm.mov", AlphaView::Straight),
        ("tiles_hap1.mov", AlphaView::Premultiplied),
    ] {
        let frame = fixture_frame(name, 7);
        for scale in [1.5, 0.5] {
            for ppp in [1.0, 2.0] {
                for background in [Background::Black, Background::Checkerboard] {
                    check(&mut b, name.into(), &frame, alpha, scale, ppp, background);
                }
            }
        }
    }
    for (kind, w, h) in [(Kind::Hap5, 1920, 1080), (Kind::Dxt5, 3840, 2160)] {
        let frame = random_frame(&scratch, kind, w, h);
        for scale in [1.5, 0.5] {
            check(&mut b, format!("{kind:?} {w}x{h}"), &frame, AlphaView::Straight, scale, 2.0, Background::Checkerboard);
        }
    }
    let worst = rows.iter().map(|r| r.1).fold(0f32, f32::max);
    eprintln!("{} scaled screens, largest distance from the model {worst}", rows.len());
    let bad: Vec<&(String, f32, u64)> = rows.iter().filter(|r| r.1 > 1.0 || r.2 != 0).collect();
    assert!(bad.is_empty(), "{} of {} screens are more than 1 from the model: {bad:?}", bad.len(), rows.len());
    assert_eq!(rows.len(), 15 * 3 * 2 + 5 * 8 + 4);
    // Negative control: against the model of a picture one texel to the side, the same screen fails.
    let frame = fixture_frame("tiles_dxt5.mov", 7);
    let (screen, rect) = b.screen(&frame, AlphaView::Straight, [192, 108], 1.0, Background::Black);
    let shifted = [rect[0] + 1.5, rect[1], rect[2] + 1.5, rect[3]];
    let picture = expected_picture(&frame, AlphaView::Straight).unwrap();
    assert!(distance(&screen, &on_screen(&picture, (128, 72), rect, [192, 108], 1.0, Background::Black), 1.0).1 == 0);
    assert!(distance(&screen, &on_screen(&picture, (128, 72), shifted, [192, 108], 1.0, Background::Black), 1.0).1 > 100);
}

fn rgba_frame(w: u32, h: u32, rgba: [u8; 4]) -> Decoded {
    let data = rgba.repeat((w * h) as usize);
    Decoded::Planes {
        index: 0,
        width: w,
        height: h,
        shading: Shading::Rgba { opaque: false },
        planes: vec![Plane { format: PixelFormat::Rgba8, width: w, height: h, data }],
    }
}

/// The checkerboard: 8-point squares of grey 0.85 and 0.65 (217 and 166), in points — 16 pixels on
/// a 2x display — under the picture only, never in the letterbox; black is the other background;
/// an opaque clip looks the same over both.
#[test]
fn the_checkerboard_is_8_point_squares_only_under_the_picture() {
    let Some(mut b) = bench() else { return };
    let clear = rgba_frame(40, 24, [0, 0, 0, 0]);
    for ppp in [1.0f32, 2.0] {
        // A 40x24 clip on a screen 100 points wide and 30 high: fitted to 50x30 points, bars beside it.
        let screen = [(100.0 * ppp) as u32, (30.0 * ppp) as u32];
        let (pixels, rect) = b.screen(&clear, AlphaView::Premultiplied, screen, ppp, Background::Checkerboard);
        assert_eq!(rect, [0.0, 0.0, 50.0 * ppp, 30.0 * ppp]);
        let cell = (8.0 * ppp) as u32;
        let (mut light, mut dark) = (0, 0);
        for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
            let (x, y) = (i as u32 % screen[0], i as u32 / screen[0]);
            let want = if x as f32 >= rect[2] {
                0
            } else if (x / cell + y / cell).is_multiple_of(2) {
                217
            } else {
                166
            };
            assert_eq!(p[..3], [want; 3], "pixel {x},{y} at {ppp}x");
            light += u32::from(want == 217);
            dark += u32::from(want == 166);
        }
        assert!(light > 0 && dark > 0);
        // Black is the other background.
        let (pixels, _) = b.screen(&clear, AlphaView::Premultiplied, screen, ppp, Background::Black);
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[..3] == [0; 3]));
        // An opaque clip covers either background alike.
        let opaque = rgba_frame(40, 24, [10, 200, 90, 255]);
        let over_checker = b.screen(&opaque, AlphaView::Premultiplied, screen, ppp, Background::Checkerboard).0;
        assert_eq!(over_checker, b.screen(&opaque, AlphaView::Premultiplied, screen, ppp, Background::Black).0);
        assert_eq!(over_checker[..3], [10, 200, 90]);
        // Half-transparent premultiplied colour adds to what is left of the background.
        let half = rgba_frame(40, 24, [50, 50, 50, 128]);
        let (pixels, _) = b.screen(&half, AlphaView::Premultiplied, screen, ppp, Background::Checkerboard);
        let want = (50.0 + 217.0 * (1.0 - 128.0 / 255.0f32)).round() as u8;
        assert!(pixels[0].abs_diff(want) <= 1, "{} against {want}", pixels[0]);
    }
}
