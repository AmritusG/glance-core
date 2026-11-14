//! HAP Q, two conventions, measured against each other.
//!
//! The Rust family decodes HAP Q as DXVKit Plus does, which is the Vidvox reference: the BC3
//! palette untruncated, chroma offset 128/255, `scale = B * 255/8 + 1`. Swift Glance deviates from
//! it (glance/Sources/GlanceCore/HAPHQDecoder.swift:329-343, glance/Sources/GlancePlayback/
//! DXVRenderer.swift:926-935 and 966-974): the BC3 palette truncated to 8 bits first, chroma
//! midpoint 127.5, in the integers `D = B + 8`, `R' = Y + 8 (R - G) / D`, `G' = Y + 4 (2G - 255) / D`,
//! `B' = Y - 8 (R + G - 255) / D`, rounded half up. [`swift_rgba`] is that formula over DXVKit Plus's
//! own truncating block decoders, so the two pictures of one frame can be compared texel by texel.

use glance_media::{Decoded, PixelFormat, Plane, Shading};

/// Swift Glance's `hapYCoCgToRGB`: scaled YCoCg (Co, Cg, scale, Y as 8-bit values) to RGB.
pub fn swift_ycocg(co: u8, cg: u8, scale: u8, y: u8) -> [u8; 3] {
    let (r, g, d) = (i32::from(co), i32::from(cg), i32::from(scale) + 8);
    let yd = i32::from(y) * d;
    let round = |v: i32| if v <= 0 { 0 } else { ((2 * v + d) / (2 * d)).min(255) as u8 };
    [round(yd + 8 * (r - g)), round(yd + 4 * (2 * g - 255)), round(yd - 8 * (r + g - 255))]
}

fn block(plane: &Plane, index: usize) -> &[u8] {
    let n = plane.format.unit_bytes() as usize;
    plane.data.get(index * n..(index + 1) * n).unwrap_or_default()
}

/// A HAP Q or HAP Q Alpha frame as Swift Glance's CPU decoder gives it: straight RGBA8 at the
/// display size. `None` for any other frame.
pub fn swift_rgba(frame: &Decoded) -> Option<Vec<u8>> {
    let Decoded::Planes { width, height, shading: Shading::HapYcocg { alpha }, planes, .. } = frame else { return None };
    let (w, h) = (*width as usize, *height as usize);
    let colour = planes.first().filter(|p| p.format == PixelFormat::Bc3)?;
    let alpha = if *alpha { Some(planes.get(1).filter(|p| p.format == PixelFormat::Bc4)?) } else { None };
    let stride = (colour.width as usize).div_ceil(4);
    let mut out = vec![0u8; w * h * 4];
    for by in 0..h.div_ceil(4) {
        for bx in 0..w.div_ceil(4) {
            let blk = block(colour, by * stride + bx);
            // DXVKit's truncating decoders: BC1 colour (always four-colour inside BC3) and BC4.
            let rgb = glance_media::bc::bc1_block(blk.get(8..).unwrap_or_default(), true);
            let luma = glance_media::bc::bc4_block(blk);
            let a = alpha.map(|p| glance_media::bc::bc4_block(block(p, by * stride + bx)));
            for i in 0..16 {
                let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                if x < w && y < h {
                    let [co, cg, scale, _] = rgb.get(i).copied().unwrap_or_default();
                    let [r, g, b] = swift_ycocg(co, cg, scale, luma.get(i).copied().unwrap_or_default());
                    let a = a.and_then(|a| a.get(i).copied()).unwrap_or(255);
                    if let Some(px) = out.get_mut((y * w + x) * 4..(y * w + x) * 4 + 4) {
                        px.copy_from_slice(&[r, g, b, a]);
                    }
                }
            }
        }
    }
    Some(out)
}

/// The Rust family's picture of a clip against Swift Glance's convention, over the frames compared.
#[derive(Debug, Default, serde::Serialize)]
pub struct Difference {
    pub file: String,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    /// Frames left out: they did not decode, or are not HAP Q inside (damaged or mislabelled test files).
    pub skipped: u32,
    pub texels: u64,
    /// The largest difference of any channel, in 8-bit code values.
    pub max: u8,
    /// Texels whose R, G or B differs, and their share of all texels in per cent.
    pub differing: u64,
    pub differing_percent: f64,
    /// Texels by their largest channel difference: 1, 2, 3, 4 and more.
    pub by_size: [u64; 4],
    /// Channels of differing texels where the Rust picture is the brighter / the darker one.
    pub brighter: u64,
    pub darker: u64,
    /// SHA-256 of frame 0 in Swift Glance's convention (to check this port against Swift's own decoder).
    pub swift_frame0_sha256: String,
}

