//! An XRootD (`root://`) byte-range [`ByteSource`] (the `xrootd` feature).
//!
//! XRootD is the binary TCP protocol most CERN data is served over. This is a
//! read-only client for the public/open-data case: it speaks the `xroot`
//! protocol directly (no libXrdCl, no extra dependencies — just `std::net`) and
//! authenticates with the credential-free `unix` security protocol, so it reads
//! world-readable files from servers that offer `unix` auth (e.g.
//! `root://eospublic.cern.ch`). GSI/Kerberos/token security is out of scope.
//!
//! `kXR_read(handle, offset, length)` maps directly onto
//! [`ByteSource::read_at`], so an [`XrootdSource`] plugs into [`FileReader`] exactly
//! like the HTTP source, and every reader fetches only the ranges it touches.
//!
//! The wire framing here was validated against `root://eospublic.cern.ch`.
//!
//! [`FileReader`]: super::reader::FileReader

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::Duration;

use bytes::Bytes;

use super::source::ByteSource;
use crate::error::{Error, Result};

// Request opcodes (`kXR_*`).
const KXR_AUTH: u16 = 3000;
const KXR_CLOSE: u16 = 3003;
const KXR_LOGIN: u16 = 3007;
const KXR_OPEN: u16 = 3010;
const KXR_READ: u16 = 3013;
const KXR_STAT: u16 = 3017;

// Response status codes.
const KXR_OK: u16 = 0;
const KXR_OKSOFAR: u16 = 4000;
const KXR_ERROR: u16 = 4003;
const KXR_REDIRECT: u16 = 4004;
const KXR_WAIT: u16 = 4005;

// Open option: read-only.
const KXR_OPEN_READ: u16 = 0x10;

/// Default XRootD port.
const DEFAULT_PORT: u16 = 1094;
/// Guard against a redirect loop.
const MAX_REDIRECTS: u32 = 8;
/// Connect / read timeout.
const TIMEOUT: Duration = Duration::from_secs(30);

/// A parsed `root://[user@]host[:port]//path` URL.
struct XrootdUrl {
    host: String,
    port: u16,
    path: String,
}

impl XrootdUrl {
    fn parse(url: &str) -> Result<XrootdUrl> {
        let rest = url
            .strip_prefix("root://")
            .or_else(|| url.strip_prefix("roots://"))
            .ok_or_else(|| Error::Format(format!("not a root:// URL: {url:?}")))?;
        // Authority is up to the first '/', the path is the remainder. XRootD's
        // canonical form uses a double slash — `root://host//abs/path` — so the
        // remainder already carries the leading slash of the absolute path.
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let authority = authority.rsplit('@').next().unwrap_or(authority); // drop any user@
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().unwrap_or(DEFAULT_PORT)),
            None => (authority.to_string(), DEFAULT_PORT),
        };
        if host.is_empty() {
            return Err(Error::Format(format!("root:// URL has no host: {url:?}")));
        }
        let path = if path.is_empty() {
            return Err(Error::Format(format!("root:// URL has no path: {url:?}")));
        } else if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        Ok(XrootdUrl { host, port, path })
    }
}

/// A live connection to one XRootD server with a file open, serialized so the
/// request/response stream stays in step under concurrent reads.
struct Conn {
    stream: TcpStream,
    stream_id: u16,
    fhandle: [u8; 4],
}

impl Conn {
    /// Handshake + login + (unix) auth against `host:port`, then open `path` for
    /// reading, following redirects. Returns the connection and the file size.
    fn open(host: &str, port: u16, path: &str) -> Result<(Conn, u64)> {
        let mut host = host.to_string();
        let mut port = port;
        // A redirector (e.g. EOS) hands back an opaque capability token that must
        // be appended to the path when the data server re-opens the file.
        let mut opaque = String::new();
        for _ in 0..MAX_REDIRECTS {
            let mut stream = TcpStream::connect((host.as_str(), port))
                .map_err(|e| io_err(&format!("connecting to {host}:{port}"), e))?;
            stream.set_read_timeout(Some(TIMEOUT)).ok();
            stream.set_write_timeout(Some(TIMEOUT)).ok();

            handshake(&mut stream)?;
            let mut conn = Conn {
                stream,
                stream_id: 0,
                fhandle: [0; 4],
            };
            conn.login()?;

            let open_path = if opaque.is_empty() {
                path.to_string()
            } else {
                format!("{path}?{opaque}")
            };
            match conn.try_open(&open_path)? {
                OpenOutcome::Opened => {
                    let size = conn.fstat()?;
                    return Ok((conn, size));
                }
                OpenOutcome::Redirect {
                    host: h,
                    port: p,
                    opaque: o,
                } => {
                    host = h;
                    port = p;
                    opaque = o;
                }
            }
        }
        Err(Error::Format(format!(
            "root://{host}: too many redirects opening {path:?}"
        )))
    }

