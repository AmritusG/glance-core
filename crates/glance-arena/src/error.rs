// SPDX-License-Identifier: MIT
//! Typed failures and the connection status, with the one-line texts the menus and the log show.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Typed failures. [`ArenaError::plain_line`] is the one-line, user-facing text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArenaError {
    NotRunning,
    WebserverOff { host: String, port: i64 },
    Unreachable { host: String, port: i64, detail: String },
    Http { status: i64, path: String, body: String },
    BadResponse(String),
    LayerGone { layer_id: i64 },
    NoFiles,
    Cancelled,
}

impl ArenaError {
    /// The one-line text the menus, dialogs and the log show.
    pub fn plain_line(&self) -> String {
        match self {
            ArenaError::NotRunning => "Arena is not running - start Resolume Arena.".to_string(),
            ArenaError::WebserverOff { host, port } => format!("Arena's webserver is off - Arena > Preferences > Webserver: enable, port {port} ({host})."),
            ArenaError::Unreachable { host, port, detail } => {
                format!("Arena not reachable at {host}:{port} - {detail}")
            }
            ArenaError::Http { status, path, body } => {
                let hint = match status {
                    404 => "not found".to_string(),
                    412 => "Arena refused (composition locked or clip busy)".to_string(),
                    400 => "bad request".to_string(),
                    other => format!("HTTP {other}"),
                };
                if body.is_empty() { format!("{hint}: {path}") } else { format!("{hint}: {path} - {body}") }
            }
            ArenaError::BadResponse(detail) => format!("Unexpected answer from Arena: {detail}"),
            ArenaError::LayerGone { layer_id } => {
                format!("That layer no longer exists in Arena (id {layer_id}) - pick again.")
            }
            ArenaError::NoFiles => "No files Arena can open in the selection.".to_string(),
            ArenaError::Cancelled => "Cancelled.".to_string(),
        }
    }

    /// The error as Swift's string interpolation prints the enum case (log lines).
    pub fn swift_description(&self) -> String {
        match self {
            ArenaError::NotRunning => "notRunning".to_string(),
            ArenaError::WebserverOff { host, port } => {
                format!("webserverOff(host: {host:?}, port: {port})")
            }
            ArenaError::Unreachable { host, port, detail } => {
                format!("unreachable(host: {host:?}, port: {port}, detail: {detail:?})")
            }
            ArenaError::Http { status, path, body } => {
                format!("http(status: {status}, path: {path:?}, body: {body:?})")
            }
            ArenaError::BadResponse(d) => format!("badResponse({d:?})"),
            ArenaError::LayerGone { layer_id } => format!("layerGone(layerID: {layer_id})"),
            ArenaError::NoFiles => "noFiles".to_string(),
            ArenaError::Cancelled => "cancelled".to_string(),
        }
    }
}

impl fmt::Display for ArenaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.plain_line())
    }
}

impl std::error::Error for ArenaError {}

/// SendEhrKit's coded form: `{"k": case, "host", "port", "text", "status", "id"}`, absent keys
/// omitted (the path of an HTTP error travels in `host`, its body in `text`).
#[derive(Serialize, Deserialize)]
struct ErrorBox {
    k: ErrorKind,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    port: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    status: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    id: Option<i64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ErrorKind {
    NotRunning,
    WebserverOff,
    Unreachable,
    Http,
    BadResponse,
    LayerGone,
    NoFiles,
    Cancelled,
}

impl ErrorBox {
    fn kind(k: ErrorKind) -> Self {
        ErrorBox { k, host: None, port: None, text: None, status: None, id: None }
    }
}

impl Serialize for ArenaError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let b = match self.clone() {
            ArenaError::NotRunning => ErrorBox::kind(ErrorKind::NotRunning),
            ArenaError::WebserverOff { host, port } => ErrorBox { host: Some(host), port: Some(port), ..ErrorBox::kind(ErrorKind::WebserverOff) },
            ArenaError::Unreachable { host, port, detail } => {
                ErrorBox { host: Some(host), port: Some(port), text: Some(detail), ..ErrorBox::kind(ErrorKind::Unreachable) }
            }
            ArenaError::Http { status, path, body } => ErrorBox { host: Some(path), text: Some(body), status: Some(status), ..ErrorBox::kind(ErrorKind::Http) },
            ArenaError::BadResponse(t) => ErrorBox { text: Some(t), ..ErrorBox::kind(ErrorKind::BadResponse) },
            ArenaError::LayerGone { layer_id } => ErrorBox { id: Some(layer_id), ..ErrorBox::kind(ErrorKind::LayerGone) },
            ArenaError::NoFiles => ErrorBox::kind(ErrorKind::NoFiles),
            ArenaError::Cancelled => ErrorBox::kind(ErrorKind::Cancelled),
        };
        b.serialize(s)
    }
}

impl<'de> Deserialize<'de> for ArenaError {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let b = ErrorBox::deserialize(d)?;
        Ok(match b.k {
            ErrorKind::NotRunning => ArenaError::NotRunning,
            ErrorKind::WebserverOff => ArenaError::WebserverOff { host: b.host.unwrap_or_default(), port: b.port.unwrap_or(0) },
            ErrorKind::Unreachable => {
                ArenaError::Unreachable { host: b.host.unwrap_or_default(), port: b.port.unwrap_or(0), detail: b.text.unwrap_or_default() }
            }
            ErrorKind::Http => ArenaError::Http { status: b.status.unwrap_or(0), path: b.host.unwrap_or_default(), body: b.text.unwrap_or_default() },
            ErrorKind::BadResponse => ArenaError::BadResponse(b.text.unwrap_or_default()),
            ErrorKind::LayerGone => ArenaError::LayerGone { layer_id: b.id.unwrap_or(0) },
            ErrorKind::NoFiles => ArenaError::NoFiles,
            ErrorKind::Cancelled => ArenaError::Cancelled,
        })
    }
}

/// Connection state as the menus show it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum ArenaStatus {
    Connected {
        product: String,
    },
    Failed(ArenaError),
    #[default]
    Unknown,
}

impl ArenaStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, ArenaStatus::Connected { .. })
    }

    pub fn plain_line(&self) -> String {
        match self {
            ArenaStatus::Connected { product } => format!("Connected to {product}"),
            ArenaStatus::Failed(e) => e.plain_line(),
            ArenaStatus::Unknown => "Checking Arena...".to_string(),
        }
    }
}

/// Swift's synthesised `Codable` shape for an enum with associated values.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum StatusRepr {
    Connected {
        product: String,
    },
    Failed {
        #[serde(rename = "_0")]
        error: ArenaError,
    },
    Unknown {},
}

impl Serialize for ArenaStatus {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let r = match self.clone() {
            ArenaStatus::Connected { product } => StatusRepr::Connected { product },
            ArenaStatus::Failed(error) => StatusRepr::Failed { error },
            ArenaStatus::Unknown => StatusRepr::Unknown {},
        };
        r.serialize(s)
    }
}

impl<'de> Deserialize<'de> for ArenaStatus {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match StatusRepr::deserialize(d)? {
            StatusRepr::Connected { product } => ArenaStatus::Connected { product },
            StatusRepr::Failed { error } => ArenaStatus::Failed(error),
            StatusRepr::Unknown {} => ArenaStatus::Unknown,
        })
    }
}
