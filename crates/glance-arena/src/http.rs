// SPDX-License-Identifier: MIT
//! A minimal plain-HTTP/1.1 client over `std::net::TcpStream`: one request per connection
//! (`Connection: close`), `Content-Length` or chunked bodies, an idle timeout per read / write /
//! connect and an overall deadline. Arena's webserver is plain HTTP; nothing here speaks TLS.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Why a request did not get an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransportError {
    /// Nothing listens (connection refused).
    Refused,
    /// The connection broke during the exchange.
    Lost,
    TimedOut,
    HostNotFound,
    Other(String),
}

pub(crate) struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

pub(crate) struct Timeouts {
    /// Connect, and the longest wait for any single read or write.
    pub idle: Duration,
    /// The whole exchange.
    pub total: Duration,
}

/// Resolves and connects, trying each address; refused wins over other failures.
pub(crate) fn connect(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, TransportError> {
    let addrs: Vec<SocketAddr> = match (host, port).to_socket_addrs() {
        Ok(a) => a.collect(),
        Err(_) => return Err(TransportError::HostNotFound),
    };
    if addrs.is_empty() {
        return Err(TransportError::HostNotFound);
    }
    let mut refused = false;
    let mut timed_out = false;
    let mut other: Option<String> = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(s) => return Ok(s),
            Err(e) => match e.kind() {
                io::ErrorKind::ConnectionRefused => refused = true,
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => timed_out = true,
                _ => other = Some(e.to_string()),
            },
        }
    }
    if refused {
        Err(TransportError::Refused)
    } else if timed_out {
        Err(TransportError::TimedOut)
    } else {
        Err(TransportError::Other(other.unwrap_or_else(|| "connection failed".to_string())))
    }
}

pub(crate) fn map_io(e: &io::Error) -> TransportError {
    match e.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => TransportError::TimedOut,
        io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::UnexpectedEof
        | io::ErrorKind::NotConnected => TransportError::Lost,
        io::ErrorKind::ConnectionRefused => TransportError::Refused,
        _ => TransportError::Other(e.to_string()),
    }
}

