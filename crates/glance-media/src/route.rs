//! Content routing, and what the viewer says when nothing plays a file.
//!
//! Swift Glance routes by what the file holds, never by its name (`PlayerModel.open`,
//! glance/Sources/Glance/PlayerModel.swift:209-229): DXV first, then HAP, then the OS decoder; a
//! file the OS decoder cannot open shows a message in the window instead of a black picture
//! (`AVPlaybackController.inspect`, glance/Sources/Glance/AVPlaybackController.swift:84-105).
//! [`route`] is that decision over DXVKit Plus's probe, and [`NotPlayable::message`] is that message,
//! word for word.
//!
//! The Rust family plays more than the Swift apps (Amrit, 2026-11-14: add, then prune): HAP R
//! (`Hap7`), HAP HDR (`HapH`) and DXVKit Plus's own LMBD route to their decoders here, where Swift
//! Glance shows "Hap7 in MOV is not supported".

use crate::{MediaError, MediaInfo, SourceKind, import_options};
use dxvkit_plus_core::container::{Codec, Movie};
use dxvkit_plus_core::source::{self, FrameSource};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Which engine plays a clip (Swift Glance's `ActivePath`, plus LMBD).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Route {
    /// `DXD3` / `DXD4` / `DXDI` sample entries; the packet decides the variant and the generation.
    Dxv,
    /// `Hap1` / `Hap5` / `HapY` / `HapM` / `HapA`, and `Hap7` / `HapH`.
    Hap,
    /// Everything else the OS decodes (H.264, HEVC, ProRes, Motion JPEG).
    Native,
    /// DXVKit Plus's LMBD (`.lmbd`).
    Lmbd,
}

/// A file that will not play: the line the viewer shows, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotPlayable {
    /// Swift Glance's wording: `<codec> in <CONTAINER> is not supported`, `<CONTAINER> files are
    /// not supported`, `This DXV file could not be opened` or `This HAP file could not be opened`.
    pub message: String,
    /// What failed underneath (for the log, not for the window).
    pub cause: MediaError,
}

impl std::fmt::Display for NotPlayable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for NotPlayable {}

/// A routed clip: what the probe learned and which engine plays it.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub info: MediaInfo,
    pub route: Route,
}

impl Clip {
    /// A frame-exact decoder for the clip. A frame's chunks decompress on worker threads.
    pub fn open(&self) -> Result<Box<dyn FrameSource>, NotPlayable> {
        source::open(&self.info, &import_options(), true).map_err(|cause| {
            let message = match self.route {
                Route::Dxv => "This DXV file could not be opened".to_string(),
                Route::Hap => "This HAP file could not be opened".to_string(),
                Route::Native | Route::Lmbd => unsupported_message(Path::new(&self.info.path)),
            };
            NotPlayable { message, cause }
        })
    }
}

/// Route `path` by its content. Every failure — a missing file, zero bytes, garbage, a truncated
/// movie, a codec nothing here decodes — is a [`NotPlayable`] carrying the line to show.
pub fn route(path: &Path) -> Result<Clip, NotPlayable> {
    let not = |cause: MediaError| NotPlayable { message: unsupported_message(path), cause };
    let Some(utf8) = path.to_str() else { return Err(not(MediaError::Invalid("the path is not UTF-8".into()))) };
    if path.is_dir() {
        return Err(not(MediaError::Unsupported("folders are not opened as clips".into())));
    }
    let info = source::probe(utf8, &import_options()).map_err(not)?;
    let route = match info.kind {
        SourceKind::Dxv => {
            // Swift Glance (DXVKit's demuxer) takes a DXV track only when its first packet states a
            // variant; otherwise the file falls through to the message.
            first_dxv_packet(path).map_err(not)?;
            Route::Dxv
        }
        SourceKind::Hap => Route::Hap,
        SourceKind::Native => Route::Native,
        SourceKind::Lmbd => Route::Lmbd,
        // Stills are Glance G1's to decide; Swift Glance does not open them.
        SourceKind::Still | SourceKind::ImageSequence => return Err(not(MediaError::Unsupported("stills are not opened as clips".into()))),
    };
    Ok(Clip { info, route })
}

