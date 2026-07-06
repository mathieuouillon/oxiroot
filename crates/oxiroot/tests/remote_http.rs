//! Remote reads over HTTP(S) byte-range requests (`http` feature).
//!
//! A tiny in-process HTTP/1.1 server serves a fixture `.root` file with
//! `Range` support and records what it was asked for. The test then reads the
//! file through [`RFile::open_url`] and asserts (a) the parsed content matches
//! a local read byte-for-byte, and (b) only ranges were fetched — the file was
//! never downloaded whole, the way ROOT and uproot read remote files.
#![cfg(feature = "http")]

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use oxiroot::prelude::*;
use oxiroot::tree::TTree;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// What the server observed, so the test can assert range-only, lazy access.
#[derive(Default)]
struct Stats {
    requests: u32,
    bytes_served: u64,
    max_response: u64,
    all_ranged: bool,
}

struct RangeServer {
    url: String,
    stats: Arc<Mutex<Stats>>,
}

impl RangeServer {
    /// Serve `data` on a background thread; return the server with its URL.
    fn start(data: Vec<u8>) -> RangeServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let stats = Arc::new(Mutex::new(Stats {
            all_ranged: true,
            ..Stats::default()
        }));
        let data = Arc::new(data);
        let stats_bg = Arc::clone(&stats);
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(stream) = conn else { continue };
                let data = Arc::clone(&data);
                let stats = Arc::clone(&stats_bg);
                std::thread::spawn(move || handle(stream, &data, &stats));
            }
        });
        RangeServer {
            url: format!("http://127.0.0.1:{port}/file.root"),
            stats,
        }
    }

    fn bytes_served(&self) -> u64 {
        self.stats.lock().unwrap().bytes_served
    }
    fn max_response(&self) -> u64 {
        self.stats.lock().unwrap().max_response
    }
    fn all_ranged(&self) -> bool {
        self.stats.lock().unwrap().all_ranged
    }
    fn requests(&self) -> u32 {
        self.stats.lock().unwrap().requests
    }
}

/// Handle one (keep-alive) connection: read requests, answer each with a 206.
fn handle(stream: TcpStream, data: &[u8], stats: &Mutex<Stats>) {
    let read_half = stream.try_clone().expect("clone");
    let mut reader = BufReader::new(read_half);
    let mut stream = stream;
    loop {
        // Request line.
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return; // client closed
        }
        // Headers until the blank line; capture `Range: bytes=A-B`.
        let mut range: Option<(u64, u64)> = None;
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h).unwrap_or(0) == 0 {
                return;
            }
            if h == "\r\n" || h == "\n" {
                break;
            }
            if let Some(v) = h.to_ascii_lowercase().strip_prefix("range:") {
                if let Some(spec) = v.trim().strip_prefix("bytes=") {
                    if let Some((a, b)) = spec.trim().split_once('-') {
                        if let (Ok(a), Ok(b)) = (a.trim().parse(), b.trim().parse()) {
                            range = Some((a, b));
                        }
                    }
                }
            }
        }

        let total = data.len() as u64;
        let (start, end, status) = match range {
            Some((a, b)) => (a, b.min(total.saturating_sub(1)), 206),
            None => (0, total.saturating_sub(1), 200),
        };
        let body = &data[start as usize..=end as usize];
        {
            let mut s = stats.lock().unwrap();
            s.requests += 1;
            s.bytes_served += body.len() as u64;
            s.max_response = s.max_response.max(body.len() as u64);
            s.all_ranged &= range.is_some();
        }
        let reason = if status == 206 {
            "Partial Content"
        } else {
            "OK"
        };
        let head = format!(
            "HTTP/1.1 {status} {reason}\r\nAccept-Ranges: bytes\r\n\
             Content-Range: bytes {start}-{end}/{total}\r\nContent-Length: {}\r\n\
             Connection: keep-alive\r\n\r\n",
            body.len()
        );
        if stream.write_all(head.as_bytes()).is_err() || stream.write_all(body).is_err() {
            return;
        }
        let _ = stream.flush();
    }
}

#[test]
fn reads_a_ttree_over_http_without_downloading_it_whole() {
    let bytes = std::fs::read(fixture("tree_flat.root")).expect("read fixture");
    let size = bytes.len() as u64;

    // Local reference: the tree, its branches, and one branch's values.
    let local = RFile::from_bytes(bytes.clone()).expect("local open");
    let local_keys: Vec<String> = local.keys().iter().map(|k| k.name.clone()).collect();
    let tkey = local
        .keys()
        .iter()
        .find(|k| k.class_name == "TTree")
        .expect("a TTree key");
    let tname = tkey.name.clone();
    let ltree = TTree::open(&local, &tname).expect("local tree");
    let branch = ltree.branch_names()[0].to_string();
    let local_vals = format!(
        "{:?}",
        ltree.read_branch(&local, &branch).expect("local branch")
    );

    // Serve it and read it back over HTTP range requests.
    let server = RangeServer::start(bytes);
    let remote = RFile::open_url(&server.url).expect("open_url");

    // Opening parsed only the header/dir/keys — not the whole file.
    let after_open = server.bytes_served();
    assert!(
        after_open < size,
        "open fetched {after_open} of {size} bytes — should read only metadata"
    );

    // Same container.
    assert_eq!(remote.size(), size);
    let remote_keys: Vec<String> = remote.keys().iter().map(|k| k.name.clone()).collect();
    assert_eq!(remote_keys, local_keys, "same keys over HTTP");

    // Same tree data, fetched as baskets.
    let rtree = TTree::open(&remote, &tname).expect("remote tree");
    assert_eq!(rtree.num_entries(), ltree.num_entries());
    let remote_vals = format!(
        "{:?}",
        rtree.read_branch(&remote, &branch).expect("remote branch")
    );
    assert_eq!(remote_vals, local_vals, "branch values match over HTTP");

    // Reading one branch never fetched the whole file (other branches' baskets
    // and the unused objects were skipped), and no single response was the whole
    // file — every request was a bounded range.
    assert!(
        server.bytes_served() < size,
        "served {} of {size} bytes total — a branch read must stay lazy",
        server.bytes_served()
    );
    assert!(
        server.max_response() < size,
        "a single response returned {} of {size} bytes — never download whole",
        server.max_response()
    );
    assert!(
        server.all_ranged(),
        "every request must carry a Range header"
    );
    assert!(server.requests() >= 3, "expected several ranged requests");
}
