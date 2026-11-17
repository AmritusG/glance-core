// SPDX-License-Identifier: MIT
//! REST client for the endpoints the kit uses (Arena's swagger.yaml, served by its webserver).
//! Calls block; the store and the controller run them on their own threads.

use crate::decoder;
use crate::error::{ArenaError, ArenaStatus};
use crate::file_url::ArenaFileURL;
use crate::http::{self, Timeouts, TransportError};
use crate::settings::ArenaEndpoint;
use crate::snapshot::{CompositionSnapshot, SlotInfo};
use crate::swift_json;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

/// `GET /product`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ArenaProduct {
    pub name: String,
    pub major: i64,
    pub minor: i64,
    pub micro: i64,
    pub revision: i64,
}

impl ArenaProduct {
    /// "Arena 7.28.1 (26535)".
    pub fn version_string(&self) -> String {
        format!("{} {}.{}.{} ({})", self.name, self.major, self.minor, self.micro, self.revision)
    }
}

/// Answers "is the Arena process running on this computer?" (`None` = unknown, e.g. a remote
/// host). The host app supplies it; the kit has no process API.
pub type RunningProbe = Arc<dyn Fn() -> Option<bool> + Send + Sync>;

/// REST client. Cheap to clone.
#[derive(Clone)]
pub struct ArenaClient {
    pub endpoint: ArenaEndpoint,
    probe: RunningProbe,
    /// Per connect / read / write (URLSession's request timeout in SendEhrKit: 8 s).
    pub request_timeout: Duration,
    /// The whole exchange (URLSession's resource timeout: 60 s).
    pub resource_timeout: Duration,
}

impl std::fmt::Debug for ArenaClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArenaClient").field("endpoint", &self.endpoint).finish()
    }
}

impl ArenaClient {
    /// A client whose "is Arena running" answer is unknown.
    pub fn new(endpoint: ArenaEndpoint) -> Self {
        Self::with_probe(endpoint, Arc::new(|| None))
    }

    pub fn with_probe(endpoint: ArenaEndpoint, probe: RunningProbe) -> Self {
        ArenaClient { endpoint, probe, request_timeout: Duration::from_secs(8), resource_timeout: Duration::from_secs(60) }
    }

    pub fn is_arena_process_running(&self) -> Option<bool> {
        (self.probe)()
    }

    pub(crate) fn port_u16(&self) -> Option<u16> {
        u16::try_from(self.endpoint.port).ok().filter(|p| *p != 0)
    }

    fn request(&self, method: &str, path: &str, body: Option<(&[u8], &str)>) -> Result<Vec<u8>, ArenaError> {
        let Some(port) = self.port_u16() else {
            return Err(ArenaError::Unreachable { host: self.endpoint.host.clone(), port: self.endpoint.port, detail: "invalid port".to_string() });
        };
        let full = format!("/api/v1/{}", path.trim_start_matches('/'));
        let t = Timeouts { idle: self.request_timeout, total: self.resource_timeout };
        let host = self.endpoint.host.trim_start_matches('[').trim_end_matches(']');
        let r = http::request(host, &self.endpoint.host_for_url(), port, method, &full, body, &t).map_err(|e| self.map_transport(e))?;
        if !(200..300).contains(&r.status) {
            let n = r.body.len().min(300);
            return Err(ArenaError::Http { status: i64::from(r.status), path: path.to_string(), body: String::from_utf8_lossy(&r.body[..n]).into_owned() });
        }
        Ok(r.body)
    }

    pub(crate) fn map_transport(&self, e: TransportError) -> ArenaError {
        let host = self.endpoint.host.clone();
        let port = self.endpoint.port;
        match e {
            TransportError::Refused | TransportError::Lost => {
                if self.endpoint.is_local() {
                    if self.is_arena_process_running() == Some(false) { ArenaError::NotRunning } else { ArenaError::WebserverOff { host, port } }
                } else {
                    ArenaError::Unreachable { host, port, detail: "connection refused (webserver off or Arena closed)".to_string() }
                }
            }
            TransportError::TimedOut => ArenaError::Unreachable { host, port, detail: "timed out".to_string() },
            TransportError::HostNotFound => ArenaError::Unreachable { host, port, detail: "host not found".to_string() },
            TransportError::Other(detail) => ArenaError::Unreachable { host, port, detail },
        }
    }

    // MARK: reads

    /// `GET /product`.
    pub fn product(&self) -> Result<ArenaProduct, ArenaError> {
        let data = self.request("GET", "/product", None)?;
        serde_json::from_slice(&data).map_err(|e| ArenaError::BadResponse(format!("product: {e}")))
    }

    pub fn status(&self) -> ArenaStatus {
        match self.product() {
            Ok(p) => ArenaStatus::Connected { product: p.version_string() },
            Err(e) => ArenaStatus::Failed(e),
        }
    }

