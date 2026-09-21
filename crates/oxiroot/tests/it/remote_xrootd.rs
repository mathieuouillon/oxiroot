//! Remote reads over the XRootD `root://` protocol (`xrootd` feature).
//!
//! A tiny in-process XRootD server speaks enough of the binary protocol —
//! handshake, login, `unix` auth, open, fstat, read, close — to serve a fixture
//! `.root` file, so the client is tested hermetically (no network). The wire
//! framing mirrors what was verified against `root://eospublic.cern.ch`; the
//! redirect + capability path is exercised by the `#[ignore]`d live test below.
#![cfg(feature = "xrootd")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;

use oxiroot::prelude::*;
use oxiroot::tree::TreeReader;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

// Opcodes / statuses used by the mock.
const KXR_AUTH: u16 = 3000;
const KXR_CLOSE: u16 = 3003;
const KXR_LOGIN: u16 = 3007;
const KXR_OPEN: u16 = 3010;
const KXR_READ: u16 = 3013;
const KXR_STAT: u16 = 3017;
const KXR_OK: u16 = 0;

/// Serve `data` as a single XRootD data server on a background thread.
fn serve(data: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let data = Arc::new(data);
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(stream) = conn else { continue };
            let data = Arc::clone(&data);
            std::thread::spawn(move || handle(stream, &data));
        }
    });
    format!("root://127.0.0.1:{port}//file.root")
}

fn reply(stream: &mut TcpStream, streamid: [u8; 2], status: u16, payload: &[u8]) -> bool {
    let mut msg = Vec::with_capacity(8 + payload.len());
    msg.extend_from_slice(&streamid);
    msg.extend_from_slice(&status.to_be_bytes());
    msg.extend_from_slice(&(payload.len() as i32).to_be_bytes());
    msg.extend_from_slice(payload);
    stream.write_all(&msg).is_ok()
}

fn handle(mut stream: TcpStream, data: &[u8]) {
    // Handshake: read the 20-byte client handshake, reply 16 bytes (the client
    // ignores the content: two pad int32, protover, server flags = data server).
    let mut hs = [0u8; 20];
    if stream.read_exact(&mut hs).is_err() {
        return;
    }
    let mut reply16 = [0u8; 16];
    reply16[8..12].copy_from_slice(&0x0000_0511i32.to_be_bytes()); // protover
    reply16[12..16].copy_from_slice(&1i32.to_be_bytes()); // kXR_isServer (data server)
    if stream.write_all(&reply16).is_err() {
        return;
    }

    loop {
        let mut head = [0u8; 24];
        if stream.read_exact(&mut head).is_err() {
            return;
        }
        let streamid = [head[0], head[1]];
        let reqid = u16::from_be_bytes([head[2], head[3]]);
        let body = &head[4..20];
        let dlen = i32::from_be_bytes([head[20], head[21], head[22], head[23]]).max(0) as usize;
        let mut reqdata = vec![0u8; dlen];
        if stream.read_exact(&mut reqdata).is_err() {
            return;
        }

        let ok = match reqid {
            KXR_LOGIN => {
                // 16-byte session id + a security spec offering `unix`.
                let mut pl = vec![0u8; 16];
                pl.extend_from_slice(b"&P=unix");
                reply(&mut stream, streamid, KXR_OK, &pl)
            }
            KXR_AUTH => reply(&mut stream, streamid, KXR_OK, &[]),
            KXR_OPEN => reply(&mut stream, streamid, KXR_OK, &[0, 0, 0, 0]), // fhandle
            KXR_STAT => {
                // "id size flags modtime"
                let s = format!("0 {} 0 0", data.len());
                reply(&mut stream, streamid, KXR_OK, s.as_bytes())
            }
            KXR_READ => {
                // Body: fhandle[4] + offset(i64) + rlen(i32).
                let offset = i64::from_be_bytes(body[4..12].try_into().unwrap()) as usize;
                let rlen = i32::from_be_bytes(body[12..16].try_into().unwrap()).max(0) as usize;
                let end = (offset + rlen).min(data.len());
                let chunk = data.get(offset..end).unwrap_or(&[]);
                reply(&mut stream, streamid, KXR_OK, chunk)
            }
            KXR_CLOSE => reply(&mut stream, streamid, KXR_OK, &[]),
            _ => reply(&mut stream, streamid, KXR_OK, &[]),
        };
        if !ok {
            return;
        }
    }
}

#[test]
fn reads_a_ttree_over_xrootd() {
    let bytes = std::fs::read(fixture("tree_flat.root")).expect("read fixture");

    // Local reference.
    let local = FileReader::from_bytes(bytes.clone()).expect("local open");
    let tkey = local
        .keys()
        .iter()
        .find(|k| k.class_name == "TTree")
        .expect("a TTree key");
    let tname = tkey.name.clone();
    let ltree = TreeReader::open(&local, &tname).expect("local tree");
    let branch = ltree.branch_names()[0].to_string();
    let local_vals = format!(
        "{:?}",
        ltree.read_branch(&local, &branch).expect("local branch")
    );

    // Same file over the XRootD protocol.
    let url = serve(bytes.clone());
    let remote = FileReader::open_url(&url).expect("open_url root://");
    assert_eq!(remote.size(), bytes.len() as u64, "size via fstat");
    let rkeys: Vec<String> = remote.keys().iter().map(|k| k.name.clone()).collect();
    let lkeys: Vec<String> = local.keys().iter().map(|k| k.name.clone()).collect();
    assert_eq!(rkeys, lkeys, "same keys over root://");

    let rtree = TreeReader::open(&remote, &tname).expect("remote tree");
    assert_eq!(rtree.num_entries(), ltree.num_entries());
    let remote_vals = format!(
        "{:?}",
        rtree.read_branch(&remote, &branch).expect("remote branch")
    );
    assert_eq!(remote_vals, local_vals, "branch values match over root://");
}

/// Live read of a real public file from CERN's EOS over `root://` (unix auth,
/// redirect + capability, lazy reads). Ignored by default: it needs network and
/// the public data servers are slow/intermittent from outside CERN. Run with
/// `cargo test -p oxiroot --features xrootd -- --ignored xrootd_live`.
#[test]
#[ignore]
fn xrootd_live_eospublic() {
    let url = "root://eospublic.cern.ch//eos/root-eos/hsimple.root";
    let f = FileReader::open_url(url).expect("open eospublic");
    assert!(f.size() > 100_000, "hsimple.root is ~400 KiB");
    let names: Vec<String> = f.keys().iter().map(|k| k.name.clone()).collect();
    assert!(names.contains(&"hpx".to_string()), "keys: {names:?}");
}
