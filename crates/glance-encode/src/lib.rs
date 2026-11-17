//! # glance-encode — GlEnc's encode engine
//!
//! One job in, one file out, as Swift GlEnc 1.6.2's `GlEncCore` does it (`CoreEncoder`, `EncodePipeline`, the source
//! readers, `FrameCropper`, `FrameResizer`, `LetterboxFit`, `AudioSupport`): the source routed by its FourCC and read
//! frame by frame, cropped, resized, encoded and written, with Swift's alpha rules (straight colour in every output
//! that keeps alpha; a premultiplied DXV source un-premultiplied on read), its frame-rate and timescale rules, its
//! limits and its error words. The encoders and the QuickTime writer are DXVKit Plus's (`dxvkit-plus-enc`, byte-
//! identical to Swift GlEnc's); the platform's decoders, writer and scaler are DXVKit Plus's Apple island on macOS.
//!
//! Differences from Swift on purpose (glance-core `docs/02-PARITY.md`, Amrit 2026-11-14 "fix"): a trimmed job's audio
//! covers the trimmed range; HAP Q sources are decoded by the Vidvox rule (core decision C6).
#![forbid(unsafe_code)]

pub mod audio;
pub mod codec;
mod error;
pub mod frame;
pub mod pipeline;
mod platform;
pub mod size;
pub mod source;

/// DXVKit Plus's demuxer and decoders, for checking outputs.
pub use dxvkit_plus_core as media;
/// DXVKit Plus's encoders and QuickTime writer, for apps that drive them directly (tests, tools).
pub use dxvkit_plus_enc as enc;
pub use error::EncodeError;
pub use pipeline::{EncodeOutcome, EncodeRequest, encode};