/// The movie's DXV track with its first packet, when that packet's header is valid.
pub(crate) fn first_dxv_packet(path: &Path) -> Result<(Movie, Vec<u8>), MediaError> {
    let movie = Movie::open(path)?;
    let track = movie.track(Codec::Dxv).ok_or(MediaError::NoVideoTrack)?;
    let mut packet = Vec::new();
    movie.read_sample(track, 0, &mut packet)?;
    dxvkit_plus_core::dxv::parse_header(&packet).map_err(|e| MediaError::Dxv(e.to_string()))?;
    Ok((movie, packet))
}

/// Swift Glance's line for a file nothing plays: `<codec> in <CONTAINER> is not supported` when the
/// codec can be named (the first video track's sample-entry FourCC, or a Matroska `V_...` codec
/// id), else `<CONTAINER> files are not supported`. The container is the file name's extension in
/// upper case, `this container` without one.
pub fn unsupported_message(path: &Path) -> String {
    let extension = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let container = if extension.is_empty() { "this container".to_string() } else { extension.to_uppercase() };
    let codec = first_video_fourcc(path)
        .map(|cc| display_name(&cc).map_or_else(|| cc.trim_matches([' ', '\t']).to_string(), str::to_string))
        .or_else(|| matroska_codec(path, &extension));
    match codec {
        Some(codec) => format!("{codec} in {container} is not supported"),
        None => format!("{container} files are not supported"),
    }
}

/// The name Swift Glance shows for a sample-entry FourCC (`DXVDetector.displayName`,
/// glance/Sources/GlanceCore/DXVDetector.swift:28-57), plus the two HAP variants only the Rust
/// family plays. `None`: show the FourCC itself.
pub fn display_name(fourcc: &str) -> Option<&'static str> {
    Some(match fourcc {
        "DXD3" | "DXD4" => "DXV3",
        "DXDI" => "DXV1",
        "rle " => "QuickTime Animation",
        "Hap1" => "HAP",
        "Hap5" => "HAP Alpha",
        "HapY" => "HAP Q",
        "HapM" => "HAP Q Alpha",
        "HapA" => "HAP Alpha-only",
        "Hap7" => "HAP R",
        "HapH" => "HAP HDR",
        "jpeg" | "mjpa" | "mjpb" | "mjpg" => "Motion JPEG",
        "apch" => "ProRes 422 HQ",
        "apcn" => "ProRes 422",
        "apcs" => "ProRes 422 LT",
        "apco" => "ProRes 422 Proxy",
        "ap4h" => "ProRes 4444",
        "ap4x" => "ProRes 4444 XQ",
        "avc1" | "avc3" => "H.264",
        "hvc1" | "hev1" => "HEVC",
        "av01" => "AV1",
        "vp09" => "VP9",
        "mp4v" => "MPEG-4",
        _ => return None,
    })
}

/// One atom: where its payload starts and where the atom ends.
#[derive(Clone, Copy)]
struct Atom {
    kind: [u8; 4],
    body: u64,
    end: u64,
}

fn read_at<const N: usize>(file: &mut File, offset: u64) -> Option<[u8; N]> {
    let mut b = [0u8; N];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut b).ok()?;
    Some(b)
}

/// The atom header at `offset` inside a container ending at `limit`. An atom is at least 8 bytes,
/// so a walk always moves forward and ends.
fn atom_at(file: &mut File, offset: u64, limit: u64) -> Option<Atom> {
    if offset.checked_add(8)? > limit {
        return None;
    }
    let head: [u8; 8] = read_at(file, offset)?;
    let [s0, s1, s2, s3, k0, k1, k2, k3] = head;
    let (size, body) = match u32::from_be_bytes([s0, s1, s2, s3]) {
        1 => (u64::from_be_bytes(read_at(file, offset.checked_add(8)?)?), offset.checked_add(16)?),
        0 => (limit.checked_sub(offset)?, offset.checked_add(8)?),
        n => (u64::from(n), offset.checked_add(8)?),
    };
    let end = offset.checked_add(size)?;
    (size >= 8 && end <= limit).then_some(Atom { kind: [k0, k1, k2, k3], body, end })
}

