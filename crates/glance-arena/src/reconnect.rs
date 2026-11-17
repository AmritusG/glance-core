// SPDX-License-Identifier: MIT
//! When the store tries the websocket again after it failed or closed.
//!
//! - Local Arena not running: no retries at all; the host observes Arena's launch and calls
//!   [`crate::CompositionStore::arena_launched`].
//! - Just launched: Arena's webserver answers ~10 s after launch, so retry every
//!   `startup_interval` for `startup_window` seconds.
//! - Otherwise (remote host, or local Arena running with its webserver off, or still quitting):
//!   exponential backoff 1, 2, 4 … seconds, capped at `cap`. A menu opened while disconnected
//!   also retries at once.

/// Seconds, as in SendEhrKit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReconnectPolicy {
    pub startup_window: f64,
    pub startup_interval: f64,
    pub cap: f64,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        ReconnectPolicy { startup_window: 60.0, startup_interval: 1.0, cap: 30.0 }
    }
}

impl ReconnectPolicy {
    /// - `local`: the endpoint is this computer.
    /// - `arena_running`: Arena's process is running on this computer (`None` = unknown).
    /// - `failures`: consecutive failed attempts, 1 for the first.
    /// - `since_launch`: seconds since Arena's launch was observed, `None` if not observed.
    ///
    /// Seconds until the next attempt; `None` = wait for Arena's launch.
    pub fn delay(&self, local: bool, arena_running: Option<bool>, failures: i64, since_launch: Option<f64>) -> Option<f64> {
        if local && arena_running == Some(false) {
            return None;
        }
        if let Some(s) = since_launch
            && s < self.startup_window
        {
            return Some(self.startup_interval);
        }
        let exponent = (failures - 1).clamp(0, 16);
        Some(2f64.powi(i32::try_from(exponent).unwrap_or(16)).min(self.cap))
    }
}
