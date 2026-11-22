//! A `real` in a [`FileStore`] reads back as the very double that was stored, bit for bit, and a reopened store
//! saves the same bytes. serde_json's default number parser returns about one full-precision double in ten one unit
//! in the last place off (TestKard, TESTKARD-025); the workspace turns on its `float_roundtrip` feature. With the
//! feature off this test finds thousands off.

use glance_settings::{FileStore, Store, Value};
use std::collections::BTreeMap;

/// splitmix64: a fixed seed, the same 200,000 doubles on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Half any finite bit pattern, half in [0, 1) with a full 53-bit mantissa (slider values, volumes).
    fn double(&mut self) -> f64 {
        loop {
            let bits = self.next();
            let d = if bits & 1 == 0 { f64::from_bits(bits) } else { (bits >> 11) as f64 / (1u64 << 53) as f64 };
            if d.is_finite() {
                return d;
            }
        }
    }
}

fn reals(v: &Value) -> Vec<f64> {
    match v {
        Value::Real(r) => vec![*r],
        Value::Array(a) => a.iter().flat_map(reals).collect(),
        Value::Dict(m) => m.values().flat_map(reals).collect(),
        _ => Vec::new(),
    }
}

#[test]
fn reals_read_back_bit_for_bit() {
    let dir = std::env::temp_dir().join(format!("glance-settings-float-roundtrip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("preferences.json");

    // What an app stores: volumes, a window frame, frame rates, computed fractions.
    let typical = Value::Dict(BTreeMap::from([
        ("volume".to_string(), Value::Real(0.7)),
        ("frame".to_string(), Value::Array([12.5, 87.25, 3838.0 / 3.0, 2159.0 / 3.0].map(Value::Real).to_vec())),
        ("rate".to_string(), Value::Real(30_000.0 / 1001.0)),
        ("third".to_string(), Value::Real(1.0 / 3.0)),
        ("sum".to_string(), Value::Real(0.1 + 0.2)),
        ("root".to_string(), Value::Real(2f64.sqrt())),
    ]));
    let mut rng = Rng(0x5EED_F10A_7000_0025);
    let random = Value::Array((0..200_000).map(|_| Value::Real(rng.double())).collect());

    let store = FileStore::open(&path);
    store.set("typical", typical.clone());
    store.set("random", random.clone());
    let written = std::fs::read(&path).unwrap();

    let back = FileStore::open(&path);
    let mut off = 0usize;
    for key in ["typical", "random"] {
        let want = reals(if key == "typical" { &typical } else { &random });
        let got = reals(&back.get(key).unwrap());
        assert_eq!(got.len(), want.len());
        off += want.iter().zip(&got).filter(|(w, g)| w.to_bits() != g.to_bits()).count();
    }
    assert_eq!(off, 0, "{off} of {} doubles read back off", 200_006);

    // A reopened store saves the same bytes (a removal of nothing does not save; re-set a value to force it).
    back.set("typical", back.get("typical").unwrap());
    assert!(std::fs::read(&path).unwrap() == written, "the re-saved file differs");
    let _ = std::fs::remove_dir_all(&dir);
}