/// The first child of `parent` of a kind; `None` when absent or when a malformed atom is met first.
fn child(file: &mut File, parent: Atom, kind: &[u8; 4]) -> Option<Atom> {
    let mut offset = parent.body;
    while offset < parent.end {
        let atom = atom_at(file, offset, parent.end)?;
        if &atom.kind == kind {
            return Some(atom);
        }
        offset = atom.end;
    }
    None
}

/// The sample-entry FourCC of the first video track (handler `vide`), read from the atom headers
/// alone: `moov > trak > mdia > hdlr`, then `minf > stbl > stsd`, first entry. It answers for files
/// whose sample tables are damaged or cut off, which is when the message needs it. The same walk as
/// Swift Glance's `DXVDetector.compressorFourCC` (glance/Sources/GlanceCore/DXVDetector.swift:13-174):
/// a FourCC with a byte outside ASCII counts as unreadable and the next track is tried.
pub fn first_video_fourcc(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let file = &mut file;
    let root = Atom { kind: *b"root", body: 0, end: size };
    let moov = child(file, root, b"moov")?;
    let mut offset = moov.body;
    while offset < moov.end {
        let atom = atom_at(file, offset, moov.end)?;
        if &atom.kind == b"trak"
            && let Some(cc) = track_video_fourcc(file, atom)
        {
            return Some(cc);
        }
        offset = atom.end;
    }
    None
}

fn track_video_fourcc(file: &mut File, trak: Atom) -> Option<String> {
    let mdia = child(file, trak, b"mdia")?;
    let hdlr = child(file, mdia, b"hdlr")?;
    // hdlr: version + flags (4), predefined (4), handler type (4).
    if &read_at::<4>(file, hdlr.body.checked_add(8)?)? != b"vide" {
        return None;
    }
    let minf = child(file, mdia, b"minf")?;
    let stbl = child(file, minf, b"stbl")?;
    let stsd = child(file, stbl, b"stsd")?;
    // stsd: version + flags (4), entry count (4), then the first entry: size (4), FourCC (4).
    let cc: [u8; 4] = read_at(file, stsd.body.checked_add(12)?)?;
    cc.is_ascii().then(|| cc.iter().map(|&b| char::from(b)).collect())
}

/// The video codec of a Matroska / WebM file, from the first `V_...` codec id in its first MiB
/// (`AVPlaybackController.matroskaCodec`, glance/Sources/Glance/AVPlaybackController.swift:110-123).
fn matroska_codec(path: &Path, extension: &str) -> Option<String> {
    if !["mkv", "webm", "mk3d"].contains(&extension.to_lowercase().as_str()) {
        return None;
    }
    let mut head = Vec::new();
    File::open(path).ok()?.take(1 << 20).read_to_end(&mut head).ok()?;
    let at = head.windows(2).position(|w| w == b"V_")?;
    let id: String = head
        .get(at..)?
        .iter()
        .take(32)
        .take_while(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || **b == b'_' || **b == b'/')
        .map(|&b| char::from(b))
        .collect();
    let name = match id.as_str() {
        "V_VP8" => "VP8",
        "V_VP9" => "VP9",
        "V_AV1" => "AV1",
        "V_MPEGH/ISO/HEVC" => "HEVC",
        "V_MPEG4/ISO/AVC" => "H.264",
        "V_PRORES" => "ProRes",
        "V_THEORA" => "Theora",
        "V_MPEG2" => "MPEG-2",
        "V_MS/VFW/FOURCC" => "VfW",
        other => return (other.len() > 2).then(|| other.get(2..).unwrap_or_default().to_string()),
    };
    Some(name.to_string())
}
