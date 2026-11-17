// SPDX-License-Identifier: MIT
//! A fake Arena on a local port: the REST endpoints the kit uses (paths and bodies as in Arena's
//! swagger.yaml: composition, product, layers/add, columns/add, rename by id, open, clip,
//! connect) and the websocket at `/api/v1`. Never a real Arena.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glance_arena::{ArenaClient, ArenaEndpoint, ArenaFileURL, websocket_accept_key};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
pub struct FSlot {
    pub name: String,
    pub state: String,
}

impl FSlot {
    pub fn empty() -> Self {
        FSlot { name: String::new(), state: "Empty".to_string() }
    }
    pub fn loaded(name: &str, state: &str) -> Self {
        FSlot { name: name.to_string(), state: state.to_string() }
    }
}

#[derive(Clone, Debug)]
pub struct FLayer {
    pub id: i64,
    pub name: String,
    pub slots: Vec<FSlot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WsMode {
    /// Answer the upgrade with 404 (the websocket fails, REST works).
    Refuse,
    /// Accept; push the composition on connect.
    Accept,
}

pub struct FakeState {
    pub layers: Vec<FLayer>,
    pub columns: usize,
    pub connects: Vec<String>,
    pub next_id: i64,
    /// "METHOD /path" of every REST request.
    pub requests: Vec<String>,
    /// Bodies of `open` requests.
    pub opened: Vec<String>,
    /// `PUT` bodies.
    pub puts: Vec<String>,
    pub ws_mode: WsMode,
    pub push_on_connect: bool,
    /// Text frames the client sent.
    pub ws_received: Vec<String>,
    pub ws_upgrades: usize,
    ws_clients: Vec<TcpStream>,
    /// Answer every REST request with this status and body.
    pub override_answer: Option<(u16, String)>,
    /// Accept REST connections and never answer.
    pub hang: bool,
    /// `open` bodies containing this answer 404.
    pub fail_open: Option<String>,
    pub composition_name: String,
    /// After an open, the slot reads "Empty" this many times before the clip shows (Arena answers the open before
    /// the clip is in the slot).
    pub load_delay_reads: usize,
    /// Reads left before each opened slot shows its clip.
    pub pending_reads: std::collections::HashMap<(usize, usize), usize>,
}

pub struct FakeArena {
    pub port: u16,
    state: Arc<Mutex<FakeState>>,
    stop: Arc<AtomicBool>,
}

impl Drop for FakeArena {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        let s = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for c in &s.ws_clients {
            let _ = c.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl FakeArena {
    pub fn start(layers: usize, columns: usize) -> FakeArena {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(FakeState {
            layers: Vec::new(),
            columns: 0,
            connects: Vec::new(),
            next_id: 1000,
            requests: Vec::new(),
            opened: Vec::new(),
            puts: Vec::new(),
            ws_mode: WsMode::Refuse,
            push_on_connect: true,
            ws_received: Vec::new(),
            ws_upgrades: 0,
            ws_clients: Vec::new(),
            override_answer: None,
            hang: false,
            fail_open: None,
            load_delay_reads: 0,
            pending_reads: std::collections::HashMap::new(),
            composition_name: "Fake".to_string(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let fake = FakeArena { port, state: Arc::clone(&state), stop: Arc::clone(&stop) };
        fake.reset(layers, columns);
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(conn) = conn else { continue };
                let st = Arc::clone(&state);
                std::thread::spawn(move || handle_conn(conn, &st));
            }
        });
        fake
    }

    pub fn lock(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap()
    }

    pub fn reset(&self, layers: usize, columns: usize) {
        let mut s = self.lock();
        s.columns = columns;
        s.layers = (0..layers).map(|i| FLayer { id: 500 + i as i64, name: "Layer #".to_string(), slots: vec![FSlot::empty(); columns] }).collect();
        s.connects.clear();
    }

    pub fn endpoint(&self) -> ArenaEndpoint {
        ArenaEndpoint::new("127.0.0.1", i64::from(self.port))
    }

    /// A client whose "is Arena running" answer is `running`.
    pub fn client(&self, running: Option<bool>) -> ArenaClient {
        ArenaClient::with_probe(self.endpoint(), Arc::new(move || running))
    }

    pub fn composition_json(&self) -> String {
        composition(&self.lock())
    }

    /// Sends a text message to every open websocket, in fragments of `fragment` bytes (0 = one
    /// frame).
    pub fn push(&self, text: &str, fragment: usize) {
        let mut s = self.lock();
        let frames = server_frames(0x1, text.as_bytes(), fragment);
        s.ws_clients.retain_mut(|c| c.write_all(&frames).is_ok());
    }

    pub fn push_raw(&self, bytes: &[u8]) {
        let mut s = self.lock();
        s.ws_clients.retain_mut(|c| c.write_all(bytes).is_ok());
    }

    /// Drops every open websocket (as a quitting Arena does).
    pub fn drop_sockets(&self) {
        let mut s = self.lock();
        for c in &s.ws_clients {
            let _ = c.shutdown(std::net::Shutdown::Both);
        }
        s.ws_clients.clear();
    }

    pub fn open_sockets(&self) -> usize {
        self.lock().ws_clients.len()
    }

    /// Waits until `f` holds (up to `timeout`).
    pub fn wait_for(&self, timeout: Duration, f: impl Fn(&FakeState) -> bool) -> bool {
        let t0 = Instant::now();
        while t0.elapsed() < timeout {
            if f(&self.lock()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        f(&self.lock())
    }
}

/// The composition as Arena writes it: the name parameter first (the websocket sniff relies on
/// it), then decks, layers, columns.
pub fn composition(s: &FakeState) -> String {
    let layers: Vec<Value> = s
        .layers
        .iter()
        .map(|l| {
            json!({
                "id": l.id,
                "name": {"value": l.name, "id": l.id + 1},
                "clips": l.slots.iter().enumerate().map(|(i, c)| json!({
                    "id": l.id * 1000 + i as i64,
                    "name": {"value": c.name},
                    "connected": {"value": c.state},
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let columns: Vec<Value> = (0..s.columns).map(|i| json!({"id": 9000 + i as i64, "name": {"value": "Column #", "id": 9500 + i as i64}})).collect();
    format!(
        "{{\"name\":{},\"decks\":{},\"layers\":{},\"columns\":{}}}",
        json!({"value": s.composition_name}),
        json!([{"name": {"value": "Main"}, "selected": {"value": true}}]),
        Value::Array(layers),
        Value::Array(columns)
    )
}

fn server_frames(opcode: u8, payload: &[u8], fragment: usize) -> Vec<u8> {
    let parts: Vec<&[u8]> = if fragment == 0 || payload.is_empty() { vec![payload] } else { payload.chunks(fragment).collect() };
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let first = i == 0;
        let last = i + 1 == parts.len();
        let op = if first { opcode } else { 0 };
        out.push(if last { 0x80 | op } else { op });
        let n = part.len();
        if n < 126 {
            out.push(n as u8);
        } else if n <= 0xFFFF {
            out.push(126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        } else {
            out.push(127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
        out.extend_from_slice(part);
    }
    out
}

/// A server frame (unmasked), for tests that build their own.
pub fn frame(opcode: u8, fin: bool, payload: &[u8]) -> Vec<u8> {
    let mut f = server_frames(opcode, payload, 0);
    if !fin {
        f[0] &= 0x7F;
    }
    f
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn handle_conn(mut conn: TcpStream, state: &Arc<Mutex<FakeState>>) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 65536];
    let end = loop {
        if let Some(p) = find(&buf, b"\r\n\r\n") {
            break p;
        }
        match conn.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    let mut rest = buf[end + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut headers: HashMap<String, String> = HashMap::new();
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    if headers.get("upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket")) {
        return websocket(conn, state, &headers, rest);
    }
    if state.lock().unwrap().hang {
        std::thread::sleep(Duration::from_secs(30));
        return;
    }
    let len: usize = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    while rest.len() < len {
        match conn.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => rest.extend_from_slice(&chunk[..n]),
        }
    }
    let (status, body) = route(state, &method, &path, &rest);
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        _ => "Status",
    };
    let mut resp = format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\n");
    if status != 204 {
        resp += &format!("Content-Type: application/json\r\nContent-Length: {}\r\n", body.len());
    }
    resp += "\r\n";
    let mut bytes = resp.into_bytes();
    if status != 204 {
        bytes.extend_from_slice(&body);
    }
    let _ = conn.write_all(&bytes);
}

fn route(state: &Arc<Mutex<FakeState>>, method: &str, path: &str, body: &[u8]) -> (u16, Vec<u8>) {
    let mut s = state.lock().unwrap();
    s.requests.push(format!("{method} {path}"));
    if let Some((st, b)) = s.override_answer.clone() {
        return (st, b.into_bytes());
    }
    let p: Vec<&str> = path.split('/').filter(|x| !x.is_empty()).skip(2).collect(); // drop api, v1
    let json = |v: Value| (200u16, v.to_string().into_bytes());
    match (method, p.len()) {
        ("GET", 1) if p[0] == "product" => json(json!({"name": "Arena", "major": 7, "minor": 28, "micro": 1, "revision": 26535})),
        ("GET", 1) => (200, composition(&s).into_bytes()),
        ("POST", 3) if p[1] == "layers" => {
            s.next_id += 1;
            let id = s.next_id;
            let cols = s.columns;
            s.layers.push(FLayer { id, name: "Layer #".to_string(), slots: vec![FSlot::empty(); cols] });
            (204, Vec::new())
        }
        ("POST", 3) if p[1] == "columns" => {
            s.columns += 1;
            for l in &mut s.layers {
                l.slots.push(FSlot::empty());
            }
            (204, Vec::new())
        }
        ("PUT", 4) => {
            s.puts.push(String::from_utf8_lossy(body).into_owned());
            if let Ok(id) = p[3].parse::<i64>() {
                let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
                if let Some(name) = v["name"]["value"].as_str() {
                    for l in &mut s.layers {
                        if l.id == id {
                            l.name = name.to_string();
                        }
                    }
                }
            }
            (204, Vec::new())
        }
        (_, 6) if p[1] == "layers" => {
            let (Ok(l), Ok(c)) = (p[2].parse::<usize>(), p[4].parse::<usize>()) else {
                return (400, Vec::new());
            };
            if l == 0 || c == 0 || l > s.layers.len() || c > s.columns {
                return (404, b"no such clip".to_vec());
            }
            if method == "POST" && p[5] == "open" {
                let url = String::from_utf8_lossy(body).into_owned();
                s.opened.push(url.clone());
                if s.fail_open.as_ref().is_some_and(|f| url.contains(f.as_str())) {
                    return (404, b"file not found".to_vec());
                }
                let name = ArenaFileURL::decode(&url).map(|d| d.rsplit('/').next().unwrap_or("?").to_string()).unwrap_or_else(|| "?".to_string());
                s.layers[l - 1].slots[c - 1] = FSlot::loaded(&name, "Disconnected");
                if s.load_delay_reads > 0 {
                    let n = s.load_delay_reads;
                    s.pending_reads.insert((l, c), n);
                }
            } else if method == "POST" && p[5] == "connect" {
                s.layers[l - 1].slots[c - 1].state = "Connected".to_string();
                s.connects.push(format!("L{l} C{c}"));
            }
            (204, Vec::new())
        }
        ("GET", 5) => {
            let (Ok(l), Ok(c)) = (p[2].parse::<usize>(), p[4].parse::<usize>()) else {
                return (400, Vec::new());
            };
            if let Some(left) = s.pending_reads.get_mut(&(l, c))
                && *left > 0
            {
                *left -= 1;
                return json(json!({"id": 1, "name": {"value": ""}, "connected": {"value": "Empty"}}));
            }
            let Some(slot) = s.layers.get(l.wrapping_sub(1)).and_then(|x| x.slots.get(c.wrapping_sub(1))) else {
                return (404, Vec::new());
            };
            json(json!({"id": 1, "name": {"value": slot.name}, "connected": {"value": slot.state}}))
        }
        _ => (404, format!("no fake for {method} {path}").into_bytes()),
    }
}

fn websocket(mut conn: TcpStream, state: &Arc<Mutex<FakeState>>, headers: &HashMap<String, String>, mut rest: Vec<u8>) {
    let mode = {
        let mut s = state.lock().unwrap();
        s.ws_upgrades += 1;
        s.ws_mode
    };
    if mode == WsMode::Refuse {
        let _ = conn.write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
        return;
    }
    let key = headers.get("sec-websocket-key").cloned().unwrap_or_default();
    let accept = websocket_accept_key(&key);
    let resp = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n");
    if conn.write_all(resp.as_bytes()).is_err() {
        return;
    }
    {
        let mut s = state.lock().unwrap();
        if s.push_on_connect {
            let comp = composition(&s);
            let _ = conn.write_all(&server_frames(0x1, comp.as_bytes(), 65536));
        }
        if let Ok(w) = conn.try_clone() {
            s.ws_clients.push(w);
        }
    }
    // Read the client's (masked) frames.
    let mut chunk = [0u8; 65536];
    loop {
        while let Some((opcode, payload, used)) = parse_client_frame(&rest) {
            rest.drain(..used);
            match opcode {
                0x1 => state.lock().unwrap().ws_received.push(String::from_utf8_lossy(&payload).into_owned()),
                0x8 => {
                    let mut f = vec![0x88, payload.len().min(2) as u8];
                    f.extend_from_slice(&payload[..payload.len().min(2)]);
                    let _ = conn.write_all(&f);
                    return;
                }
                0xA => state.lock().unwrap().ws_received.push(format!("PONG {}", String::from_utf8_lossy(&payload))),
                _ => {}
            }
        }
        match conn.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => rest.extend_from_slice(&chunk[..n]),
        }
    }
}

/// (opcode, unmasked payload, bytes used); panics on an unmasked client frame.
fn parse_client_frame(b: &[u8]) -> Option<(u8, Vec<u8>, usize)> {
    if b.len() < 2 {
        return None;
    }
    assert!(b[1] & 0x80 != 0, "client frames must be masked");
    let len7 = (b[1] & 0x7F) as usize;
    let (len, mut i) = match len7 {
        126 => (u16::from_be_bytes([*b.get(2)?, *b.get(3)?]) as usize, 4),
        127 => {
            let mut a = [0u8; 8];
            a.copy_from_slice(b.get(2..10)?);
            (u64::from_be_bytes(a) as usize, 10)
        }
        n => (n, 2),
    };
    let mask = b.get(i..i + 4)?.to_vec();
    i += 4;
    let payload: Vec<u8> = b.get(i..i + len)?.iter().enumerate().map(|(k, x)| x ^ mask[k % 4]).collect();
    Some((b[0] & 0x0F, payload, i + len))
}

/// A local port nothing listens on.
pub fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

/// A fresh, empty temporary folder.
pub fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("glance-arena-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub fn sleep(secs: f64) {
    std::thread::sleep(Duration::from_secs_f64(secs));
}
