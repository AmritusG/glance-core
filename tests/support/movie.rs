//! Shared test support (not shipped): writes minimal QuickTime movies — tracks of any sample-entry
//! FourCC holding the samples given — and frames of random blocks for every DXV and HAP variant,
//! so the tests can make a movie of any variant at any size without an encoder. Laid out from
//! Apple's QuickTime File Format documentation; the single-track layout follows DXVKit Plus's
//! `tests/support/qt_movie.rs` (MIT, Amrit Rosell). Included with `#[path]` by the crates' tests.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

pub fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend_from_slice(kind);
    v.extend_from_slice(body);
    v
}

fn full(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    atom(kind, &[&[0u8; 4][..], body].concat())
}

/// One video track to write: its sample-entry FourCC, size and samples.
pub struct Track<'a> {
    pub fourcc: &'a [u8; 4],
    pub width: u16,
    pub height: u16,
    pub samples: &'a [Vec<u8>],
}

/// A 30 fps QuickTime movie of `tracks` (each one chunk, in order, in one `mdat` before the `moov`).
pub fn movie(tracks: &[Track<'_>]) -> Vec<u8> {
    movie_laid_out(tracks, false)
}

/// [`movie`] with the `moov` before the `mdat` when `moov_first` (so a cut-off file keeps its
/// sample tables).
pub fn movie_laid_out(tracks: &[Track<'_>], moov_first: bool) -> Vec<u8> {
    let ftyp = atom(b"ftyp", b"qt  \0\0\0\0qt  ");
    let data: Vec<u8> = tracks.iter().flat_map(|t| t.samples.concat()).collect();
    let moov = |first_sample: u32| {
        let mut offset = first_sample;
        let mut moov = Vec::new();
        for t in tracks {
            // The video sample entry: 86 bytes (8 of header, 78 of visual fields).
            let mut v = vec![0u8; 78];
            v[6..8].copy_from_slice(&1u16.to_be_bytes());
            v[24..26].copy_from_slice(&t.width.to_be_bytes());
            v[26..28].copy_from_slice(&t.height.to_be_bytes());
            v[74..76].copy_from_slice(&24u16.to_be_bytes());
            v[76..78].copy_from_slice(&0xFFFFu16.to_be_bytes());
            let entry = [&86u32.to_be_bytes()[..], t.fourcc, &v].concat();
            let n = t.samples.len() as u32;
            let stsd = full(b"stsd", &[&1u32.to_be_bytes()[..], &entry].concat());
            let stts = full(b"stts", &[1u32.to_be_bytes(), n.to_be_bytes(), 1u32.to_be_bytes()].concat());
            let stsc = full(b"stsc", &[1u32.to_be_bytes(), 1u32.to_be_bytes(), n.to_be_bytes(), 1u32.to_be_bytes()].concat());
            let mut sizes = [0u32.to_be_bytes(), n.to_be_bytes()].concat();
            t.samples.iter().for_each(|s| sizes.extend_from_slice(&(s.len() as u32).to_be_bytes()));
            let stsz = full(b"stsz", &sizes);
            let stco = full(b"stco", &[1u32.to_be_bytes(), offset.to_be_bytes()].concat());
            let stbl = atom(b"stbl", &[stsd, stts, stsc, stsz, stco].concat());
            let hdlr = atom(b"hdlr", &[&[0u8; 8][..], b"vide", &[0u8; 12]].concat());
            let mut mdhd = vec![0u8; 24];
            mdhd[12..16].copy_from_slice(&30u32.to_be_bytes());
            mdhd[16..20].copy_from_slice(&n.to_be_bytes());
            moov.extend(atom(b"trak", &atom(b"mdia", &[atom(b"mdhd", &mdhd), hdlr, atom(b"minf", &stbl)].concat())));
            offset += t.samples.iter().map(|s| s.len() as u32).sum::<u32>();
        }
        atom(b"moov", &moov)
    };
    if moov_first {
        // The moov's size does not depend on the offsets inside it.
        let size = moov(0).len();
        [ftyp.clone(), moov((ftyp.len() + size + 8) as u32), atom(b"mdat", &data)].concat()
    } else {
        [ftyp.clone(), atom(b"mdat", &data), moov((ftyp.len() + 8) as u32)].concat()
    }
}

/// A one-track movie.
pub fn video_movie(fourcc: &[u8; 4], width: u32, height: u32, samples: &[Vec<u8>]) -> Vec<u8> {
    movie(&[Track { fourcc, width: width as u16, height: height as u16, samples }])
}

/// A HAP section: a 4-byte header of type `kind` and its bytes (uncompressed blocks, or sections).
pub fn hap_section(kind: u8, body: &[u8]) -> Vec<u8> {
    let l = (body.len() as u32).to_le_bytes();
    [&[l[0], l[1], l[2], kind][..], body].concat()
}

/// splitmix64.
pub fn rng(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The block formats a variant's planes are made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Bc1,
    Bc3,
    Bc4,
    Bc5,
    Bc7,
}

impl Block {
    pub fn bytes(self) -> usize {
        if matches!(self, Block::Bc1 | Block::Bc4) { 8 } else { 16 }
    }
}

/// Every DXV and HAP variant with a CPU decode (HAP HDR has none: its fixture is committed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dxt1,
    Dxt5,
    Ycg6,
    Yg10,
    LegacyDxt1,
    LegacyDxt5,
    Hap1,
    Hap5,
    HapY,
    HapM,
    HapA,
    Hap7,
}

impl Kind {
    pub const ALL: [Kind; 12] = [
        Kind::Dxt1,
        Kind::Dxt5,
        Kind::Ycg6,
        Kind::Yg10,
        Kind::LegacyDxt1,
        Kind::LegacyDxt5,
        Kind::Hap1,
        Kind::Hap5,
        Kind::HapY,
        Kind::HapM,
        Kind::HapA,
        Kind::Hap7,
    ];

    pub fn fourcc(self) -> &'static [u8; 4] {
        match self {
            Kind::Dxt1 | Kind::Dxt5 | Kind::Ycg6 | Kind::Yg10 => b"DXD3",
            Kind::LegacyDxt1 | Kind::LegacyDxt5 => b"DXDI",
            Kind::Hap1 => b"Hap1",
            Kind::Hap5 => b"Hap5",
            Kind::HapY => b"HapY",
            Kind::HapM => b"HapM",
            Kind::HapA => b"HapA",
            Kind::Hap7 => b"Hap7",
        }
    }

    pub fn is_dxv(self) -> bool {
        self.fourcc().starts_with(b"DXD")
    }

    /// Whether the variant's alpha is the user's Alpha Type choice (DXV with alpha).
    pub fn choosable_alpha(self) -> bool {
        matches!(self, Kind::Dxt5 | Kind::Yg10 | Kind::LegacyDxt5)
    }

    /// Each plane's block format and its size in blocks at `w` x `h`: DXV codes to 16 (the High
    /// Quality chroma plane at half size), HAP to whole blocks.
    pub fn planes(self, w: u32, h: u32) -> Vec<(Block, usize)> {
        let unit = if self.is_dxv() { 16 } else { 4 };
        let full = (w.div_ceil(unit) * unit / 4 * (h.div_ceil(unit) * unit / 4)) as usize;
        match self {
            Kind::Dxt1 | Kind::LegacyDxt1 | Kind::Hap1 => vec![(Block::Bc1, full)],
            Kind::Dxt5 | Kind::LegacyDxt5 | Kind::Hap5 | Kind::HapY => vec![(Block::Bc3, full)],
            Kind::Ycg6 => vec![(Block::Bc4, full), (Block::Bc5, full / 4)],
            Kind::Yg10 => vec![(Block::Bc5, full), (Block::Bc5, full / 4)],
            Kind::HapM => vec![(Block::Bc3, full), (Block::Bc4, full)],
            Kind::HapA => vec![(Block::Bc4, full)],
            Kind::Hap7 => vec![(Block::Bc7, full)],
        }
    }

    /// The planes of frame `f` at `w` x `h`: random blocks, deterministic.
    pub fn frame_planes(self, w: u32, h: u32, f: u32) -> Vec<Vec<u8>> {
        let mut state = (u64::from(w) << 40) ^ (u64::from(h) << 20) ^ (u64::from(f) << 8) ^ self as u64;
        self.planes(w, h).into_iter().map(|(block, n)| random_blocks(&mut state, n, block)).collect()
    }

    /// One stored packet holding `planes` (HAP: uncompressed sections; DXV3: the stored flag, the
    /// planes back to back; DXDI: the stored type bit).
    pub fn packet(self, planes: &[Vec<u8>]) -> Vec<u8> {
        let all = planes.concat();
        let dxv3 = |tag: &[u8; 4]| [&tag[..], &[4, 0, 1, 0], &(all.len() as u32).to_le_bytes(), &all].concat();
        let legacy = |kind: u32| [&((kind << 24) | all.len() as u32).to_le_bytes()[..], &all].concat();
        match self {
            Kind::Dxt1 => dxv3(b"1TXD"),
            Kind::Dxt5 => dxv3(b"5TXD"),
            Kind::Ycg6 => dxv3(b"6GCY"),
            Kind::Yg10 => dxv3(b"01GY"),
            Kind::LegacyDxt1 => legacy(0x80),
            Kind::LegacyDxt5 => legacy(0xC0),
            Kind::Hap1 => hap_section(0xAB, &all),
            Kind::Hap5 => hap_section(0xAE, &all),
            Kind::HapY => hap_section(0xAF, &all),
            Kind::HapA => hap_section(0xA1, &all),
            Kind::Hap7 => hap_section(0xAC, &all),
            Kind::HapM => hap_section(0x0D, &[hap_section(0xAF, &planes[0]), hap_section(0xA1, &planes[1])].concat()),
        }
    }

    /// A movie of `frames` frames of random blocks at `w` x `h`.
    pub fn movie(self, w: u32, h: u32, frames: u32) -> Vec<u8> {
        let packets: Vec<Vec<u8>> = (0..frames).map(|f| self.packet(&self.frame_planes(w, h, f))).collect();
        video_movie(self.fourcc(), w, h, &packets)
    }
}

