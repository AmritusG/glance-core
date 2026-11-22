//! Sidecars and stash files: every double (`created_at`, `updated_at`, `bpm`) reads back as the very double that was
//! written, bit for bit, and re-saves byte-identical. serde_json's default number parser returns about one
//! full-precision double in ten one unit in the last place off (TestKard, TESTKARD-025; 31 of the 251 real sidecars
//! of MaCrate's M7); the workspace turns on its `float_roundtrip` feature. With the feature off this test finds
//! thousands off.

use glance_library::sidecar::ClipMetadata;
use glance_library::stash::{Stash, StashEntry, decode_stash_file, encode_stash_file};

/// splitmix64: a fixed seed, the same values on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Seconds since 1970 between 2001 and 2063, a full 53-bit mantissa (what `Date()` gives).
    fn date(&mut self) -> f64 {
        978_307_200.0 + self.unit() * 2e9
    }
}

#[test]
fn sidecar_and_stash_doubles_read_back_bit_for_bit() {
    let mut rng = Rng(0x5EED_F10A_7000_0025);
    let mut off = 0usize;
    let mut doubles = 0usize;
    let mut resaved = 0usize;

    // A real sidecar's two timestamps (17 digits, as other tools write them; MaCrate tests/sidecar_numbers.rs), then
    // 66,666 random sidecars: 200,001 doubles in all.
    let real = ClipMetadata { bpm: Some(128.0), updated_at: 1_779_203_778.545_776_1, ..ClipMetadata::new("x:1", 1_778_925_862.203_755_9) };
    let mut sidecars = vec![real];
    for i in 0..66_666 {
        let mut m = ClipMetadata::new(format!("x:{i}"), rng.date());
        m.updated_at = rng.date();
        m.bpm = Some(40.0 + 200.0 * rng.unit());
        sidecars.push(m);
    }
    for m in &sidecars {
        let bytes = m.encode().unwrap();
        let back = ClipMetadata::decode(&bytes).unwrap();
        for (w, g) in [(m.created_at, back.created_at), (m.updated_at, back.updated_at), (m.bpm.unwrap(), back.bpm.unwrap())] {
            doubles += 1;
            off += usize::from(w.to_bits() != g.to_bits());
        }
        resaved += usize::from(back.encode().unwrap() != bytes);
    }

    // One stash file, 100,000 stashes: 200,000 doubles.
    let stashes: Vec<Stash> = (0..100_000)
        .map(|i| Stash {
            id: format!("A52E4F7E-0000-4000-8000-{i:012}"),
            name: format!("Set {i}"),
            created_at: rng.date(),
            updated_at: rng.date(),
            clips: vec![StashEntry { clip_id: format!("x:{i}"), relative_path_hint: format!("clips/{i}.mov") }],
        })
        .collect();
    let bytes = encode_stash_file(&stashes).unwrap();
    let back = decode_stash_file(&bytes).unwrap();
    assert_eq!(back.len(), stashes.len());
    for (w, g) in stashes.iter().zip(&back) {
        for (a, b) in [(w.created_at, g.created_at), (w.updated_at, g.updated_at)] {
            doubles += 1;
            off += usize::from(a.to_bits() != b.to_bits());
        }
    }
    resaved += usize::from(encode_stash_file(&back).unwrap() != bytes);

    assert_eq!((off, resaved), (0, 0), "{off} of {doubles} doubles read back off, {resaved} files re-saved differently");
}
