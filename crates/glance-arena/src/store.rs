// SPDX-License-Identifier: MIT
//! Keeps a [`CompositionSnapshot`] of the current deck. The mode (the "Arena Updates" setting)
//! picks how:
//! - Live: the websocket (default). Arena pushes the full composition on connect and after every
//!   structural change; layer and column name parameters are subscribed, because renames are only
//!   sent as `parameter_update`; a `GET /composition` poll every `poll_interval` (30 s) is the
//!   safety net for clip renames, only while the websocket is open and no push came lately;
//!   reconnects follow the [`ReconnectPolicy`] (while a local Arena is not running nothing retries
//!   until the host reports its launch with [`CompositionStore::arena_launched`]).
//! - On Demand: no websocket; one REST fetch per [`CompositionStore::wake`] (a menu opening, at
//!   most once per `menu_fetch_gap`, 2 s) or [`CompositionStore::refresh`].
//! - Interval: no websocket; a REST fetch every `interval` seconds plus on `wake`.
//!
//! [`CompositionStore::set_mode`] switches at once. The change callback ([`OnChange`]) gets
//! (snapshot, status, updated_at) only when they differ from the last delivery (`updated_at`
//! counts only outside Live, where menus show "Updated n s ago"): Arena pushes the full
//! composition on every clip trigger, and most pushes differ only in live values the snapshot
//! does not hold.
//!
//! The store runs on its own thread; every method only posts a message to it. The callback is
//! called on that thread: forward it to the UI thread (and never call
//! [`CompositionStore::activity`] from inside it).

use crate::client::ArenaClient;
use crate::decoder;
use crate::error::{ArenaError, ArenaStatus};
use crate::event_log::EventLog;
use crate::reconnect::ReconnectPolicy;
use crate::settings::{ArenaEndpoint, ArenaUpdateMode};
use crate::snapshot::CompositionSnapshot;
use crate::socket::{ArenaSocket, SocketEvent};
use crate::swift_json::Timestamp;
use std::collections::BTreeSet;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// (snapshot, status, updated_at), called on the store's thread.
pub type OnChange = Arc<dyn Fn(Option<CompositionSnapshot>, ArenaStatus, Option<Timestamp>) + Send + Sync>;

/// What is running right now (tests, diagnostics): websocket object, interval timer, 30 s poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Activity {
    pub socket: bool,
    pub interval_timer: bool,
    pub poll: bool,
}

/// Messages larger than this that are not a composition are skipped unread (sources / effects
/// lists); parameter updates are small.
const SMALL_MESSAGE: usize = 64 * 1024;

enum Msg {
    Start,
    Stop,
    SetMode(ArenaUpdateMode, Duration),
    SetClient(Box<ArenaClient>),
    ArenaLaunched,
    ArenaTerminated,
    Wake,
    Refresh(String),
    SetOnChange(Option<OnChange>),
    SetLog(Option<EventLog>),
    SetPollInterval(Duration),
    SetPolicy(ReconnectPolicy),
    SetMenuFetchGap(Duration),
    Activity(Sender<Activity>),
    SocketOpen(u64),
    SocketPush(u64, Box<CompositionSnapshot>),
    SocketRename(u64, i64, String),
    SocketLog(u64, String),
    SocketClosed(u64),
    StatusChecked { status: ArenaStatus, running: Option<bool>, local: bool },
    FetchDone(Result<Box<CompositionSnapshot>, ArenaError>, String),
    PublishStatus(ArenaStatus),
    Shutdown,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TimerKind {
    Retry,
    Poll,
    Interval,
    FetchRetry,
}

/// The values the handle reads without a round trip to the store thread.
#[derive(Clone, Default)]
struct Observed {
    snapshot: Option<CompositionSnapshot>,
    status: ArenaStatus,
    mode: ArenaUpdateMode,
    interval: Duration,
    updated_at: Option<Timestamp>,
    fetches: usize,
    attempts: usize,
    deliveries: usize,
    endpoint: ArenaEndpoint,
}

/// The composition store. Dropping the handle stops its thread.
pub struct CompositionStore {
    tx: Sender<Msg>,
    observed: Arc<Mutex<Observed>>,
}

impl std::fmt::Debug for CompositionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositionStore").finish_non_exhaustive()
    }
}

