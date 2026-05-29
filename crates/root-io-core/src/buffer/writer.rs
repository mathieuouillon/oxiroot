//! Growable write buffer for ROOT structures, with back-patching.
//!
//! Correct ROOT output is full of "write a placeholder now, fill in the real
//! value once the size/offset is known": object byte counts, TFile seek
//! pointers, RNTuple envelope and frame lengths. [`WBuffer`] provides reserved
//! regions ([`Patch`]) and a byte-count helper ([`CountToken`]) for exactly
//! this pattern.

use super::reader::K_BYTE_COUNT_MASK;

/// Handle to a reserved, fixed-size region awaiting a later value.
#[derive(Debug, Clone, Copy)]
pub struct Patch {
    offset: usize,
    len: usize,
}

/// Handle returned by [`WBuffer::begin_object`] and consumed by
/// [`WBuffer::end_object`] to back-patch a streamed object's byte count.
#[derive(Debug, Clone, Copy)]
pub struct CountToken {
    count_pos: usize,
}

/// A growable big-/little-endian write buffer.
#[derive(Debug, Default, Clone)]
pub struct WBuffer {
    buf: Vec<u8>,
}

macro_rules! num_writer {
    ($(#[$doc:meta])* $name:ident, $ty:ty, $to:ident) => {
        $(#[$doc])*
        pub fn $name(&mut self, v: $ty) {
            self.buf.extend_from_slice(&v.$to());
        }
    };
}

impl WBuffer {
    /// Create an empty buffer.
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Create an empty buffer with reserved capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
        }
    }

    /// Number of bytes written so far (also the next write offset).
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether nothing has been written yet.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Borrow the bytes written so far.
    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    /// Consume the buffer, returning the written bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }

    /// Append a single byte.
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    /// Append a single signed byte.
    pub fn i8(&mut self, v: i8) {
        self.buf.push(v as u8);
    }

    /// Append raw bytes.
    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    num_writer!(/// Append a big-endian `u16`.
        be_u16, u16, to_be_bytes);
    num_writer!(/// Append a big-endian `u32`.
        be_u32, u32, to_be_bytes);
    num_writer!(/// Append a big-endian `u64`.
        be_u64, u64, to_be_bytes);
    num_writer!(/// Append a big-endian `i16`.
        be_i16, i16, to_be_bytes);
    num_writer!(/// Append a big-endian `i32`.
        be_i32, i32, to_be_bytes);
    num_writer!(/// Append a big-endian `i64`.
        be_i64, i64, to_be_bytes);
    num_writer!(/// Append a big-endian `f32`.
        be_f32, f32, to_be_bytes);
    num_writer!(/// Append a big-endian `f64`.
        be_f64, f64, to_be_bytes);

    num_writer!(/// Append a little-endian `u16`.
        le_u16, u16, to_le_bytes);
    num_writer!(/// Append a little-endian `u32`.
        le_u32, u32, to_le_bytes);
    num_writer!(/// Append a little-endian `u64`.
        le_u64, u64, to_le_bytes);
    num_writer!(/// Append a little-endian `i64`.
        le_i64, i64, to_le_bytes);
    num_writer!(/// Append a little-endian `f32`.
        le_f32, f32, to_le_bytes);
    num_writer!(/// Append a little-endian `f64`.
        le_f64, f64, to_le_bytes);

    /// Append a ROOT-encoded string (1-byte length, or `0xFF` + big-endian
    /// `u32` length, then the UTF-8 bytes).
    pub fn string(&mut self, s: &str) {
        let b = s.as_bytes();
        if b.len() < 255 {
            self.buf.push(b.len() as u8);
        } else {
            self.buf.push(255);
            self.buf.extend_from_slice(&(b.len() as u32).to_be_bytes());
        }
        self.buf.extend_from_slice(b);
    }

    /// Reserve `n` zero bytes for later patching and return a handle to them.
    pub fn reserve(&mut self, n: usize) -> Patch {
        let offset = self.buf.len();
        self.buf.resize(offset + n, 0);
        Patch { offset, len: n }
    }

    /// The offset a [`Patch`] points at (useful when computing seek pointers).
    pub fn patch_offset(&self, p: Patch) -> usize {
        p.offset
    }

    /// Overwrite a reserved 4-byte region with a big-endian `u32`.
    pub fn patch_be_u32(&mut self, p: Patch, v: u32) {
        assert_eq!(p.len, 4, "patch_be_u32 on a {}-byte region", p.len);
        self.buf[p.offset..p.offset + 4].copy_from_slice(&v.to_be_bytes());
    }

    /// Overwrite a reserved 8-byte region with a big-endian `u64`.
    pub fn patch_be_u64(&mut self, p: Patch, v: u64) {
        assert_eq!(p.len, 8, "patch_be_u64 on a {}-byte region", p.len);
        self.buf[p.offset..p.offset + 8].copy_from_slice(&v.to_be_bytes());
    }

    /// Begin a streamed object: writes a placeholder byte count and the class
    /// version, ROOT-style. Pair with [`WBuffer::end_object`].
    pub fn begin_object(&mut self, version: u16) -> CountToken {
        let count_pos = self.buf.len();
        self.buf.extend_from_slice(&[0u8; 4]);
        self.buf.extend_from_slice(&version.to_be_bytes());
        CountToken { count_pos }
    }

    /// Finish a streamed object, back-patching the byte count to cover
    /// everything written since [`WBuffer::begin_object`] (excluding the count
    /// word itself), with [`K_BYTE_COUNT_MASK`] set.
    pub fn end_object(&mut self, tok: CountToken) {
        let nbytes = (self.buf.len() - (tok.count_pos + 4)) as u32;
        let word = nbytes | K_BYTE_COUNT_MASK;
        self.buf[tok.count_pos..tok.count_pos + 4].copy_from_slice(&word.to_be_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::super::reader::RBuffer;
    use super::*;

    #[test]
    fn numeric_round_trips_through_reader() {
        let mut w = WBuffer::new();
        w.be_u32(0x1234_5678);
        w.le_u64(0x0102_0304_0506_0708);
        w.be_f64(3.5);
        let bytes = w.into_vec();

        let mut r = RBuffer::new(&bytes);
        assert_eq!(r.be_u32().unwrap(), 0x1234_5678);
        assert_eq!(r.le_u64().unwrap(), 0x0102_0304_0506_0708);
        assert_eq!(r.be_f64().unwrap(), 3.5);
    }

    #[test]
    fn string_round_trips_short_and_long() {
        let long = "x".repeat(300);
        let mut w = WBuffer::new();
        w.string("hello");
        w.string(&long);
        let bytes = w.into_vec();

        let mut r = RBuffer::new(&bytes);
        assert_eq!(r.string().unwrap(), "hello");
        assert_eq!(r.string().unwrap(), long);
    }

    #[test]
    fn object_byte_count_round_trips() {
        let mut w = WBuffer::new();
        let tok = w.begin_object(3);
        w.be_u32(0xDEAD_BEEF);
        w.be_u16(0x0102);
        w.end_object(tok);
        let bytes = w.into_vec();

        let mut r = RBuffer::new(&bytes);
        let vh = r.read_version().unwrap();
        assert_eq!(vh.version, 3);
        // ROOT's byte count covers everything after the count word, i.e. the
        // 2-byte version plus the 6-byte payload (u32 + u16) = 8.
        assert_eq!(vh.byte_count, Some(8));
        assert_eq!(vh.end, Some(bytes.len()));
        assert_eq!(r.be_u32().unwrap(), 0xDEAD_BEEF);
        assert_eq!(r.be_u16().unwrap(), 0x0102);
        r.check_byte_count(&vh).unwrap();
    }

    #[test]
    fn reserve_and_patch() {
        let mut w = WBuffer::new();
        let p = w.reserve(4);
        w.be_u32(0xAABB_CCDD);
        w.patch_be_u32(p, 0x1122_3344);
        let bytes = w.into_vec();

        let mut r = RBuffer::new(&bytes);
        assert_eq!(r.be_u32().unwrap(), 0x1122_3344);
        assert_eq!(r.be_u32().unwrap(), 0xAABB_CCDD);
    }
}