/// `n` random blocks. Random bytes are valid blocks of every palette mode and index; the boundary
/// cases random data rarely hits are forced on some blocks: equal endpoints, the extreme
/// endpoints, all-zero and all-one blocks (BC7: never a reserved mode).
pub fn random_blocks(state: &mut u64, n: usize, block: Block) -> Vec<u8> {
    let bytes = block.bytes();
    let mut v = Vec::with_capacity(n * bytes + 8);
    while v.len() < n * bytes {
        v.extend_from_slice(&rng(state).to_le_bytes());
    }
    v.truncate(n * bytes);
    // Where a block's BC4 halves (two endpoint bytes) and its BC1 half (two 16-bit endpoints) start.
    let (alpha, colour): (&[usize], Option<usize>) = match block {
        Block::Bc1 => (&[], Some(0)),
        Block::Bc3 => (&[0], Some(8)),
        Block::Bc4 => (&[0], None),
        Block::Bc5 => (&[0, 8], None),
        Block::Bc7 => (&[], None),
    };
    for (i, b) in v.chunks_exact_mut(bytes).enumerate() {
        match i % 53 {
            3 => alpha.iter().for_each(|&a| b[a + 1] = b[a]),
            11 => {
                if let Some(c) = colour {
                    b.copy_within(c..c + 2, c + 2);
                }
            }
            19 => alpha.iter().for_each(|&a| (b[a], b[a + 1]) = (255, 0)),
            29 => alpha.iter().for_each(|&a| (b[a], b[a + 1]) = (0, 255)),
            37 => b.fill(0),
            43 => b.fill(0xFF),
            _ => {}
        }
        // BC7: a first byte of zero is a reserved mode, which the format leaves to the decoder (see
        // `reserved_bc7`). Every other bit pattern is a valid block of one of the eight modes.
        if block == Block::Bc7 && b[0] == 0 {
            b[0] = 1 << (i % 8);
        }
    }
    v
}