impl CompositionStore {
    /// A store in `mode`, not started. `interval` is Interval mode's period.
    pub fn new(client: ArenaClient, mode: ArenaUpdateMode, interval: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let observed = Arc::new(Mutex::new(Observed { mode, interval, endpoint: client.endpoint.clone(), ..Observed::default() }));
        let mut core = Core::new(client, mode, interval, tx.clone(), Arc::clone(&observed));
        let spawned = std::thread::Builder::new().name("arena-store".to_string()).spawn(move || core.run(rx));
        if spawned.is_err() {
            // No thread: the store stays idle (status Unknown); nothing to recover.
        }
        CompositionStore { tx, observed }
    }

    fn send(&self, m: Msg) {
        let _ = self.tx.send(m);
    }

    fn obs(&self) -> MutexGuard<'_, Observed> {
        self.observed.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn start(&self) {
        self.send(Msg::Start);
    }

    pub fn stop(&self) {
        self.send(Msg::Stop);
    }

    /// Switches the update mode now: Live opens the websocket; On Demand and Interval close it,
    /// fetch once, and Interval starts its timer.
    pub fn set_mode(&self, mode: ArenaUpdateMode, interval: Duration) {
        self.send(Msg::SetMode(mode, interval));
    }

    /// Switch host / port: drops the snapshot and reconnects.
    pub fn set_client(&self, client: ArenaClient) {
        self.send(Msg::SetClient(Box::new(client)));
    }

    /// The host saw Arena launch on this computer: connect now, then retry every
    /// `policy.startup_interval` until its webserver answers. Ignored for a remote endpoint.
    pub fn arena_launched(&self) {
        self.send(Msg::ArenaLaunched);
    }

    /// The host saw Arena quit on this computer: drop pending retries; the next failure (or this
    /// status check) finds it not running and waits.
    pub fn arena_terminated(&self) {
        self.send(Msg::ArenaTerminated);
    }

    /// A menu is opening. Live: if disconnected, try once now. On Demand / Interval: fetch, at
    /// most once per `menu_fetch_gap`.
    pub fn wake(&self) {
        self.send(Msg::Wake);
    }

    /// Fetch now, unthrottled (after our own loads, or Refresh). A call while a fetch is in
    /// flight is dropped.
    pub fn refresh(&self, reason: impl Into<String>) {
        self.send(Msg::Refresh(reason.into()));
    }

    pub fn set_on_change(&self, f: Option<OnChange>) {
        self.send(Msg::SetOnChange(f));
    }

    pub fn set_log(&self, log: Option<EventLog>) {
        self.send(Msg::SetLog(log));
    }

    /// Live mode's safety-net poll (default 30 s).
    pub fn set_poll_interval(&self, d: Duration) {
        self.send(Msg::SetPollInterval(d));
    }

    pub fn set_policy(&self, p: ReconnectPolicy) {
        self.send(Msg::SetPolicy(p));
    }

    /// `wake` fetches at most once per this (default 2 s; menus can open in bursts).
    pub fn set_menu_fetch_gap(&self, d: Duration) {
        self.send(Msg::SetMenuFetchGap(d));
    }

    pub fn snapshot(&self) -> Option<CompositionSnapshot> {
        self.obs().snapshot.clone()
    }

    pub fn status(&self) -> ArenaStatus {
        self.obs().status.clone()
    }

    pub fn mode(&self) -> ArenaUpdateMode {
        self.obs().mode
    }

    /// Interval mode's period.
    pub fn interval(&self) -> Duration {
        self.obs().interval
    }

    /// When the last composition arrived (push or fetch), changed or not.
    pub fn updated_at(&self) -> Option<Timestamp> {
        self.obs().updated_at
    }

    /// REST composition fetches started.
    pub fn fetches(&self) -> usize {
        self.obs().fetches
    }

    /// Websocket connect attempts.
    pub fn attempts(&self) -> usize {
        self.obs().attempts
    }

    /// Change-callback deliveries.
    pub fn deliveries(&self) -> usize {
        self.obs().deliveries
    }

    pub fn endpoint(&self) -> ArenaEndpoint {
        self.obs().endpoint.clone()
    }

    /// What runs right now, after every message sent before this call was handled.
    pub fn activity(&self) -> Activity {
        let (tx, rx) = mpsc::channel();
        self.send(Msg::Activity(tx));
        rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default()
    }
}