/// Compare every `step`-th frame of a HAP Q clip.
pub fn measure(path: &std::path::Path, step: u32) -> Result<Difference, String> {
    use sha2::Digest;
    let clip = glance_media::route(path).map_err(|e| e.to_string())?;
    let mut source = clip.open().map_err(|e| e.to_string())?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut d = Difference { file: name, codec: clip.info.codec.clone(), width: clip.info.width, height: clip.info.height, ..Default::default() };
    for index in (0..clip.info.frames).step_by(step.max(1) as usize) {
        let Some((frame, swift)) = source.decode(index).ok().and_then(|f| swift_rgba(&f).map(|s| (f, s))) else {
            d.skipped += 1;
            continue;
        };
        let ours = frame.to_rgba().map_err(|e| e.to_string())?;
        if index == 0 {
            d.swift_frame0_sha256 = sha2::Sha256::digest(&swift).iter().map(|b| format!("{b:02x}")).collect();
        }
        for (o, s) in ours.as_chunks::<4>().0.iter().zip(swift.as_chunks::<4>().0) {
            let worst = o.iter().zip(s).take(3).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
            if worst > 0 {
                d.differing += 1;
                d.max = d.max.max(worst);
                d.by_size[usize::from(worst.min(4)) - 1] += 1;
                for (a, b) in o.iter().zip(s).take(3) {
                    d.brighter += u64::from(a > b);
                    d.darker += u64::from(a < b);
                }
            }
        }
        d.frames += 1;
        d.texels += (ours.len() / 4) as u64;
    }
    d.differing_percent = 100.0 * d.differing as f64 / d.texels.max(1) as f64;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;
    use std::path::Path;

    /// The port above is Swift Glance's decoder: every frame of the committed HAP Q fixtures hashes
    /// to the SHA-256 of Swift Glance's own CPU decode of it (`tests/data/swift-glance-hap-rgba.tsv`,
    /// made by GlanceCore's `HAPThumbnail.rgbaOfFrame`). And for the HAP variants that are not HAP Q,
    /// DXVKit Plus's decode itself is Swift Glance's, bit for bit (HAP Alpha-Only: Swift returns
    /// white at the stored alpha, DXVKit Plus the stored value as grey).
    #[test]
    fn the_swift_convention_here_is_swift_glances_decoder() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        let (mut q, mut plain, mut ours_differs) = (0, 0, 0);
        let mut open: Option<(String, Box<dyn glance_media::FrameSource>)> = None;
        for line in include_str!("../../../tests/data/swift-glance-hap-rgba.tsv").lines() {
            let col: Vec<&str> = line.split('\t').collect();
            let (name, index, swift) = (col[0], col[1].parse::<u32>().unwrap(), col[2]);
            if open.as_ref().is_none_or(|(n, _)| n != name) {
                open = Some((name.into(), glance_media::route(&fixtures.join(name)).unwrap().open().unwrap()));
            }
            let frame = open.as_mut().unwrap().1.decode(index).unwrap();
            let hash = |rgba: &[u8]| -> String { sha2::Sha256::digest(rgba).iter().map(|b| format!("{b:02x}")).collect() };
            let mut ours = frame.to_rgba().unwrap();
            if let Some(as_swift) = swift_rgba(&frame) {
                assert_eq!(hash(&as_swift), swift, "{name} frame {index}: the port of Swift's HAP Q convention");
                ours_differs += u32::from(hash(&ours) != swift);
                q += 1;
            } else {
                if name == "tiles_hapa.mov" {
                    ours.as_chunks_mut::<4>().0.iter_mut().for_each(|p| *p = [255, 255, 255, p[0]]);
                }
                assert_eq!(hash(&ours), swift, "{name} frame {index}: DXVKit Plus's decode is Swift Glance's");
                plain += 1;
            }
        }
        // 3 HAP Q movies and 3 others, 24 frames each. The control: on HAP Q the Rust family's own
        // picture is not Swift's (the two conventions do differ on these frames).
        assert_eq!((q, plain), (72, 72));
        assert!(ours_differs > 0, "the conventions should differ on some HAP Q frame");
    }

    #[test]
    fn swifts_formula_at_known_points() {
        // Mid chroma is 127.5 in Swift's convention: Co = Cg = 128 leans half a step.
        assert_eq!(swift_ycocg(128, 128, 0, 100), [100, 101, 99]);
        assert_eq!(swift_ycocg(255, 0, 0, 128), [255, 1, 128]);
        assert_eq!(swift_ycocg(0, 0, 255, 0), [0, 0, 8]);
    }
}