    /// Send a request (24-byte header + `data`) and return the collected
    /// response payload, resolving `kXR_wait` (retry) and surfacing errors.
    fn request(&mut self, reqid: u16, body: [u8; 16], data: &[u8]) -> Result<(u16, Vec<u8>)> {
        self.stream_id = self.stream_id.wrapping_add(1);
        let mut hdr = Vec::with_capacity(24 + data.len());
        hdr.extend_from_slice(&self.stream_id.to_be_bytes());
        hdr.extend_from_slice(&reqid.to_be_bytes());
        hdr.extend_from_slice(&body);
        hdr.extend_from_slice(&(data.len() as i32).to_be_bytes());
        hdr.extend_from_slice(data);
        self.stream
            .write_all(&hdr)
            .map_err(|e| io_err("sending request", e))?;
        self.read_response()
    }

    /// Read one logical response, concatenating `kXR_oksofar` continuations.
    fn read_response(&mut self) -> Result<(u16, Vec<u8>)> {
        let mut payload = Vec::new();
        loop {
            let mut head = [0u8; 8];
            self.read_exact(&mut head)?;
            let status = u16::from_be_bytes([head[2], head[3]]);
            let dlen = i32::from_be_bytes([head[4], head[5], head[6], head[7]]).max(0) as usize;
            let mut chunk = vec![0u8; dlen];
            self.read_exact(&mut chunk)?;
            if status == KXR_OKSOFAR {
                payload.extend_from_slice(&chunk);
                continue;
            }
            payload.extend_from_slice(&chunk);
            return Ok((status, payload));
        }
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        self.stream
            .read_exact(buf)
            .map_err(|e| io_err("reading response", e))
    }

    fn login(&mut self) -> Result<()> {
        // Body: pid(0), username[8], reserved, ability, capver, role. capver is
        // the protocol version (4) *without* the async-capability bit (0x80), so
        // the server always answers reads synchronously — this client does not
        // implement the async `kXR_waitresp`/`kXR_attn` path.
        let mut body = [0u8; 16];
        body[4..10].copy_from_slice(b"nobody");
        body[14] = 0x04;
        let (status, payload) = self.request(KXR_LOGIN, body, &[])?;
        if status != KXR_OK {
            return Err(server_error("login", status, &payload));
        }
        // Payload: 16-byte session id, then (if the server requires auth) a
        // security spec such as "&P=gsi...&P=unix". Do the credential-free `unix`
        // handshake when it is offered; anything else is unsupported.
        if payload.len() > 16 {
            let sec = &payload[16..];
            if find_subslice(sec, b"&P=unix").is_some() || find_subslice(sec, b"P=unix").is_some() {
                self.auth_unix()?;
            } else {
                let spec = String::from_utf8_lossy(sec);
                return Err(Error::Format(format!(
                    "root://: server requires authentication this client does not support \
                     (only `unix` is implemented); offered: {spec}"
                )));
            }
        }
        Ok(())
    }

    fn auth_unix(&mut self) -> Result<()> {
        // Body: reserved[12] + credtype[4]="unix"; credential is the tag itself.
        let mut body = [0u8; 16];
        body[12..16].copy_from_slice(b"unix");
        let (status, payload) = self.request(KXR_AUTH, body, b"unix\0")?;
        if status != KXR_OK {
            return Err(server_error("unix authentication", status, &payload));
        }
        Ok(())
    }