impl Drop for CompositionStore {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Shutdown);
    }
}

struct Delivered {
    snapshot: Option<CompositionSnapshot>,
    status: ArenaStatus,
    updated_at: Option<Timestamp>,
}

/// The state machine, owned by the store thread.
struct Core {
    client: ArenaClient,
    mode: ArenaUpdateMode,
    interval: Duration,
    snapshot: Option<CompositionSnapshot>,
    status: ArenaStatus,
    updated_at: Option<Timestamp>,
    on_change: Option<OnChange>,
    log: Option<EventLog>,
    poll_interval: Duration,
    policy: ReconnectPolicy,
    menu_fetch_gap: Duration,
    fetches: usize,
    attempts: usize,
    deliveries: usize,

    tx: Sender<Msg>,
    observed: Arc<Mutex<Observed>>,
    snapshot_dirty: bool,

    socket: Option<ArenaSocket>,
    socket_generation: u64,
    poll_timer: Option<u64>,
    interval_timer: Option<u64>,
    last_wake_fetch: Option<Instant>,
    fetch_until_up: bool,
    fetch_retry: Option<u64>,
    failures: i64,
    launched_at: Option<Instant>,
    retry: Option<u64>,
    waiting_for_launch: bool,
    subscribed: BTreeSet<i64>,
    refresh_pending: bool,
    running: bool,
    last_push: Option<Instant>,
    delivered: Option<Delivered>,
    timers: Vec<(Instant, TimerKind, u64)>,
    next_token: u64,
}

impl Core {
    fn new(client: ArenaClient, mode: ArenaUpdateMode, interval: Duration, tx: Sender<Msg>, observed: Arc<Mutex<Observed>>) -> Self {
        Core {
            client,
            mode,
            interval,
            snapshot: None,
            status: ArenaStatus::Unknown,
            updated_at: None,
            on_change: None,
            log: None,
            poll_interval: Duration::from_secs(30),
            policy: ReconnectPolicy::default(),
            menu_fetch_gap: Duration::from_secs(2),
            fetches: 0,
            attempts: 0,
            deliveries: 0,
            tx,
            observed,
            snapshot_dirty: false,
            socket: None,
            socket_generation: 0,
            poll_timer: None,
            interval_timer: None,
            last_wake_fetch: None,
            fetch_until_up: false,
            fetch_retry: None,
            failures: 0,
            launched_at: None,
            retry: None,
            waiting_for_launch: false,
            subscribed: BTreeSet::new(),
            refresh_pending: false,
            running: false,
            last_push: None,
            delivered: None,
            timers: Vec::new(),
            next_token: 0,
        }
    }

    fn log(&self, text: impl Into<String>) {
        if let Some(l) = &self.log {
            l.add(text);
        }
    }

