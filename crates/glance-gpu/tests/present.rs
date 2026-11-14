//! The present pass against hand-computed values. Needs a GPU (Metal on macOS, DX12 on Windows);
//! skips without one, unless `GLANCE_REQUIRE_GPU=1`.
// Test helpers outside `#[test]` functions may fail loudly too.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use glance_gpu::{ClipView, Presenter, read_texture_rgba};
use glance_media::{AlphaView, Decoded, PixelFormat, Plane, Shading};

const W: u32 = 32;
const H: u32 = 16;

/// A 32x16 RGBA8 frame, black and transparent except three probes.
fn frame() -> Decoded {
    let mut data = vec![0u8; (W * H * 4) as usize];
    let mut put = |x: u32, rgba: [u8; 4]| data[(x * 4) as usize..(x * 4 + 4) as usize].copy_from_slice(&rgba);
    put(0, [200, 100, 50, 128]); // half transparent
    put(1, [10, 20, 30, 0]); // colour under alpha 0 (a premultiplied glow)
    put(2, [9, 8, 7, 255]); // opaque
    Decoded::Planes {
        index: 0,
        width: W,
        height: H,
        shading: Shading::Rgba { opaque: false },
        planes: vec![Plane { format: PixelFormat::Rgba8, width: W, height: H, data }],
    }
}

fn shown(alpha: AlphaView) -> Option<impl Fn(u32) -> [u8; 4]> {
    let gpu = glance_gpu::verify::test_gpu().unwrap()?;
    let presenter = Presenter::new(&gpu.device);
    let mut view = ClipView::new(&gpu.device);
    let picture = view.show(&gpu, &presenter, &frame(), alpha).expect("show").clone();
    assert_eq!((picture.width(), picture.height()), (W, H));
    let rgba = read_texture_rgba(&gpu.device, &gpu.queue, &picture).expect("read back");
    Some(move |x: u32| rgba[(x * 4) as usize..(x * 4 + 4) as usize].try_into().unwrap())
}

#[test]
fn premultiplied_colour_goes_out_as_stored() {
    let Some(px) = shown(AlphaView::Premultiplied) else { return };
    assert_eq!(px(0), [200, 100, 50, 128]);
    assert_eq!(px(1), [10, 20, 30, 0]); // colour under alpha 0 survives: it adds to the background
    assert_eq!(px(2), [9, 8, 7, 255]);
}

#[test]
fn straight_colour_is_multiplied_by_its_alpha() {
    let Some(px) = shown(AlphaView::Straight) else { return };
    assert_eq!(px(0), [100, 50, 25, 128]); // round(200 * 128 / 255), round(100 * 128 / 255), round(50 * 128 / 255)
    assert_eq!(px(1), [0, 0, 0, 0]); // the control: the same texel that Premultiplied shows as 10 20 30
    assert_eq!(px(2), [9, 8, 7, 255]);
}

#[test]
fn the_alpha_switch_recomposites_without_a_new_frame() {
    let Some(gpu) = glance_gpu::verify::test_gpu().unwrap() else { return };
    let presenter = Presenter::new(&gpu.device);
    let mut view = ClipView::new(&gpu.device);
    assert!(view.relook(&gpu, &presenter, AlphaView::Premultiplied).is_err(), "nothing shown yet");
    view.show(&gpu, &presenter, &frame(), AlphaView::Premultiplied).expect("show");
    let picture = view.relook(&gpu, &presenter, AlphaView::Straight).expect("relook").clone();
    let rgba = read_texture_rgba(&gpu.device, &gpu.queue, &picture).expect("read back");
    assert_eq!(rgba[4..8], [0, 0, 0, 0]);
}
