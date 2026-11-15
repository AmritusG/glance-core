//! The system font, as the Swift apps use it (`.font(.system(size:))`), read from the OS at run
//! time — egui's built-in fonts are not linked (no `default_fonts` feature), so no font file is
//! bundled with an app.
//!
//! - macOS: SF Pro (`/System/Library/Fonts/SFNS.ttf`, a variable font) and SF Mono
//!   (`SFNSMono.ttf`); semibold (SwiftUI's `.headline`) and medium are the same files at weight
//!   590 and 510 — the `wght` values Apple's system font uses for `.semibold` / `.medium`.
//!   Fallback for symbols the system font lacks: Apple Symbols.
//! - Windows: Segoe UI (`segoeui.ttf`, `seguisb.ttf` semibold) and Consolas (`consola.ttf`),
//!   from `%WINDIR%\Fonts`.
//!
//! Families: [`egui::FontFamily::Proportional`] (regular), [`egui::FontFamily::Monospace`],
//! [`SEMIBOLD`] and [`MEDIUM`].

use egui::FontFamily;
use egui::epaint::text::{FontData, FontDefinitions, FontTweak, VariationCoords};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The semibold family (SwiftUI `.headline`, 13 pt semibold on macOS).
pub const SEMIBOLD: &str = "system-semibold";
/// The medium family (the transport bar's icons are SF Symbols at `.medium`).
pub const MEDIUM: &str = "system-medium";

/// Where this system keeps its UI fonts: (regular, semibold, medium, monospace, fallbacks).
struct Files {
    regular: PathBuf,
    semibold: PathBuf,
    medium: PathBuf,
    mono: PathBuf,
    /// Whether the regular file is variable, weights then chosen by `wght`.
    variable: bool,
    fallbacks: Vec<PathBuf>,
}

fn files() -> Option<Files> {
    if cfg!(target_os = "macos") {
        let dir = Path::new("/System/Library/Fonts");
        let sf = dir.join("SFNS.ttf");
        Some(Files {
            regular: sf.clone(),
            semibold: sf.clone(),
            medium: sf,
            mono: dir.join("SFNSMono.ttf"),
            variable: true,
            fallbacks: vec![dir.join("Apple Symbols.ttf")],
        })
    } else if cfg!(windows) {
        let dir = PathBuf::from(std::env::var_os("WINDIR")?).join("Fonts");
        Some(Files {
            regular: dir.join("segoeui.ttf"),
            semibold: dir.join("seguisb.ttf"),
            medium: dir.join("seguisb.ttf"),
            mono: dir.join("consola.ttf"),
            variable: false,
            fallbacks: vec![dir.join("seguisym.ttf")],
        })
    } else {
        None
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("system font {}: {e}", path.display()))
}

fn at_weight(bytes: &Arc<[u8]>, weight: Option<f32>) -> FontData {
    let data = FontData::from_owned(bytes.to_vec());
    match weight {
        Some(w) => data.tweak(FontTweak { coords: VariationCoords::new([(b"wght", w)]), ..Default::default() }),
        None => data,
    }
}

/// The font definitions for this system: the system font as egui's proportional family, its
/// monospace face, and the two weights. `Err` names the file that is missing — an app shows it
/// instead of drawing with no font.
pub fn system_fonts() -> Result<FontDefinitions, String> {
    let f = files().ok_or("no system font known for this system")?;
    let regular: Arc<[u8]> = read(&f.regular)?.into();
    let mut defs = FontDefinitions { font_data: Default::default(), families: Default::default() };
    let (semibold, medium) = if f.variable {
        (at_weight(&regular, Some(590.0)), at_weight(&regular, Some(510.0)))
    } else {
        (FontData::from_owned(read(&f.semibold)?), FontData::from_owned(read(&f.medium)?))
    };
    defs.font_data.insert("system".into(), Arc::new(at_weight(&regular, f.variable.then_some(400.0))));
    defs.font_data.insert(SEMIBOLD.into(), Arc::new(semibold));
    defs.font_data.insert(MEDIUM.into(), Arc::new(medium));
    defs.font_data.insert("system-mono".into(), Arc::new(FontData::from_owned(read(&f.mono)?)));
    let mut fallbacks = Vec::new();
    for (i, path) in f.fallbacks.iter().enumerate() {
        // A missing fallback only costs the odd symbol.
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("fallback-{i}");
            defs.font_data.insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
            fallbacks.push(name);
        }
    }
    let family = |first: &str| std::iter::once(first.to_string()).chain(fallbacks.iter().cloned()).collect::<Vec<_>>();
    defs.families.insert(FontFamily::Proportional, family("system"));
    defs.families.insert(FontFamily::Monospace, std::iter::once("system-mono".to_string()).chain(family("system")).collect());
    defs.families.insert(FontFamily::Name(SEMIBOLD.into()), family(SEMIBOLD));
    defs.families.insert(FontFamily::Name(MEDIUM.into()), family(MEDIUM));
    Ok(defs)
}

/// Install the system fonts in `ctx`.
pub fn install(ctx: &egui::Context) -> Result<(), String> {
    ctx.set_fonts(system_fonts()?);
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn the_system_font_is_found_and_has_the_weights() {
        let defs = system_fonts().unwrap();
        for family in [FontFamily::Proportional, FontFamily::Monospace, FontFamily::Name(SEMIBOLD.into()), FontFamily::Name(MEDIUM.into())] {
            assert!(defs.families.get(&family).is_some_and(|f| !f.is_empty()), "{family:?}");
        }
        if cfg!(target_os = "macos") {
            // SF Pro is variable: the weights are coordinates on its `wght` axis, which must exist.
            let axes = defs.font_data["system"].variation_axes();
            eprintln!("SFNS.ttf axes: {axes:?}");
            assert!(axes.iter().any(|a| a.tag.to_be_bytes() == *b"wght" && a.range.min <= 400.0 && a.range.max >= 590.0), "{axes:?}");
        }
    }
}
