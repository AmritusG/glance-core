//! A date in the shared store (`recents.json`, `cache.json`: Swift `Date`s, written as Swift's encoder writes them)
//! reads back as the very double that was written, bit for bit, and re-saves byte-identical. serde_json's default
//! number parser returns about one full-precision double in ten one unit in the last place off (TestKard,
//! TESTKARD-025); the workspace turns on its `float_roundtrip` feature. With the feature off this test finds
//! thousands off.

use glance_arena::{ArenaStatus, CacheState, Destination, RecentTarget, SharedStore, Timestamp};

/// splitmix64: a fixed seed, the same 200,000 dates on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Seconds since 1970 between 2001 and 2063, a full 53-bit mantissa (what `Date()` gives).
    fn date(&mut self) -> Timestamp {
        Timestamp(978_307_200.0 + (self.next() >> 11) as f64 / (1u64 << 53) as f64 * 2e9)
    }
}

#[test]
fn dates_read_back_bit_for_bit() {
    let dir = std::env::temp_dir().join(format!("glance-arena-float-roundtrip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = SharedStore::new(&dir);
    let mut rng = Rng(0x5EED_F10A_7000_0025);

    // The recents file, 200,000 entries long; then today's date as the app writes it.
    let mut recents: Vec<RecentTarget> =
        (0..200_000).map(|i| RecentTarget { destination: Destination::NextFree { layer_id: i }, label: format!("Layer {i}"), date: rng.date() }).collect();
    recents.push(RecentTarget::new(Destination::Slot { layer_id: 1, column: 2 }, "Layer 1"));
    store.write_recents(&recents).unwrap();
    let written = std::fs::read(store.recents_path()).unwrap();
    let back = store.read_recents();
    assert_eq!(back.len(), recents.len());
    // A Swift date is written relative to 1970 from a value stored relative to 2001: the file holds swift_encoded().
    let off = recents.iter().zip(&back).filter(|(w, g)| w.date.swift_encoded().to_bits() != g.date.0.to_bits()).count();
    assert_eq!(off, 0, "{off} of {} dates read back off", recents.len());
    store.write_recents(&back).unwrap();
    assert!(std::fs::read(store.recents_path()).unwrap() == written, "the re-saved recents differ");

    // The cache file's two dates.
    let cache = CacheState {
        status: ArenaStatus::Connected { product: "Arena 7.23.1".into() },
        snapshot: None,
        host_pid: 1,
        written_at: rng.date(),
        updated_at: Some(Timestamp::now()),
    };
    store.write_cache(&cache).unwrap();
    let got = store.read_cache().unwrap();
    assert_eq!(got.written_at.0.to_bits(), cache.written_at.swift_encoded().to_bits());
    assert_eq!(got.updated_at.unwrap().0.to_bits(), cache.updated_at.unwrap().swift_encoded().to_bits());
    let _ = std::fs::remove_dir_all(&dir);
}