/// A HAP R frame of `w` x `h` whose BC7 blocks are all of a reserved mode (first byte zero). The
/// BC7 definition says such a block decodes to transparent black; DXVKit Plus's CPU decoder and
/// Apple's GPUs do that, but it is not something every GPU honours, and no encoder writes one.
pub fn reserved_bc7(w: u32, h: u32) -> Vec<u8> {
    let blocks = (w.div_ceil(4) * h.div_ceil(4)) as usize;
    let mut state = 7u64;
    let mut data: Vec<u8> = (0..blocks * 16).map(|_| rng(&mut state) as u8).collect();
    data.as_chunks_mut::<16>().0.iter_mut().for_each(|b| b[0] = 0);
    video_movie(b"Hap7", w, h, &[hap_section(0xAC, &data)])
}

/// The sizes of the sweep: one texel, sizes below one block, odd sizes, sizes that are not
/// multiples of 16 (DXV's coded size), 1080p and 2160p.
pub const SIZES: [(u32, u32); 9] = [(1, 1), (2, 2), (3, 3), (4, 4), (5, 7), (15, 9), (17, 33), (1920, 1080), (3840, 2160)];

/// Amrit's own test media (read-only, never copied into the repo), when this machine has it:
/// `GLANCE_TESTFILES`, else the folder the W0 proof used.
pub fn testfiles() -> Option<PathBuf> {
    let dir =
        std::env::var_os("GLANCE_TESTFILES").map_or_else(|| PathBuf::from("<testfiles>"), PathBuf::from);
    dir.is_dir().then_some(dir)
}

/// The committed fixtures (`tests/fixtures`, CC0 and MIT: `ATTRIBUTION.md`).
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// The committed DXV and HAP movies with a CPU decode: the tile movies, HAP R, and DXVKit's vector
/// streams that decode (the `glenc` and `synthetic` groups), in a fixed order.
pub fn block_fixtures() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = ["hap1", "hap1_chunks4", "hap5", "hapa", "hapy", "hapm", "hapm_chunks4", "dxt1", "dxt5", "ycg6", "yg10"]
        .iter()
        .map(|n| fixtures().join(format!("tiles_{n}.mov")))
        .collect();
    v.push(fixtures().join("synthetic_hap7.mov"));
    for group in ["glenc", "synthetic"] {
        v.extend(movies_in(&fixtures().join("dxvkit/streams").join(group)));
    }
    v
}

/// The `.mov` files of a folder, sorted.
pub fn movies_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> =
        std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().and_then(|x| x.to_str()) == Some("mov")).collect();
    files.sort();
    files
}

/// A scratch directory, removed when dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("glance-core-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    /// Write `bytes` as `name` and return its path.
    pub fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