    /// `GET /composition` reduced to a snapshot. The body is read into one buffer of its final
    /// size, scanned in place and dropped.
    pub fn snapshot(&self) -> Result<CompositionSnapshot, ArenaError> {
        let data = self.request("GET", "/composition", None)?;
        decoder::decode(&data)
    }

    /// `GET /composition/layers/{L}/clips/{C}`: slot check.
    pub fn slot(&self, layer: usize, column: usize) -> Result<SlotInfo, ArenaError> {
        #[derive(Deserialize)]
        struct P {
            #[serde(default)]
            value: Option<String>,
        }
        #[derive(Deserialize)]
        struct C {
            id: i64,
            #[serde(default)]
            name: Option<P>,
            #[serde(default)]
            connected: Option<P>,
        }
        let data = self.request("GET", &format!("/composition/layers/{layer}/clips/{column}"), None)?;
        let c: C = serde_json::from_slice(&data).map_err(|e| ArenaError::BadResponse(format!("clip: {e}")))?;
        let state = c.connected.and_then(|p| p.value).unwrap_or_else(|| "Empty".to_string());
        let clip_name = if state == "Empty" { None } else { c.name.and_then(|p| p.value) };
        Ok(SlotInfo { column, clip_id: c.id, clip_name, state })
    }

    // MARK: writes

    /// `POST /composition/layers/add`, empty body = at the end / top.
    pub fn add_layer(&self) -> Result<(), ArenaError> {
        self.request("POST", "/composition/layers/add", None).map(|_| ())
    }

    /// `POST /composition/columns/add`, empty body = at the end.
    pub fn add_column(&self) -> Result<(), ArenaError> {
        self.request("POST", "/composition/columns/add", None).map(|_| ())
    }

    /// `POST /composition/layers/{L}/clips/{C}/open`, text/plain file URL.
    pub fn open(&self, layer: usize, column: usize, path: &str) -> Result<(), ArenaError> {
        let url = ArenaFileURL::encode(path);
        self.request("POST", &format!("/composition/layers/{layer}/clips/{column}/open"), Some((url.as_bytes(), "text/plain"))).map(|_| ())
    }

    /// `POST /composition/layers/{L}/clips/{C}/connect`, no body = short click.
    pub fn connect(&self, layer: usize, column: usize) -> Result<(), ArenaError> {
        self.request("POST", &format!("/composition/layers/{layer}/clips/{column}/connect"), None).map(|_| ())
    }

    /// `PUT /composition/layers/by-id/{id}` `{"name":{"value":...}}` (answers 204).
    pub fn rename_layer(&self, id: i64, name: &str) -> Result<(), ArenaError> {
        let body = swift_json::to_vec(&serde_json::json!({ "name": { "value": name } })).map_err(|e| ArenaError::BadResponse(e.to_string()))?;
        self.request("PUT", &format!("/composition/layers/by-id/{id}"), Some((&body, "application/json"))).map(|_| ())
    }

    /// `DELETE /composition/layers/by-id/{id}`.
    pub fn delete_layer(&self, id: i64) -> Result<(), ArenaError> {
        self.request("DELETE", &format!("/composition/layers/by-id/{id}"), None).map(|_| ())
    }

    /// `DELETE /composition/columns/by-id/{id}`.
    pub fn delete_column(&self, id: i64) -> Result<(), ArenaError> {
        self.request("DELETE", &format!("/composition/columns/by-id/{id}"), None).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_failures_map_to_the_kit_texts() {
        let remote = ArenaClient::new(ArenaEndpoint::new("10.0.0.5", 8080));
        for e in [TransportError::Refused, TransportError::Lost] {
            assert_eq!(remote.map_transport(e).plain_line(), "Arena not reachable at 10.0.0.5:8080 - connection refused (webserver off or Arena closed)");
        }
        assert_eq!(
            remote.map_transport(TransportError::Other("No route to host".into())).plain_line(),
            "Arena not reachable at 10.0.0.5:8080 - No route to host"
        );
        let local_running = ArenaClient::with_probe(ArenaEndpoint::default(), Arc::new(|| Some(true)));
        assert_eq!(local_running.map_transport(TransportError::Lost), ArenaError::WebserverOff { host: "localhost".into(), port: 8080 });
        let local_quit = ArenaClient::with_probe(ArenaEndpoint::default(), Arc::new(|| Some(false)));
        assert_eq!(local_quit.map_transport(TransportError::Lost), ArenaError::NotRunning);
        assert_eq!(local_quit.map_transport(TransportError::TimedOut).plain_line(), "Arena not reachable at localhost:8080 - timed out");
        assert_eq!(local_quit.map_transport(TransportError::HostNotFound).plain_line(), "Arena not reachable at localhost:8080 - host not found");
        let bad_port = ArenaClient::new(ArenaEndpoint::new("localhost", 70_000));
        assert_eq!(bad_port.product().unwrap_err().plain_line(), "Arena not reachable at localhost:70000 - invalid port");
    }
}