    fn run(&mut self, rx: Receiver<Msg>) {
        loop {
            let now = Instant::now();
            let next = self.timers.iter().map(|t| t.0).min();
            let received = match next {
                Some(at) if at <= now => Err(RecvTimeoutError::Timeout),
                Some(at) => rx.recv_timeout(at - now),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match received {
                Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                    self.running = false;
                    self.end();
                    return;
                }
                Ok(m) => self.handle(m),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.fire_due_timers();
            self.sync_observed();
        }
    }

    fn sync_observed(&mut self) {
        let mut o = self.observed.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.snapshot_dirty {
            o.snapshot = self.snapshot.clone();
            self.snapshot_dirty = false;
        }
        o.status = self.status.clone();
        o.mode = self.mode;
        o.interval = self.interval;
        o.updated_at = self.updated_at;
        o.fetches = self.fetches;
        o.attempts = self.attempts;
        o.deliveries = self.deliveries;
        o.endpoint = self.client.endpoint.clone();
    }

    fn set_snapshot(&mut self, s: Option<CompositionSnapshot>) {
        self.snapshot = s;
        self.snapshot_dirty = true;
    }

    // MARK: timers

    fn schedule(&mut self, after: Duration, kind: TimerKind) -> u64 {
        self.next_token += 1;
        let token = self.next_token;
        self.timers.push((Instant::now() + after, kind, token));
        token
    }

    fn cancel_timer(&mut self, token: Option<u64>) {
        if let Some(t) = token {
            self.timers.retain(|(_, _, tok)| *tok != t);
        }
    }

    fn fire_due_timers(&mut self) {
        loop {
            let now = Instant::now();
            let Some(pos) = self.timers.iter().position(|t| t.0 <= now) else { return };
            let (_, kind, token) = self.timers.swap_remove(pos);
            match kind {
                TimerKind::Retry => {
                    if self.retry == Some(token) && self.running && self.socket.is_none() {
                        self.retry = None;
                        self.open_socket();
                    }
                }
                TimerKind::Poll => {
                    if self.poll_timer == Some(token) {
                        self.poll_timer = Some(self.schedule(self.poll_interval, TimerKind::Poll));
                        // Pushes already carry everything; poll only when none came lately
                        // (clip renames are not pushed).
                        let recent = self.last_push.is_some_and(|t| t.elapsed() < self.poll_interval);
                        if !recent {
                            self.schedule_refresh("poll");
                        }
                    }
                }
                TimerKind::Interval => {
                    if self.interval_timer == Some(token) {
                        self.interval_timer = Some(self.schedule(self.interval, TimerKind::Interval));
                        self.interval_tick();
                    }
                }
                TimerKind::FetchRetry => {
                    if self.fetch_retry == Some(token) && self.fetch_until_up {
                        self.fetch_retry = None;
                        self.schedule_refresh("Arena launched");
                    }
                }
            }
        }
    }

    fn interval_tick(&mut self) {
        if !self.running || self.mode != ArenaUpdateMode::Interval {
            return;
        }
        // While a local Arena is not running the tick publishes "not running" without touching
        // the network.
        if self.client.endpoint.is_local() && self.client.is_arena_process_running() == Some(false) {
            self.set_snapshot(None);
            self.publish(ArenaStatus::Failed(ArenaError::NotRunning));
            return;
        }
        self.schedule_refresh("interval");
    }

    // MARK: messages

    fn handle(&mut self, m: Msg) {
        match m {
            Msg::Start => {
                if !self.running {
                    self.running = true;
                    self.begin();
                }
            }
            Msg::Stop => {
                self.running = false;
                self.end();
            }
            Msg::SetMode(mode, interval) => self.set_mode(mode, interval),
            Msg::SetClient(client) => {
                self.client = *client;
                self.set_snapshot(None);
                self.publish(ArenaStatus::Unknown);
                self.end();
                self.failures = 0;
                self.launched_at = None;
                if self.running {
                    self.begin();
                }
            }
            Msg::ArenaLaunched => {
                if !self.running || !self.client.endpoint.is_local() {
                    return;
                }
                self.log("Arena launched - connecting");
                self.launched_at = Some(Instant::now());
                self.failures = 0;
                if self.mode == ArenaUpdateMode::Live {
                    self.connect_now();
                } else {
                    self.fetch_until_up = true;
                    self.schedule_refresh("Arena launched");
                }
            }
            Msg::ArenaTerminated => {
                if !self.running || !self.client.endpoint.is_local() {
                    return;
                }
                self.launched_at = None;
                self.fetch_until_up = false;
                let fr = self.fetch_retry.take();
                self.cancel_timer(fr);
                if self.socket.is_some() {
                    return; // the open socket reports its own close
                }
                let r = self.retry.take();
                self.cancel_timer(r);
                self.check_status_and_schedule_retry();
            }
            Msg::Wake => self.wake(),
            Msg::Refresh(reason) => self.schedule_refresh(&reason),
            Msg::SetOnChange(f) => self.on_change = f,
            Msg::SetLog(l) => self.log = l,
            Msg::SetPollInterval(d) => self.poll_interval = d,
            Msg::SetPolicy(p) => self.policy = p,
            Msg::SetMenuFetchGap(d) => self.menu_fetch_gap = d,
            Msg::Activity(reply) => {
                let _ = reply.send(Activity { socket: self.socket.is_some(), interval_timer: self.interval_timer.is_some(), poll: self.poll_timer.is_some() });
            }
            Msg::SocketOpen(generation) => {
                if generation != self.socket_generation {
                    return;
                }
                self.failures = 0;
                self.launched_at = None;
                self.waiting_for_launch = false;
                self.start_poll();
            }
            Msg::SocketPush(generation, snap) => {
                if generation != self.socket_generation {
                    return;
                }
                self.last_push = Some(Instant::now());
                self.apply(*snap, "websocket");
            }
            Msg::SocketRename(generation, id, value) => {
                if generation == self.socket_generation {
                    self.apply_rename(id, &value);
                }
            }
            Msg::SocketLog(generation, text) => {
                if generation == self.socket_generation {
                    self.log(text);
                }
            }
            Msg::SocketClosed(generation) => {
                if generation == self.socket_generation {
                    self.socket_failed();
                }
            }
            Msg::StatusChecked { status, running, local } => {
                self.status_checked(status, running, local);
            }
            Msg::FetchDone(result, reason) => {
                self.refresh_pending = false;
                match result {
                    Ok(snap) => {
                        self.fetch_until_up = false;
                        self.apply(*snap, &reason);
                    }
                    Err(e) => {
                        self.set_snapshot(None);
                        self.publish(ArenaStatus::Failed(e));
                        self.retry_fetch_while_starting();
                    }
                }
            }
            Msg::PublishStatus(st) => self.publish(st),
            Msg::Shutdown => {}
        }
    }

    fn set_mode(&mut self, new_mode: ArenaUpdateMode, new_interval: Duration) {
        if !(new_mode != self.mode || (new_mode == ArenaUpdateMode::Interval && new_interval != self.interval)) {
            return;
        }
        self.mode = new_mode;
        // Settings clamp to >= 1 s; tests use less.
        self.interval = new_interval.max(Duration::from_millis(50));
        let secs = self.interval.as_secs();
        match new_mode {
            ArenaUpdateMode::Live => self.log("Arena Updates: Live (websocket open)"),
            ArenaUpdateMode::OnDemand => {
                self.log("Arena Updates: On Demand (websocket closed, fetch on menu open)");
            }
            ArenaUpdateMode::Interval => self.log(format!("Arena Updates: every {secs} s (websocket closed, timer {secs} s)")),
        }
        if !self.running {
            return;
        }
        self.end();
        self.failures = 0;
        self.waiting_for_launch = false;
        // A switch inside Arena's startup window keeps retrying until its webserver answers.
        if new_mode != ArenaUpdateMode::Live
            && let Some(at) = self.launched_at
            && at.elapsed().as_secs_f64() < self.policy.startup_window
        {
            self.fetch_until_up = true;
        }
        self.begin();
    }

    /// Starts the current mode.
    fn begin(&mut self) {
        match self.mode {
            ArenaUpdateMode::Live => self.open_socket(),
            ArenaUpdateMode::OnDemand => self.schedule_refresh("on demand"),
            ArenaUpdateMode::Interval => {
                self.schedule_refresh("interval");
                self.start_interval_timer();
            }
        }
    }

    /// Stops every mode's socket, timers and retries.
    fn end(&mut self) {
        self.close_socket();
        let r = self.retry.take();
        self.cancel_timer(r);
        let it = self.interval_timer.take();
        self.cancel_timer(it);
        let fr = self.fetch_retry.take();
        self.cancel_timer(fr);
        self.fetch_until_up = false;
        self.last_push = None;
    }

    fn wake(&mut self) {
        if !self.running {
            return;
        }
        if self.mode != ArenaUpdateMode::Live {
            let now = Instant::now();
            if let Some(last) = self.last_wake_fetch
                && now.duration_since(last) < self.menu_fetch_gap
            {
                return;
            }
            self.last_wake_fetch = Some(now);
            self.schedule_refresh("menu");
            return;
        }
        if self.socket.is_some() || self.status.is_connected() {
            return;
        }
        self.connect_now();
    }

    fn connect_now(&mut self) {
        let r = self.retry.take();
        self.cancel_timer(r);
        if self.socket.is_none() {
            self.open_socket();
        }
    }

    // MARK: websocket

    fn open_socket(&mut self) {
        self.socket_generation += 1;
        let generation = self.socket_generation;
        let ep = self.client.endpoint.clone();
        let tx = self.tx.clone();
        let port = u16::try_from(ep.port.clamp(0, i64::from(u16::MAX))).unwrap_or(8080);
        let host = ep.host.trim_start_matches('[').trim_end_matches(']').to_string();
        let sock = ArenaSocket::start(host, port, "/api/v1".to_string(), self.client.request_timeout, move |event| {
            let msg = match event {
                SocketEvent::Open => Some(Msg::SocketOpen(generation)),
                SocketEvent::Message(bytes) => message_to_msg(generation, bytes),
                SocketEvent::Closed(_) => Some(Msg::SocketClosed(generation)),
            };
            if let Some(m) = msg {
                let _ = tx.send(m);
            }
        });
        self.socket = Some(sock);
        self.subscribed.clear();
        self.attempts += 1;
    }

    fn close_socket(&mut self) {
        self.socket_generation += 1;
        if let Some(s) = self.socket.take() {
            s.cancel();
        }
        let p = self.poll_timer.take();
        self.cancel_timer(p);
    }

    fn socket_failed(&mut self) {
        self.close_socket();
        self.last_push = None;
        if !self.running {
            return;
        }
        self.failures += 1;
        self.check_status_and_schedule_retry();
    }

    /// Publishes the REST status, then schedules the next attempt per the policy (or none while
    /// a local Arena is not running).
    fn check_status_and_schedule_retry(&mut self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        spawn_work(move || {
            let status = client.status();
            let running = client.is_arena_process_running();
            let local = client.endpoint.is_local();
            let _ = tx.send(Msg::StatusChecked { status, running, local });
        });
    }

    fn status_checked(&mut self, st: ArenaStatus, running: Option<bool>, local: bool) {
        if !self.running || self.socket.is_some() {
            return;
        }
        if !st.is_connected() {
            self.set_snapshot(None);
        }
        self.publish(st);
        if self.mode != ArenaUpdateMode::Live {
            return; // other modes fetch on their own schedule
        }
        let since = self.launched_at.map(|t| t.elapsed().as_secs_f64());
        let Some(delay) = self.policy.delay(local, running, self.failures.max(1), since) else {
            if !self.waiting_for_launch {
                self.log("Waiting for Arena to launch (no retries until then)");
            }
            self.waiting_for_launch = true;
            return;
        };
        self.waiting_for_launch = false;
        let r = self.retry.take();
        self.cancel_timer(r);
        self.retry = Some(self.schedule(Duration::from_secs_f64(delay.max(0.0)), TimerKind::Retry));
    }

    fn apply_rename(&mut self, param_id: i64, value: &str) {
        let Some(mut snap) = self.snapshot.clone() else { return };
        let mut changed = false;
        for l in &mut snap.layers {
            if l.name_param_id == Some(param_id) && l.raw_name != value {
                l.raw_name = value.to_string();
                changed = true;
            }
        }
        for c in &mut snap.columns {
            if c.name_param_id == Some(param_id) && c.raw_name != value {
                c.raw_name = value.to_string();
                changed = true;
            }
        }
        if changed {
            self.set_snapshot(Some(snap));
            self.log(format!("Renamed in Arena -> \"{value}\""));
            self.publish(self.status.clone());
        }
    }

    fn subscribe_names(&mut self, snap: &CompositionSnapshot) {
        let Some(sock) = self.socket.as_ref() else { return };
        let ids: BTreeSet<i64> = snap.layers.iter().filter_map(|l| l.name_param_id).chain(snap.columns.iter().filter_map(|c| c.name_param_id)).collect();
        for id in ids.difference(&self.subscribed) {
            sock.send_text(&format!("{{\"action\":\"subscribe\",\"parameter\":\"/parameter/by-id/{id}\"}}"));
        }
        self.subscribed.extend(ids);
    }

    // MARK: poll

    fn start_poll(&mut self) {
        let p = self.poll_timer.take();
        self.cancel_timer(p);
        self.poll_timer = Some(self.schedule(self.poll_interval, TimerKind::Poll));
    }

    fn start_interval_timer(&mut self) {
        let it = self.interval_timer.take();
        self.cancel_timer(it);
        self.interval_timer = Some(self.schedule(self.interval, TimerKind::Interval));
    }

    /// One REST fetch, coalesced: a call while one is in flight is dropped.
    fn schedule_refresh(&mut self, reason: &str) {
        if self.refresh_pending {
            return;
        }
        self.refresh_pending = true;
        self.fetches += 1;
        let client = self.client.clone();
        let tx = self.tx.clone();
        let reason = reason.to_string();
        spawn_work(move || {
            let r = client.snapshot().map(Box::new);
            let _ = tx.send(Msg::FetchDone(r, reason));
        });
    }

    /// Outside Live, after Arena's launch: fetch again until its webserver answers.
    fn retry_fetch_while_starting(&mut self) {
        if !self.fetch_until_up || !self.running || self.mode == ArenaUpdateMode::Live {
            return;
        }
        let within = self.launched_at.is_some_and(|t| t.elapsed().as_secs_f64() < self.policy.startup_window);
        if !within {
            self.fetch_until_up = false;
            return;
        }
        let fr = self.fetch_retry.take();
        self.cancel_timer(fr);
        self.fetch_retry = Some(self.schedule(Duration::from_secs_f64(self.policy.startup_interval.max(0.0)), TimerKind::FetchRetry));
    }

    fn apply(&mut self, snap: CompositionSnapshot, source: &str) {
        // Log only what a person would notice (comp / deck / layer / column count); clip state
        // changes arrive every few seconds and stay silent.
        let structural = self.snapshot.as_ref().is_none_or(|s| {
            s.layers.len() != snap.layers.len()
                || s.columns.len() != snap.columns.len()
                || s.composition_name != snap.composition_name
                || s.deck_name != snap.deck_name
        });
        self.updated_at = Some(Timestamp::now());
        if structural {
            self.log(format!(
                "Composition ({source}): \"{}\" deck {}, {} layers, {} columns",
                snap.composition_name,
                snap.deck_name,
                snap.layers.len(),
                snap.columns.len()
            ));
        }
        self.subscribe_names(&snap);
        if self.snapshot.as_ref() != Some(&snap) {
            self.set_snapshot(Some(snap));
        }
        if self.status.is_connected() {
            self.publish(self.status.clone());
        } else {
            let client = self.client.clone();
            let tx = self.tx.clone();
            spawn_work(move || {
                let _ = tx.send(Msg::PublishStatus(client.status()));
            });
        }
    }

    /// Delivers (snapshot, status, updated_at) unless they equal the last delivery
    /// (`updated_at` counts only outside Live, where menus show it).
    fn publish(&mut self, st: ArenaStatus) {
        if st != self.status {
            self.log(format!("Status: {}", st.plain_line()));
        }
        self.status = st.clone();
        let up = self.updated_at;
        if let Some(d) = &self.delivered
            && d.status == st
            && d.snapshot == self.snapshot
            && (self.mode == ArenaUpdateMode::Live || d.updated_at == up)
        {
            return;
        }
        self.delivered = Some(Delivered { snapshot: self.snapshot.clone(), status: st.clone(), updated_at: up });
        self.deliveries += 1;
        if let Some(f) = self.on_change.clone() {
            // The counters a callback may read are current before it runs.
            self.sync_observed();
            f(self.snapshot.clone(), st, up);
        }
    }
}

/// One complete websocket message, scanned in place on the socket's thread.
fn message_to_msg(generation: u64, bytes: &[u8]) -> Option<Msg> {
    if decoder::looks_like_composition(bytes) {
        return match decoder::decode(bytes) {
            // A quitting Arena pushes an empty composition ("", 0 layers, 0 columns) for a few
            // seconds while it saves; a real one always has a layer and a column. Keep the last
            // real snapshot.
            Ok(snap) if snap.layers.is_empty() && snap.columns.is_empty() => None,
            Ok(snap) => Some(Msg::SocketPush(generation, Box::new(snap))),
            Err(e) => Some(Msg::SocketLog(generation, format!("websocket composition decode failed: {}", e.swift_description()))),
        };
    }
    // Small typed messages only (parameter updates); skip sources / effects lists.
    if bytes.len() >= SMALL_MESSAGE {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let obj = v.as_object()?;
    let kind = obj.get("type")?.as_str()?;
    if kind != "parameter_update" && kind != "parameter_subscribed" {
        return None;
    }
    let id = obj.get("id")?;
    let id = id.as_i64().or_else(|| id.as_f64().map(|f| f as i64))?;
    let value = obj.get("value")?.as_str()?;
    Some(Msg::SocketRename(generation, id, value.to_string()))
}

fn spawn_work(f: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new().name("arena-request".to_string()).spawn(f);
}
