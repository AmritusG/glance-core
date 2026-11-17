// SPDX-License-Identifier: MIT
//! Ring buffer of the last 500 events, for a log window's Copy All; optionally mirrored to a file.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

/// One event.
#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub date: SystemTime,
    pub text: String,
}

type Observer = Arc<dyn Fn(&LogEntry) + Send + Sync>;

#[derive(Default)]
struct Inner {
    entries: Vec<LogEntry>,
    observers: Vec<Observer>,
    file: Option<File>,
}

/// The event log. Cloning shares the same buffer.
#[derive(Clone, Default)]
pub struct EventLog {
    inner: Arc<Mutex<Inner>>,
}

impl std::fmt::Debug for EventLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventLog").field("entries", &self.lock().entries.len()).finish()
    }
}

impl EventLog {
    pub const CAPACITY: usize = 500;

    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn add(&self, text: impl Into<String>) {
        let e = LogEntry { date: SystemTime::now(), text: text.into() };
        let observers = {
            let mut g = self.lock();
            g.entries.push(e.clone());
            let n = g.entries.len();
            if n > Self::CAPACITY {
                g.entries.drain(..n - Self::CAPACITY);
            }
            if let Some(f) = g.file.as_mut() {
                let _ = f.write_all(format!("{}\n", Self::line(&e)).as_bytes());
            }
            g.observers.clone()
        };
        for o in observers {
            o(&e);
        }
    }

    /// Called on the adding thread after each append.
    pub fn observe(&self, f: impl Fn(&LogEntry) + Send + Sync + 'static) {
        self.lock().observers.push(Arc::new(f));
    }

    /// Also append every event to a plain text file (created with its folder if missing).
    pub fn mirror(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(f) = OpenOptions::new().create(true).append(true).open(path) {
            self.lock().file = Some(f);
        }
    }

    pub fn all(&self) -> Vec<LogEntry> {
        self.lock().entries.clone()
    }

    /// `HH:mm:ss.SSS  text`, local time (UTC when the time zone cannot be read).
    pub fn line(e: &LogEntry) -> String {
        let clock = jiff::Timestamp::try_from(e.date)
            .map(|t| {
                let tz = jiff::tz::TimeZone::try_system().unwrap_or(jiff::tz::TimeZone::UTC);
                let c = tz.to_datetime(t);
                format!("{:02}:{:02}:{:02}.{:03}", c.hour(), c.minute(), c.second(), c.millisecond())
            })
            .unwrap_or_else(|_| "--:--:--.---".to_string());
        format!("{clock}  {}", e.text)
    }

    pub fn text(&self) -> String {
        self.all().iter().map(Self::line).collect::<Vec<_>>().join("\n")
    }

    /// True when some entry's text satisfies `f` (tests, diagnostics).
    pub fn contains(&self, f: impl Fn(&str) -> bool) -> bool {
        self.lock().entries.iter().any(|e| f(&e.text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_and_line_format() {
        let log = EventLog::new();
        let seen = Arc::new(Mutex::new(0usize));
        let s2 = Arc::clone(&seen);
        log.observe(move |_| *s2.lock().unwrap_or_else(std::sync::PoisonError::into_inner) += 1);
        for i in 0..520 {
            log.add(format!("event {i}"));
        }
        let all = log.all();
        assert_eq!(all.len(), EventLog::CAPACITY);
        assert_eq!(all[0].text, "event 20");
        assert_eq!(*seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner), 520);
        let line = EventLog::line(&all[0]);
        let (clock, text) = line.split_once("  ").unwrap_or_default();
        assert_eq!(text, "event 20");
        let b = clock.as_bytes();
        assert_eq!(b.len(), 12, "{clock}");
        assert!(b[2] == b':' && b[5] == b':' && b[8] == b'.');
        assert_eq!(log.text().lines().count(), 500);
    }
}