    fn try_open(&mut self, path: &str) -> Result<OpenOutcome> {
        // Body: mode(0) + options + reserved[12]; data = path.
        let mut body = [0u8; 16];
        body[0..2].copy_from_slice(&0u16.to_be_bytes()); // mode
        body[2..4].copy_from_slice(&KXR_OPEN_READ.to_be_bytes());
        let (status, payload) = self.request(KXR_OPEN, body, path.as_bytes())?;
        match status {
            KXR_OK => {
                if payload.len() < 4 {
                    return Err(Error::Format("root://: short open response".into()));
                }
                self.fhandle.copy_from_slice(&payload[0..4]);
                Ok(OpenOutcome::Opened)
            }
            KXR_REDIRECT => {
                let (host, port, opaque) = parse_redirect(&payload)?;
                Ok(OpenOutcome::Redirect { host, port, opaque })
            }
            KXR_WAIT => {
                let secs = payload
                    .get(0..4)
                    .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]).max(0) as u64)
                    .unwrap_or(1)
                    .min(10);
                std::thread::sleep(Duration::from_secs(secs));
                self.try_open(path)
            }
            _ => Err(server_error(&format!("opening {path}"), status, &payload)),
        }
    }

    /// Stat the open file by its handle (`fstat`) and return its size. The
    /// `kXR_stat` response is `"id size flags modtime"`.
    fn fstat(&mut self) -> Result<u64> {
        // Body: reserved[11] + options(0) + fhandle[4]; no path (dlen 0) => fstat.
        let mut body = [0u8; 16];
        body[12..16].copy_from_slice(&self.fhandle);
        let (status, payload) = self.request(KXR_STAT, body, &[])?;
        if status != KXR_OK {
            return Err(server_error("stat", status, &payload));
        }
        let s = String::from_utf8_lossy(&payload);
        s.split_whitespace()
            .nth(1)
            .and_then(|f| f.parse().ok())
            .ok_or_else(|| Error::Format(format!("root://: unparseable stat {:?}", s.trim())))
    }

    fn read_at(&mut self, offset: u64, len: usize) -> Result<Bytes> {
        // Body: fhandle[4] + offset(i64) + rlen(i32).
        let mut body = [0u8; 16];
        body[0..4].copy_from_slice(&self.fhandle);
        body[4..12].copy_from_slice(&(offset as i64).to_be_bytes());
        body[12..16].copy_from_slice(&(len as i32).to_be_bytes());
        let (status, payload) = self.request(KXR_READ, body, &[])?;
        if status != KXR_OK {
            return Err(server_error("reading", status, &payload));
        }
        if payload.len() != len {
            return Err(Error::UnexpectedEof {
                needed: len,
                available: payload.len(),
            });
        }
        Ok(Bytes::from(payload))
    }

    fn close(&mut self) {
        let mut body = [0u8; 16];
        body[0..4].copy_from_slice(&self.fhandle);
        let _ = self.request(KXR_CLOSE, body, &[]);
    }
}

enum OpenOutcome {
    Opened,
    Redirect {
        host: String,
        port: u16,
        opaque: String,
    },
}

/// A ROOT file accessed over XRootD (`root://`) with byte-range reads.
pub struct XrootdSource {
    conn: Mutex<Conn>,
    size: u64,
    url: String,
}

impl std::fmt::Debug for XrootdSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XrootdSource")
            .field("url", &self.url)
            .field("size", &self.size)
            .finish()
    }
}

impl XrootdSource {
    /// Connect to `url` (`root://host[:port]//path`), log in with `unix` auth,
    /// and open the file for reading.
    pub fn open(url: &str) -> Result<XrootdSource> {
        let parsed = XrootdUrl::parse(url)?;
        let (conn, size) = Conn::open(&parsed.host, parsed.port, &parsed.path)?;
        Ok(XrootdSource {
            conn: Mutex::new(conn),
            size,
            url: url.to_string(),
        })
    }
}

impl ByteSource for XrootdSource {
    fn len(&self) -> u64 {
        self.size
    }

