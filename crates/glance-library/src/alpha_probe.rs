//! `clip_alpha.probe`: the cached Alpha Type probe as Swift MaCrate's `AlphaProbeCodec` strings it —
//! `none`, `choosable:<look>:<samples>:<share>` (`<look>` = `looksPremultiplied` / `looksStraight` / `cantTell`,
//! or `-`), `stated:<alpha type>:<stated by>` (the last part may hold `:`).

use crate::thumbkey::swift_double;
use glance_media::{AlphaLook, AlphaView};

/// A decoded probe (`AlphaTypeReport`'s stored part).
#[derive(Clone, Debug, PartialEq)]
pub enum AlphaProbe {
    /// Opaque, or no video alpha: no control.
    None,
    /// DXV with alpha: the user chooses; the hint and its counts.
    Choosable { look: Option<AlphaLook>, samples: i64, share: f64 },
    /// The format states it (HAP, ProRes 4444, HEVC with alpha).
    Stated { alpha: AlphaView, by: String },
}

/// `AlphaLook.rawValue`.
pub fn look_raw_value(look: AlphaLook) -> &'static str {
    match look {
        AlphaLook::LooksPremultiplied => "looksPremultiplied",
        AlphaLook::LooksStraight => "looksStraight",
        AlphaLook::CantTell => "cantTell",
    }
}

/// `AlphaLook(rawValue:)`.
pub fn look_from_raw_value(raw: &str) -> Option<AlphaLook> {
    match raw {
        "looksPremultiplied" => Some(AlphaLook::LooksPremultiplied),
        "looksStraight" => Some(AlphaLook::LooksStraight),
        "cantTell" => Some(AlphaLook::CantTell),
        _ => None,
    }
}

impl AlphaProbe {
    /// `AlphaProbeCodec.encode` (the share written as Swift's `"\(Double)"`).
    pub fn encode(&self) -> String {
        match self {
            AlphaProbe::None => "none".to_string(),
            AlphaProbe::Choosable { look, samples, share } => {
                format!("choosable:{}:{samples}:{}", look.map_or("-", look_raw_value), swift_double(*share))
            }
            AlphaProbe::Stated { alpha, by } => format!("stated:{}:{by}", alpha.stored_name()),
        }
    }

    /// `AlphaProbeCodec.decode`: split at `:` into at most four parts; `none` (whatever follows), `choosable` with
    /// exactly four parts (an unknown look reads as none, unparsable numbers as 0), `stated` with a known alpha
    /// type and at least three parts; anything else `None`.
    pub fn decode(s: &str) -> Option<AlphaProbe> {
        let f: Vec<&str> = s.splitn(4, ':').collect();
        match f.as_slice() {
            ["none", ..] => Some(AlphaProbe::None),
            ["choosable", look, samples, share] => {
                Some(AlphaProbe::Choosable { look: look_from_raw_value(look), samples: samples.parse().unwrap_or(0), share: share.parse().unwrap_or(0.0) })
            }
            ["stated", alpha, rest @ ..] if !rest.is_empty() => Some(AlphaProbe::Stated { alpha: AlphaView::from_stored_name(alpha)?, by: rest.join(":") }),
            _ => None,
        }
    }
}
