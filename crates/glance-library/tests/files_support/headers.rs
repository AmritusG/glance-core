//! Small movies written byte by byte for the header cross-test (ours, generated; laid out from Apple's QuickTime
//! File Format documentation and ISO/IEC 14496-12): the box layouts MaCrate's `ContainerDurationReader` and
//! GlanceCore's DXV / HAP demuxers must agree on — `moov` first or last, 64-bit and to-the-end box sizes,
//! version 0 and 1 headers, turned and mirrored track matrices, variable frame durations, empty and short tables,
//! audio-only and two-track movies, broken boxes, zero time scales and sizes, DXV and HAP tracks with valid and
//! invalid packets, and files that are not QuickTime at all.
#![allow(dead_code)]

fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend_from_slice(kind);
    v.extend_from_slice(body);
    v
}

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// One track.
#[derive(Clone)]
pub struct Track {
    pub fourcc: [u8; 4],
    pub handler: [u8; 4],
    pub width: u16,
    pub height: u16,
    pub timescale: u32,
    pub duration: u64,
    pub mdhd_v1: bool,
    pub tkhd_v1: bool,
    /// a b u c d v x y w.
    pub matrix: [u32; 9],
    pub stts: Vec<(u32, u32)>,
    /// `stts` entry count written (may lie).
    pub stts_count: Option<u32>,
    pub samples: Vec<Vec<u8>>,
    pub minf: bool,
    pub hdlr: bool,
}