    fn read_at(&self, offset: u64, len: usize) -> Result<Bytes> {
        if len == 0 {
            return Ok(Bytes::new());
        }
        offset
            .checked_add(len as u64)
            .filter(|&e| e <= self.size)
            .ok_or_else(|| Error::UnexpectedEof {
                needed: len,
                available: self.size.saturating_sub(offset) as usize,
            })?;
        // One request/response at a time keeps the shared stream in step.
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.read_at(offset, len)
    }
}

impl Drop for XrootdSource {
    fn drop(&mut self) {
        if let Ok(mut conn) = self.conn.lock() {
            conn.close();
        }
    }
}

/// Send the initial 20-byte client handshake and consume the 16-byte reply.
fn handshake(stream: &mut TcpStream) -> Result<()> {
    // Five big-endian int32: 0, 0, 0, 4, 2012.
    let mut req = [0u8; 20];
    req[12..16].copy_from_slice(&4i32.to_be_bytes());
    req[16..20].copy_from_slice(&2012i32.to_be_bytes());
    stream
        .write_all(&req)
        .map_err(|e| io_err("sending handshake", e))?;
    let mut reply = [0u8; 16];
    stream
        .read_exact(&mut reply)
        .map_err(|e| io_err("reading handshake reply", e))?;
    Ok(())
}

/// Parse a `kXR_redirect` payload: `int32 port` then `host[:port][?opaque]`.
/// Returns the host, the port (the leading int32), and the opaque capability
/// token (the `?…` tail, empty if none) to forward on the data server's re-open.
fn parse_redirect(payload: &[u8]) -> Result<(String, u16, String)> {
    if payload.len() < 4 {
        return Err(Error::Format("root://: short redirect response".into()));
    }
    let port = i32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let rest = String::from_utf8_lossy(&payload[4..]);
    let (hostport, opaque) = rest.split_once('?').unwrap_or((rest.as_ref(), ""));
    let host = hostport.split(':').next().unwrap_or("").trim().to_string();
    if host.is_empty() || !(0..=65535).contains(&port) {
        return Err(Error::Format(format!(
            "root://: bad redirect to {rest:?}:{port}"
        )));
    }
    Ok((host, port as u16, opaque.to_string()))
}

/// Turn a `kXR_error`/unexpected status into an [`Error`]. The error payload is
/// `int32 errnum` then a message string.
fn server_error(ctx: &str, status: u16, payload: &[u8]) -> Error {
    if status == KXR_ERROR && payload.len() >= 4 {
        let msg = String::from_utf8_lossy(&payload[4..]);
        let msg = msg.trim_end_matches('\0');
        Error::Format(format!("root:// {ctx}: {msg}"))
    } else {
        Error::Format(format!("root:// {ctx}: unexpected status {status}"))
    }
}

fn io_err(ctx: &str, e: std::io::Error) -> Error {
    Error::Io {
        kind: e.kind(),
        message: format!("{ctx}: {e}"),
    }
}

/// First index of `needle` in `hay` (small `sec`-spec scans).
fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_root_urls() {
        let u = XrootdUrl::parse("root://eospublic.cern.ch//eos/root-eos/hsimple.root").unwrap();
        assert_eq!(u.host, "eospublic.cern.ch");
        assert_eq!(u.port, DEFAULT_PORT);
        assert_eq!(u.path, "/eos/root-eos/hsimple.root");

        let u = XrootdUrl::parse("root://user@host.example:1095//a/b.root").unwrap();
        assert_eq!(u.host, "host.example");
        assert_eq!(u.port, 1095);
        assert_eq!(u.path, "/a/b.root");

        assert!(XrootdUrl::parse("https://x/y").is_err());
        assert!(XrootdUrl::parse("root://host").is_err()); // no path
    }

    #[test]
    fn parses_a_redirect_payload() {
        let mut p = 1095i32.to_be_bytes().to_vec();
        p.extend_from_slice(b"dataserver.cern.ch?authz=abc&xrd.k=v");
        let (h, port, opaque) = parse_redirect(&p).unwrap();
        assert_eq!(h, "dataserver.cern.ch");
        assert_eq!(port, 1095);
        assert_eq!(opaque, "authz=abc&xrd.k=v");
    }
}