/// One request. `body` = (bytes, content type).
pub(crate) fn request(
    host: &str,
    host_header: &str,
    port: u16,
    method: &str,
    path: &str,
    body: Option<(&[u8], &str)>,
    t: &Timeouts,
) -> Result<Response, TransportError> {
    let start = Instant::now();
    let mut stream = connect(host, port, t.idle)?;
    stream.set_read_timeout(Some(t.idle)).map_err(|e| map_io(&e))?;
    stream.set_write_timeout(Some(t.idle)).map_err(|e| map_io(&e))?;
    let _ = stream.set_nodelay(true);

    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {host_header}:{port}\r\nAccept: */*\r\nUser-Agent: glance-arena\r\nConnection: close\r\n");
    let payload: &[u8] = match body {
        Some((bytes, content_type)) => {
            head += &format!("Content-Type: {content_type}\r\n");
            bytes
        }
        None => &[],
    };
    if body.is_some() || method == "POST" || method == "PUT" {
        head += &format!("Content-Length: {}\r\n", payload.len());
    }
    head += "\r\n";
    let mut req = head.into_bytes();
    req.extend_from_slice(payload);
    stream.write_all(&req).map_err(|e| map_io(&e))?;
    stream.flush().map_err(|e| map_io(&e))?;

    let mut reader = Reader { stream, buf: Vec::new(), start, total: t.total };
    loop {
        let (status, headers) = reader.head()?;
        if (100..200).contains(&status) && status != 101 {
            continue; // interim answer (100 Continue): the real one follows
        }
        let no_body = status == 204 || status == 304 || method == "HEAD";
        let body = if no_body {
            Vec::new()
        } else if headers.chunked {
            reader.chunked()?
        } else if let Some(n) = headers.content_length {
            reader.exact(n)?
        } else {
            reader.until_closed()?
        };
        return Ok(Response { status, body });
    }
}

struct Headers {
    content_length: Option<usize>,
    chunked: bool,
}

struct Reader {
    stream: TcpStream,
    /// Bytes read but not consumed yet.
    buf: Vec<u8>,
    start: Instant,
    total: Duration,
}

impl Reader {
    fn fill(&mut self) -> Result<usize, TransportError> {
        if self.start.elapsed() > self.total {
            return Err(TransportError::TimedOut);
        }
        let mut chunk = [0u8; 64 * 1024];
        let n = self.stream.read(&mut chunk).map_err(|e| map_io(&e))?;
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(n)
    }

    fn head(&mut self) -> Result<(u16, Headers), TransportError> {
        let end = loop {
            if let Some(p) = find(&self.buf, b"\r\n\r\n") {
                break p;
            }
            if self.buf.len() > 64 * 1024 {
                return Err(TransportError::Other("bad server response".to_string()));
            }
            if self.fill()? == 0 {
                return Err(TransportError::Lost);
            }
        };
        let head = String::from_utf8_lossy(&self.buf[..end]).into_owned();
        self.buf.drain(..end + 4);
        let mut lines = head.split("\r\n");
        let status_line = lines.next().unwrap_or("");
        let status =
            status_line.split(' ').nth(1).and_then(|s| s.parse::<u16>().ok()).ok_or_else(|| TransportError::Other("bad server response".to_string()))?;
        let mut h = Headers { content_length: None, chunked: false };
        for line in lines {
            let Some((name, value)) = line.split_once(':') else { continue };
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim();
            if name == "content-length" {
                h.content_length = value.parse().ok();
            } else if name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked") {
                h.chunked = true;
            }
        }
        Ok((status, h))
    }

    fn exact(&mut self, n: usize) -> Result<Vec<u8>, TransportError> {
        // The whole body in one allocation of its final size (a composition is 18-29 MB).
        let mut body = Vec::with_capacity(n.min(1 << 30));
        let take = self.buf.len().min(n);
        body.extend(self.buf.drain(..take));
        while body.len() < n {
            if self.start.elapsed() > self.total {
                return Err(TransportError::TimedOut);
            }
            let want = (n - body.len()).min(1 << 20);
            let old = body.len();
            body.resize(old + want, 0);
            let got = match self.stream.read(&mut body[old..]) {
                Ok(g) => g,
                Err(e) => return Err(map_io(&e)),
            };
            body.truncate(old + got);
            if got == 0 {
                return Err(TransportError::Lost);
            }
        }
        Ok(body)
    }

    fn until_closed(&mut self) -> Result<Vec<u8>, TransportError> {
        while self.fill()? > 0 {}
        Ok(std::mem::take(&mut self.buf))
    }

    fn line(&mut self) -> Result<String, TransportError> {
        loop {
            if let Some(p) = find(&self.buf, b"\r\n") {
                let line = String::from_utf8_lossy(&self.buf[..p]).into_owned();
                self.buf.drain(..p + 2);
                return Ok(line);
            }
            if self.fill()? == 0 {
                return Err(TransportError::Lost);
            }
        }
    }

    fn chunked(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut body = Vec::new();
        loop {
            let size_line = self.line()?;
            let size_hex = size_line.split(';').next().unwrap_or("").trim();
            let size = usize::from_str_radix(size_hex, 16).map_err(|_| TransportError::Other("bad chunk size".to_string()))?;
            if size == 0 {
                // Trailers until the empty line.
                while !self.line()?.is_empty() {}
                return Ok(body);
            }
            let chunk = self.exact(size)?;
            body.extend_from_slice(&chunk);
            let _ = self.line()?; // CRLF after the chunk
        }
    }
}

pub(crate) fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Serves one canned answer on a local port.
    fn serve(answer: &'static [u8]) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut c, _)) = l.accept() {
                let mut buf = [0u8; 4096];
                let _ = c.read(&mut buf);
                let _ = c.write_all(answer);
            }
        });
        port
    }

    fn t() -> Timeouts {
        Timeouts { idle: Duration::from_secs(2), total: Duration::from_secs(5) }
    }

    #[test]
    fn chunked_continue_and_eof_bodies() {
        let port =
            serve(b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4;x=1\r\nWiki\r\n6\r\npedia \r\n0\r\nTrailer: y\r\n\r\n");
        let r = request("127.0.0.1", "127.0.0.1", port, "GET", "/x", None, &t()).unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, b"Wikipedia ".as_slice()));

        let port = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello and more");
        let r = request("127.0.0.1", "127.0.0.1", port, "GET", "/x", None, &t()).unwrap();
        assert_eq!(r.body, b"hello");

        let port = serve(b"HTTP/1.0 404 Not Found\r\n\r\nuntil the end");
        let r = request("127.0.0.1", "127.0.0.1", port, "GET", "/x", None, &t()).unwrap();
        assert_eq!((r.status, r.body.as_slice()), (404, b"until the end".as_slice()));

        let port = serve(b"HTTP/1.1 204 No Content\r\n\r\n");
        let r = request("127.0.0.1", "127.0.0.1", port, "POST", "/x", None, &t()).unwrap();
        assert_eq!((r.status, r.body.len()), (204, 0));
    }

    #[test]
    fn truncated_answers_are_lost_connections() {
        let port = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\nshort");
        assert_eq!(request("127.0.0.1", "127.0.0.1", port, "GET", "/x", None, &t()).err(), Some(TransportError::Lost));
        let port = serve(b"HTTP/1.1 200 OK\r\n");
        assert_eq!(request("127.0.0.1", "127.0.0.1", port, "GET", "/x", None, &t()).err(), Some(TransportError::Lost));
    }
}