const IDENTITY: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];
const ROT90: [u32; 9] = [0, 0x0001_0000, 0, 0xFFFF_0000, 0, 0, 0, 0, 0x4000_0000];
const ROT270: [u32; 9] = [0, 0xFFFF_0000, 0, 0x0001_0000, 0, 0, 0, 0, 0x4000_0000];
const ROT180: [u32; 9] = [0xFFFF_0000, 0, 0, 0, 0xFFFF_0000, 0, 0, 0, 0x4000_0000];
const MIRROR: [u32; 9] = [0xFFFF_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

fn video(fourcc: &[u8; 4], w: u16, h: u16, frames: u32, packet: &[u8]) -> Track {
    Track {
        fourcc: *fourcc,
        handler: *b"vide",
        width: w,
        height: h,
        timescale: 30_000,
        duration: u64::from(frames) * 1001,
        mdhd_v1: false,
        tkhd_v1: false,
        matrix: IDENTITY,
        stts: vec![(frames, 1001)],
        stts_count: None,
        samples: (0..frames).map(|_| packet.to_vec()).collect(),
        minf: true,
        hdlr: true,
    }
}

fn audio(frames: u32) -> Track {
    Track {
        fourcc: *b"sowt",
        handler: *b"soun",
        timescale: 48_000,
        duration: u64::from(frames) * 1024,
        stts: vec![(frames, 1024)],
        ..video(b"sowt", 0, 0, frames, &[0; 4])
    }
}

/// A DXV3 DXT1 packet with a valid header (tag, version, stored flag, payload size) and raw blocks.
pub fn dxv_packet() -> Vec<u8> {
    let payload = vec![0x5Au8; 64];
    [&b"1TXD"[..], &[4, 0, 1, 0], &(payload.len() as u32).to_le_bytes(), &payload].concat()
}

/// A HAP section of uncompressed DXT1 blocks.
pub fn hap_packet() -> Vec<u8> {
    let body = vec![0xA5u8; 64];
    let l = (body.len() as u32).to_le_bytes();
    [&[l[0], l[1], l[2], 0xAB][..], &body].concat()
}

fn trak(t: &Track, first_offset: u32) -> Vec<u8> {
    let mut tkhd = Vec::new();
    if t.tkhd_v1 {
        tkhd.extend([1, 0, 0, 7]);
        tkhd.extend([0u8; 16]); // creation, modification
        tkhd.extend(be32(1)); // track id
        tkhd.extend([0u8; 4]);
        tkhd.extend(t.duration.to_be_bytes());
    } else {
        tkhd.extend([0, 0, 0, 7]);
        tkhd.extend([0u8; 8]);
        tkhd.extend(be32(1));
        tkhd.extend([0u8; 4]);
        tkhd.extend(be32(t.duration as u32));
    }
    tkhd.extend([0u8; 8]); // reserved
    tkhd.extend([0u8; 8]); // layer, alternate group, volume, reserved
    t.matrix.iter().for_each(|m| tkhd.extend(be32(*m)));
    tkhd.extend(be32(u32::from(t.width) << 16));
    tkhd.extend(be32(u32::from(t.height) << 16));
    let mut mdhd = Vec::new();
    if t.mdhd_v1 {
        mdhd.extend([1, 0, 0, 0]);
        mdhd.extend([0u8; 16]);
        mdhd.extend(be32(t.timescale));
        mdhd.extend(t.duration.to_be_bytes());
    } else {
        mdhd.extend([0u8; 12]);
        mdhd.extend(be32(t.timescale));
        mdhd.extend(be32(t.duration as u32));
    }
    mdhd.extend([0u8; 4]);
    let hdlr = atom(b"hdlr", &[&[0u8; 8][..], &t.handler, &[0u8; 12]].concat());
    let mut v = vec![0u8; 78];
    v[6..8].copy_from_slice(&1u16.to_be_bytes());
    v[24..26].copy_from_slice(&t.width.to_be_bytes());
    v[26..28].copy_from_slice(&t.height.to_be_bytes());
    v[74..76].copy_from_slice(&24u16.to_be_bytes());
    v[76..78].copy_from_slice(&0xFFFFu16.to_be_bytes());
    let entry = [&86u32.to_be_bytes()[..], &t.fourcc, &v].concat();
    let stsd = atom(b"stsd", &[&[0u8; 4][..], &be32(1), &entry].concat());
    let mut stts = vec![0u8; 4];
    stts.extend(be32(t.stts_count.unwrap_or(t.stts.len() as u32)));
    t.stts.iter().for_each(|(c, d)| {
        stts.extend(be32(*c));
        stts.extend(be32(*d));
    });
    let n = t.samples.len() as u32;
    let stsc = atom(b"stsc", &[&[0u8; 4][..], &be32(1), &be32(1), &be32(n.max(1)), &be32(1)].concat());
    let mut stsz = [&[0u8; 4][..], &be32(0), &be32(n)].concat();
    t.samples.iter().for_each(|s| stsz.extend(be32(s.len() as u32)));
    let stco = atom(b"stco", &[&[0u8; 4][..], &be32(1), &be32(first_offset)].concat());
    let stbl = atom(b"stbl", &[stsd, atom(b"stts", &stts), stsc, atom(b"stsz", &stsz), stco].concat());
    let mut mdia = atom(b"mdhd", &mdhd);
    if t.hdlr {
        mdia.extend(hdlr);
    }
    if t.minf {
        mdia.extend(atom(b"minf", &stbl));
    }
    atom(b"trak", &[atom(b"tkhd", &tkhd), atom(b"mdia", &mdia)].concat())
}

/// A movie: `mvhd` (version, time scale, duration), the tracks, laid out with the `moov` first or last.
pub struct Movie {
    pub mvhd: Option<(u8, u32, u64)>,
    pub tracks: Vec<Track>,
    pub moov_first: bool,
    /// The `mdat` written with a 64-bit size (`size == 1`).
    pub mdat_large: bool,
    /// The last top-level box written with `size == 0` (to the end of the file).
    pub last_size0: bool,
}

fn mvhd(version: u8, ts: u32, dur: u64) -> Vec<u8> {
    let mut b = vec![version, 0, 0, 0];
    if version == 1 {
        b.extend([0u8; 16]);
        b.extend(be32(ts));
        b.extend(dur.to_be_bytes());
    } else {
        b.extend([0u8; 8]);
        b.extend(be32(ts));
        b.extend(be32(dur as u32));
    }
    b.extend([0u8; 80]);
    atom(b"mvhd", &b)
}

impl Movie {
    fn of(tracks: Vec<Track>) -> Movie {
        Movie { mvhd: Some((0, 600, 2002)), tracks, moov_first: false, mdat_large: false, last_size0: false }
    }

    pub fn bytes(&self) -> Vec<u8> {
        let ftyp = atom(b"ftyp", b"qt  \0\0\0\0qt  ");
        let data: Vec<u8> = self.tracks.iter().flat_map(|t| t.samples.concat()).collect();
        let mdat = |large: bool| {
            if large { [&1u32.to_be_bytes()[..], b"mdat", &((data.len() + 16) as u64).to_be_bytes(), &data].concat() } else { atom(b"mdat", &data) }
        };
        let moov = |first: u32| {
            let mut body = self.mvhd.map(|(v, ts, d)| mvhd(v, ts, d)).unwrap_or_default();
            let mut offset = first;
            for t in &self.tracks {
                body.extend(trak(t, offset));
                offset += t.samples.iter().map(|s| s.len() as u32).sum::<u32>();
            }
            atom(b"moov", &body)
        };
        let mdat_header = if self.mdat_large { 16 } else { 8 };
        let mut out = if self.moov_first {
            let size = moov(0).len();
            [ftyp.clone(), moov((ftyp.len() + size + mdat_header) as u32), mdat(self.mdat_large)].concat()
        } else {
            [ftyp.clone(), mdat(self.mdat_large), moov((ftyp.len() + mdat_header) as u32)].concat()
        };
        if self.last_size0 {
            // Rewrite the last top-level box's size as 0.
            let last = if self.moov_first { ftyp.len() + moov(0).len() } else { out.len() - moov(0).len() };
            if let Some(s) = out.get_mut(last..last + 4) {
                s.copy_from_slice(&[0, 0, 0, 0]);
            }
        }
        out
    }
}

/// Every case: (file name, bytes).
pub fn cases() -> Vec<(String, Vec<u8>)> {
    let h264 = || video(b"avc1", 1920, 1080, 10, &[0u8; 16]);
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |name: &str, bytes: Vec<u8>| out.push((name.to_string(), bytes));
    add("moov-last.mov", Movie::of(vec![h264()]).bytes());
    add("moov-first.mov", Movie { moov_first: true, ..Movie::of(vec![h264()]) }.bytes());
    add("mdat-largesize.mov", Movie { mdat_large: true, ..Movie::of(vec![h264()]) }.bytes());
    add("moov-size0.mov", Movie { last_size0: true, ..Movie::of(vec![h264()]) }.bytes());
    add("mdat-size0.mp4", Movie { moov_first: true, last_size0: true, ..Movie::of(vec![h264()]) }.bytes());
    add("mvhd-v1.mov", Movie { mvhd: Some((1, 90_000, 5_000_000_000)), ..Movie::of(vec![h264()]) }.bytes());
    add("mvhd-v2.mov", Movie { mvhd: Some((2, 600, 600)), ..Movie::of(vec![h264()]) }.bytes());
    add("mvhd-ts0.mov", Movie { mvhd: Some((0, 0, 600)), ..Movie::of(vec![h264()]) }.bytes());
    add("mvhd-dur0.mov", Movie { mvhd: Some((0, 600, 0)), ..Movie::of(vec![h264()]) }.bytes());
    add("no-mvhd.mov", Movie { mvhd: None, ..Movie::of(vec![h264()]) }.bytes());
    for (name, m, v1) in
        [("rot90", ROT90, false), ("rot270", ROT270, false), ("rot180", ROT180, false), ("mirror", MIRROR, false), ("rot90-tkhd-v1", ROT90, true)]
    {
        add(&format!("{name}.mov"), Movie::of(vec![Track { matrix: m, tkhd_v1: v1, mdhd_v1: v1, ..video(b"avc1", 1080, 1920, 10, &[0u8; 16]) }]).bytes());
    }
    add("vfr.mov", Movie::of(vec![Track { stts: vec![(5, 1000), (5, 1001), (1, 500)], ..video(b"avc1", 640, 360, 11, &[0u8; 16]) }]).bytes());
    add("stts-empty.mov", Movie::of(vec![Track { stts: vec![], ..h264() }]).bytes());
    add("stts-count-lies.mov", Movie::of(vec![Track { stts_count: Some(1000), ..h264() }]).bytes());
    add("mdhd-ts0.mov", Movie::of(vec![Track { timescale: 0, ..h264() }]).bytes());
    add("mdhd-v1.mov", Movie::of(vec![Track { mdhd_v1: true, timescale: 24_000, ..h264() }]).bytes());
    add("width0.mov", Movie::of(vec![Track { width: 0, ..h264() }]).bytes());
    add("audio-only.mov", Movie::of(vec![audio(20)]).bytes());
    add("audio-then-video.mov", Movie::of(vec![audio(20), video(b"apcn", 1280, 720, 10, &[0u8; 16])]).bytes());
    add("first-video-broken.mov", Movie::of(vec![Track { minf: false, ..h264() }, video(b"avc1", 640, 360, 10, &[0u8; 16])]).bytes());
    add("no-hdlr-then-video.mov", Movie::of(vec![Track { hdlr: false, ..h264() }, video(b"avc1", 640, 360, 10, &[0u8; 16])]).bytes());
    add("two-video.mov", Movie::of(vec![video(b"avc1", 1920, 1080, 10, &[0u8; 16]), video(b"hvc1", 640, 360, 20, &[0u8; 16])]).bytes());
    let good = Movie::of(vec![h264()]).bytes();
    add("truncated.mov", good.get(..good.len() - 40).unwrap_or_default().to_vec());
    add("garbage-box.mov", [&[0u8, 0, 0, 3][..], b"junk", &good].concat());
    add("ftyp-only.mp4", atom(b"ftyp", b"isom\0\0\0\0isom"));
    add("empty.mov", Vec::new());
    add("random.mkv", (0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect());
    add("ebml.mkv", [&[0x1A, 0x45, 0xDF, 0xA3][..], &[0x9F; 60]].concat());
    add("riff.avi", [&b"RIFF"[..], &1000u32.to_le_bytes(), b"AVI LIST", &[0u8; 100]].concat());
    add("dxv.mov", Movie::of(vec![video(b"DXD3", 1284, 724, 12, &dxv_packet())]).bytes());
    add("dxv-moov-first.mov", Movie { moov_first: true, ..Movie::of(vec![video(b"DXD3", 1920, 1080, 25, &dxv_packet())]) }.bytes());
    add("dxv-literal-dxv3.mov", Movie::of(vec![video(b"DXV3", 1920, 1080, 12, &dxv_packet())]).bytes());
    add("dxv-bad-packet.mov", Movie::of(vec![video(b"DXD3", 1920, 1080, 12, &[0xFF; 32])]).bytes());
    add("dxv-ts0.mov", Movie::of(vec![Track { timescale: 0, ..video(b"DXD3", 640, 360, 12, &dxv_packet()) }]).bytes());
    add("dxv-vfr.mov", Movie::of(vec![Track { stts: vec![(6, 1000), (6, 1001)], duration: 12_006, ..video(b"DXD3", 640, 360, 12, &dxv_packet()) }]).bytes());
    // `moov` first, the file cut inside the samples: the table lists samples past the end.
    let dxv = Movie { moov_first: true, ..Movie::of(vec![video(b"DXD3", 640, 360, 12, &dxv_packet())]) }.bytes();
    add("dxv-truncated.mov", dxv.get(..dxv.len() - 100).unwrap_or_default().to_vec());
    let hap = Movie { moov_first: true, ..Movie::of(vec![video(b"Hap1", 640, 360, 12, &hap_packet())]) }.bytes();
    add("hap-truncated.mov", hap.get(..hap.len() - 100).unwrap_or_default().to_vec());
    add("hap.mov", Movie::of(vec![video(b"Hap1", 1080, 720, 12, &hap_packet())]).bytes());
    add("hap-vfr.mov", Movie::of(vec![Track { stts: vec![(4, 1001), (4, 2002), (4, 1001)], ..video(b"HapY", 640, 360, 12, &hap_packet()) }]).bytes());
    add("hap-stts-short.mov", Movie::of(vec![Track { stts: vec![(5, 1001)], ..video(b"Hap5", 640, 360, 12, &hap_packet()) }]).bytes());
    add("hap-stts-zero-tail.mov", Movie::of(vec![Track { stts: vec![(12, 500), (0, 7)], ..video(b"HapM", 640, 360, 12, &hap_packet()) }]).bytes());
    add("hap-ts0.mov", Movie::of(vec![Track { timescale: 0, ..video(b"Hap1", 640, 360, 12, &hap_packet()) }]).bytes());
    add("hap-width0.mov", Movie::of(vec![Track { width: 0, ..video(b"Hap1", 640, 360, 12, &hap_packet()) }]).bytes());
    add("hap-one-frame.mov", Movie::of(vec![video(b"HapA", 320, 240, 1, &hap_packet())]).bytes());
    add("hap-after-audio.mov", Movie::of(vec![audio(20), video(b"Hap1", 640, 360, 12, &hap_packet())]).bytes());
    add("hap7.mov", Movie::of(vec![video(b"Hap7", 640, 360, 12, &hap_packet())]).bytes());
    add("hap-rot90.mov", Movie::of(vec![Track { matrix: ROT90, ..video(b"Hap1", 640, 360, 12, &hap_packet()) }]).bytes());
    out
}

/// Write every case into `dir` (created).
pub fn write(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, bytes) in cases() {
        std::fs::write(dir.join(name), bytes)?;
    }
    Ok(())
}
