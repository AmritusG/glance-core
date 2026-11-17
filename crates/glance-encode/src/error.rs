use crate::source::SourceError;
use std::fmt;

/// Why an encode failed. The text is what the queue row and the CLI show (Swift: `String(describing: error)`).
#[derive(Debug)]
pub enum EncodeError {
    /// The source could not be opened or read (Swift's `EncodePipeline: <SourceReaderError>` family).
    Source(SourceError),
    /// A setting cannot be encoded (crop / size alignment, rate, chunks…); the text says which.
    Invalid(String),
    /// The encoder or the writer failed.
    Write(String),
    /// Cancelled by the user (the row shows "Cancelled").
    Cancelled,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::Source(SourceError::NoVideoTrack) => f.write_str("EncodePipeline: source has no video track"),
            EncodeError::Source(SourceError::ReaderStartFailed(e)) => write!(f, "EncodePipeline: reader.startReading() failed: {e}"),
            EncodeError::Source(SourceError::ReaderFailed(e)) => write!(f, "EncodePipeline: reader failed mid-stream: {e}"),
            EncodeError::Source(e) => write!(f, "EncodePipeline: {e}"),
            EncodeError::Invalid(s) | EncodeError::Write(s) => f.write_str(s),
            EncodeError::Cancelled => f.write_str("Cancelled"),
        }
    }
}

impl std::error::Error for EncodeError {}

impl From<dxvkit_plus_enc::EncodeError> for EncodeError {
    fn from(e: dxvkit_plus_enc::EncodeError) -> EncodeError {
        EncodeError::Write(e.to_string())
    }
}
