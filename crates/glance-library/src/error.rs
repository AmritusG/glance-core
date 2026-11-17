//! Every failure of the library code, as a value.

use std::fmt;

/// What a library operation could not do.
#[derive(Debug)]
pub enum LibraryError {
    /// SQLite refused (a constraint, a busy timeout, a damaged file…).
    Sqlite(rusqlite::Error),
    /// A file or directory could not be read, written, moved or removed.
    Io(std::io::Error),
    /// A number Foundation's JSON writer cannot write (NaN or an infinity).
    NonFiniteNumber(f64),
    /// A sidecar or other JSON document that Swift MaCrate's decoder would refuse.
    Decode(String),
    /// `PRAGMA foreign_keys` reads 0 after opening, so `ON UPDATE CASCADE` would silently not run (Swift MaCrate
    /// refuses to start then: `Crate.MetadataStore` error 1).
    ForeignKeysOff,
    /// `PRAGMA foreign_key_check` found violations at the end of a migration (GRDB rolls it back).
    ForeignKeyViolation { migration: &'static str, violations: usize },
    /// A saved-filter preset operation the UI reports by name (`MetadataStore.PresetError`).
    Preset(PresetError),
    /// The destination of a sidecar move already exists (`FileManager.moveItem` refuses).
    DestinationExists(std::path::PathBuf),
}

/// `MetadataStore.PresetError`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresetError {
    DuplicateName(String),
    NotFound(String),
    EmptyName,
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::Sqlite(e) => write!(f, "SQLite: {e}"),
            LibraryError::Io(e) => write!(f, "file: {e}"),
            LibraryError::NonFiniteNumber(v) => write!(f, "unable to encode {v} in JSON"),
            LibraryError::Decode(why) => write!(f, "cannot decode: {why}"),
            LibraryError::ForeignKeysOff => f.write_str("PRAGMA foreign_keys is OFF — v8 ON UPDATE CASCADE for clip_tags would silently fail."),
            LibraryError::ForeignKeyViolation { migration, violations } => {
                write!(f, "migration {migration} left {violations} foreign key violation(s)")
            }
            LibraryError::Preset(PresetError::DuplicateName(n)) => write!(f, "a preset named \"{n}\" already exists"),
            LibraryError::Preset(PresetError::NotFound(n)) => write!(f, "no preset named \"{n}\""),
            LibraryError::Preset(PresetError::EmptyName) => f.write_str("a preset needs a name"),
            LibraryError::DestinationExists(p) => write!(f, "{} already exists", p.display()),
        }
    }
}

impl std::error::Error for LibraryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LibraryError::Sqlite(e) => Some(e),
            LibraryError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for LibraryError {
    fn from(e: rusqlite::Error) -> Self {
        LibraryError::Sqlite(e)
    }
}

impl From<std::io::Error> for LibraryError {
    fn from(e: std::io::Error) -> Self {
        LibraryError::Io(e)
    }
}

impl From<crate::json::NonFiniteNumber> for LibraryError {
    fn from(e: crate::json::NonFiniteNumber) -> Self {
        LibraryError::NonFiniteNumber(e.0)
    }
}

impl From<PresetError> for LibraryError {
    fn from(e: PresetError) -> Self {
        LibraryError::Preset(e)
    }
}

/// The library's result type.
pub type Result<T> = std::result::Result<T, LibraryError>;
