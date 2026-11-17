// SPDX-License-Identifier: MIT
//! Minimal RFC 6455 websocket client for Arena's `ws://host:port/api/v1`.
//!
//! Arena pushes the full ~18-29 MB composition as one message (in ~64 KB fragments) on every clip
//! load. A frame's payload length is read from its header (RFC 6455 5.2), payloads are appended to
//! one buffer reserved at the announced size, the complete message is handed to the handler in
//! place, and the buffer is released right after. Masked server frames are refused, control
//! frames answered (ping → pong, close echoed), messages over 512 MiB refused, the handshake's
//! accept key checked. No TLS, no extensions.
//!
//! The reader runs on its own thread; the handler is called there.

use crate::http::{self, find};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) enum SocketEvent<'a> {
    Open,
    /// A complete message (text or binary), valid only during the call.
    Message(&'a [u8]),
    /// `None`: closed cleanly (close frame or end of stream); `Some`: why it failed (the store
    /// only needs the fact; tests read the reason).
    Closed(#[allow(dead_code)] Option<String>),
}

pub(crate) const MAX_MESSAGE: usize = 512 * 1024 * 1024;
const ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

struct Shared {
    closed: AtomicBool,
    writer: Mutex<Option<TcpStream>>,
}

impl Shared {
    fn send(&self, opcode: u8, payload: &[u8]) {
        let frame = client_frame(opcode, payload, random4());
        let mut g = self.writer.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(s) = g.as_mut() {
            let _ = s.write_all(&frame);
        }
    }

    fn shutdown(&self) {
        let g = self.writer.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(s) = g.as_ref() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }
}

/// A websocket connection in progress or open. Dropping it does not close it; `cancel` does.
pub(crate) struct ArenaSocket {
    shared: Arc<Shared>,
}

impl ArenaSocket {
    /// Connects on a new thread; `handler` receives the events there.
    pub(crate) fn start(
        host: String,
        port: u16,
        path: String,
        connect_timeout: Duration,
        handler: impl FnMut(SocketEvent<'_>) + Send + 'static,
    ) -> ArenaSocket {
        let shared = Arc::new(Shared { closed: AtomicBool::new(false), writer: Mutex::new(None) });
        let s2 = Arc::clone(&shared);
        let spawned = std::thread::Builder::new().name("arena-socket".to_string()).spawn(move || {
            let mut conn = Connection::new(s2, handler);
            conn.run(&host, port, &path, connect_timeout);
        });
        if spawned.is_err() {
            shared.closed.store(true, Ordering::SeqCst);
        }
        ArenaSocket { shared }
    }

    /// Stops: no more events are delivered.
    pub(crate) fn cancel(&self) {
        self.shared.closed.store(true, Ordering::SeqCst);
        self.shared.shutdown();
    }

    pub(crate) fn send_text(&self, text: &str) {
        self.shared.send(0x1, text.as_bytes());
    }
}

struct Connection<H> {
    shared: Arc<Shared>,
    handler: H,
    finished: bool,
    // Frame state
    header: Vec<u8>,
    message: Vec<u8>,
    frame_remaining: usize,
    frame_opcode: u8,
    frame_fin: bool,
    control: Vec<u8>,
    in_control: bool,
}

impl<H: FnMut(SocketEvent<'_>)> Connection<H> {
    fn new(shared: Arc<Shared>, handler: H) -> Self {
        Connection {
            shared,
            handler,
            finished: false,
            header: Vec::new(),
            message: Vec::new(),
            frame_remaining: 0,
            frame_opcode: 0,
            frame_fin: false,
            control: Vec::new(),
            in_control: false,
        }
    }

    fn closed(&self) -> bool {
        self.finished || self.shared.closed.load(Ordering::SeqCst)
    }

    fn run(&mut self, host: &str, port: u16, path: &str, connect_timeout: Duration) {
        let mut stream = match http::connect(host, port, connect_timeout) {
            Ok(s) => s,
            Err(e) => return self.finish(Some(format!("websocket: {e:?}"))),
        };
        let _ = stream.set_nodelay(true);
        let _ = stream.set_write_timeout(Some(Duration::from_secs(8)));
        match stream.try_clone() {
            Ok(w) => {
                *self.shared.writer.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(w);
            }
            Err(e) => return self.finish(Some(e.to_string())),
        }
        if self.closed() {
            self.shared.shutdown();
            return;
        }
        // Handshake (RFC 6455 4.1).
        let key = base64(&random16());
        let host_header = if host.contains(':') { format!("[{host}]") } else { host.to_string() };
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {host_header}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        if let Err(e) = stream.write_all(req.as_bytes()) {
            return self.finish(Some(e.to_string()));
        }
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = vec![0u8; 256 * 1024];
        let end = loop {
            if let Some(p) = find(&buf, b"\r\n\r\n") {
                break p;
            }
            let n = match stream.read(&mut chunk) {
                Ok(n) => n,
                Err(e) => return self.finish(Some(e.to_string())),
            };
            if self.closed() {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
            if n == 0 || buf.len() > 64 * 1024 {
                return self.finish(Some("websocket handshake: no response".to_string()));
            }
        };
        let head = String::from_utf8_lossy(&buf[..end]).into_owned();
        if !head.starts_with("HTTP/1.1 101") {
            let first: String = head.chars().take(60).collect();
            return self.finish(Some(format!("websocket handshake: {first}")));
        }
        let expected = base64(&sha1(format!("{key}{ACCEPT_GUID}").as_bytes()));
        let accept = head
            .split("\r\n")
            .find(|l| l.to_lowercase().starts_with("sec-websocket-accept:"))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_string());
        if accept.as_deref() != Some(expected.as_str()) {
            return self.finish(Some("websocket handshake: bad accept key".to_string()));
        }
        (self.handler)(SocketEvent::Open);
        let rest = buf[end + 4..].to_vec();
        drop(buf);
        if !rest.is_empty() {
            self.consume(&rest);
        }
        // Frames (RFC 6455 5.2).
        while !self.closed() {
            match stream.read(&mut chunk) {
                Ok(0) => return self.finish(None),
                Ok(n) => self.consume(&chunk[..n]),
                Err(e) => {
                    if self.closed() {
                        return;
                    }
                    return self.finish(Some(e.to_string()));
                }
            }
        }
    }

    fn consume(&mut self, bytes: &[u8]) {
        let mut i = 0;
        while i < bytes.len() && !self.closed() {
            if self.frame_remaining > 0 {
                let n = self.frame_remaining.min(bytes.len() - i);
                let part = &bytes[i..i + n];
                if self.in_control {
                    self.control.extend_from_slice(part);
                } else {
                    self.message.extend_from_slice(part);
                }
                self.frame_remaining -= n;
                i += n;
                if self.frame_remaining == 0 {
                    self.frame_done();
                }
                continue;
            }
            self.header.push(bytes[i]);
            i += 1;
            self.parse_header_if_complete();
        }
    }

    fn parse_header_if_complete(&mut self) {
        if self.header.len() < 2 {
            return;
        }
        let masked = self.header[1] & 0x80 != 0;
        let len7 = usize::from(self.header[1] & 0x7F);
        let ext_len = match len7 {
            126 => 2,
            127 => 8,
            _ => 0,
        };
        let need = 2 + ext_len + if masked { 4 } else { 0 };
        if self.header.len() < need {
            return;
        }
        if masked {
            return self.fail("masked server frame"); // RFC 6455 5.1: servers never mask
        }
        let mut length: u64 = len7 as u64;
        if ext_len > 0 {
            length = 0;
            for k in 0..ext_len {
                length = (length << 8) | u64::from(self.header[2 + k]);
            }
        }
        self.frame_fin = self.header[0] & 0x80 != 0;
        self.frame_opcode = self.header[0] & 0x0F;
        self.header.clear();
        self.in_control = self.frame_opcode >= 0x8;
        let length = usize::try_from(length).unwrap_or(usize::MAX);
        if self.in_control {
            if length > 125 || !self.frame_fin {
                return self.fail("bad control frame");
            }
            self.control.clear();
        } else {
            if self.message.len().saturating_add(length) > MAX_MESSAGE {
                return self.fail("message too large");
            }
            if self.message.is_empty() {
                self.message.reserve_exact(length);
            } else {
                self.message.reserve(length); // fragments grow geometrically
            }
        }
        self.frame_remaining = length;
        if length == 0 {
            self.frame_done();
        }
    }

    fn frame_done(&mut self) {
        if self.in_control {
            self.in_control = false;
            match self.frame_opcode {
                0x8 => {
                    let n = self.control.len().min(2);
                    let echo = self.control[..n].to_vec();
                    self.shared.send(0x8, &echo); // close echo
                    self.finish(None);
                }
                0x9 => {
                    let payload = self.control.clone();
                    self.shared.send(0xA, &payload); // ping → pong
                }
                _ => {} // pong
            }
            return;
        }
        if !self.frame_fin {
            return; // more continuation frames follow
        }
        let message = std::mem::take(&mut self.message);
        if !self.closed() {
            (self.handler)(SocketEvent::Message(&message));
        }
    }

    fn fail(&mut self, reason: &str) {
        self.finish(Some(format!("websocket: {reason}")));
    }

    fn finish(&mut self, error: Option<String>) {
        if self.closed() {
            return;
        }
        self.finished = true;
        self.shared.closed.store(true, Ordering::SeqCst);
        self.shared.shutdown();
        self.message = Vec::new();
        (self.handler)(SocketEvent::Closed(error));
    }
}

// MARK: frames from the client are masked (RFC 6455 5.3)

pub(crate) fn client_frame(opcode: u8, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let n = payload.len();
    let mut frame = Vec::with_capacity(n + 14);
    frame.push(0x80 | opcode);
    if n < 126 {
        frame.push(0x80 | u8::try_from(n).unwrap_or(0));
    } else if n <= 0xFFFF {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&u16::try_from(n).unwrap_or(0).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(n as u64).to_be_bytes());
    }
    frame.extend_from_slice(&mask);
    frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    frame
}

fn random16() -> [u8; 16] {
    uuid::Uuid::new_v4().into_bytes()
}

fn random4() -> [u8; 4] {
    let b = random16();
    [b[0], b[1], b[2], b[3]]
}

// MARK: SHA-1 (FIPS 180-4) and base64 (RFC 4648), for the handshake's accept key.

pub(crate) fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (t, word) in block.as_chunks::<4>().0.iter().enumerate() {
            w[t] = u32::from_be_bytes(*word);
        }
        for t in 16..80 {
            w[t] = (w[t - 3] ^ w[t - 8] ^ w[t - 14] ^ w[t - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (t, wt) in w.iter().enumerate() {
            let (f, k) = match t {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wt);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

pub(crate) fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let idx = |shift: u32| char::from(A[((n >> shift) & 63) as usize]);
        out.push(idx(18));
        out.push(idx(12));
        out.push(if chunk.len() > 1 { idx(6) } else { '=' });
        out.push(if chunk.len() > 2 { idx(0) } else { '=' });
    }
    out
}

/// The `Sec-WebSocket-Accept` value for a key (RFC 6455 4.2.2), for test servers.
#[doc(hidden)]
pub fn accept_key(key: &str) -> String {
    base64(&sha1(format!("{key}{ACCEPT_GUID}").as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6455_accept_example() {
        // RFC 6455 1.3.
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn sha1_and_base64_vectors() {
        assert_eq!(base64(&sha1(b"abc")), "qZk+NkcGgWq6PiVxeFDCbJzQ2J0=");
        assert_eq!(base64(&sha1(b"")), "2jmj7l5rSw0yVb/vlWAYkK/YBwk=");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }

    #[test]
    fn client_frames_are_masked() {
        let f = client_frame(0x1, b"Hello", [0x37, 0xfa, 0x21, 0x3d]);
        // RFC 6455 5.7: a masked "Hello".
        assert_eq!(f, vec![0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58]);
        let big = client_frame(0x2, &[0u8; 300], [0; 4]);
        assert_eq!(&big[..4], &[0x82, 0x80 | 126, 0x01, 0x2C]);
        let huge = client_frame(0x2, &vec![0u8; 70_000], [0; 4]);
        assert_eq!(&huge[..2], &[0x82, 0x80 | 127]);
        assert_eq!(&huge[2..10], &70_000u64.to_be_bytes());
    }

    use std::net::TcpListener;
    use std::sync::mpsc;

    #[derive(Debug, PartialEq)]
    enum Ev {
        Open,
        Msg(Vec<u8>),
        Closed(Option<String>),
    }

    /// A server that answers the upgrade with `answer(key)` then writes `then`, and the events
    /// the client saw.
    fn run(answer: impl Fn(&str) -> String + Send + 'static, then: Vec<u8>) -> Vec<Ev> {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let Ok((mut c, _)) = l.accept() else { return };
            let mut buf = vec![0u8; 4096];
            let n = c.read(&mut buf).unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]).into_owned();
            let key = head.split("\r\n").find_map(|l| l.strip_prefix("Sec-WebSocket-Key: ")).unwrap_or("").to_string();
            let _ = c.write_all(answer(&key).as_bytes());
            let _ = c.write_all(&then);
            std::thread::sleep(Duration::from_millis(300));
        });
        let (tx, rx) = mpsc::channel();
        let _s = ArenaSocket::start("127.0.0.1".into(), port, "/api/v1".into(), Duration::from_secs(2), move |e| {
            let ev = match e {
                SocketEvent::Open => Ev::Open,
                SocketEvent::Message(b) => Ev::Msg(b.to_vec()),
                SocketEvent::Closed(r) => Ev::Closed(r),
            };
            let _ = tx.send(ev);
        });
        let mut out = Vec::new();
        while let Ok(ev) = rx.recv_timeout(Duration::from_secs(3)) {
            let done = matches!(ev, Ev::Closed(_));
            out.push(ev);
            if done {
                break;
            }
        }
        out
    }

    fn ok(key: &str) -> String {
        format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n", accept_key(key))
    }

    fn server_frame(first: u8, payload: &[u8]) -> Vec<u8> {
        let mut f = vec![first];
        let n = payload.len();
        if n < 126 {
            f.push(n as u8);
        } else {
            f.push(126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
        f.extend_from_slice(payload);
        f
    }

    #[test]
    fn handshake_failures() {
        assert_eq!(
            run(|_| "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".into(), vec![]),
            [Ev::Closed(Some("websocket handshake: HTTP/1.1 404 Not Found\r\nContent-Length: 0".into()))]
        );
        assert_eq!(
            run(|_| "HTTP/1.1 101 Switching Protocols\r\nSec-WebSocket-Accept: nope\r\n\r\n".into(), vec![]),
            [Ev::Closed(Some("websocket handshake: bad accept key".into()))]
        );
    }

    #[test]
    fn fragments_reassemble_and_control_frames() {
        let mut then = server_frame(0x01, b"{\"na"); // text, not final
        then.extend(server_frame(0x89, b"hi")); // ping between fragments
        then.extend(server_frame(0x00, &[b'x'; 300])); // continuation, 16-bit length
        then.extend(server_frame(0x80, b"\"}")); // final continuation
        then.extend(server_frame(0x88, &[0x03, 0xE8])); // close
        let evs = run(ok, then);
        let mut want = b"{\"na".to_vec();
        want.extend([b'x'; 300]);
        want.extend(b"\"}");
        assert_eq!(evs, [Ev::Open, Ev::Msg(want), Ev::Closed(None)]);
    }

    #[test]
    fn masked_and_bad_control_frames_fail() {
        let masked = vec![0x81, 0x81, 1, 2, 3, 4, b'a' ^ 1];
        assert_eq!(run(ok, masked), [Ev::Open, Ev::Closed(Some("websocket: masked server frame".into()))]);
        let long_ping = server_frame(0x89, &[0u8; 126]);
        assert_eq!(run(ok, long_ping), [Ev::Open, Ev::Closed(Some("websocket: bad control frame".into()))]);
        let mut too_big = vec![0x81, 127];
        too_big.extend_from_slice(&((MAX_MESSAGE as u64) + 1).to_be_bytes());
        assert_eq!(run(ok, too_big), [Ev::Open, Ev::Closed(Some("websocket: message too large".into()))]);
    }
}
