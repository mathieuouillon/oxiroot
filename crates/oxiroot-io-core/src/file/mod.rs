//! The ROOT (TFile) on-disk container: header, keys, directories, free list,
//! and the [`RFile`] reading entry point.

mod directory;
mod free;
mod header;
mod key;
mod rfile;
mod writer;

pub use directory::Directory;
pub use free::{read_free, FreeSegment};
pub use header::{FileHeader, TUuid, BIG_FILE_VERSION, MAGIC};
pub use key::{TDatime, TKey};
pub use rfile::RFile;
pub use writer::{
    guard_small_format, key_len, key_len_fmt, update_root_file, write_key_header,
    write_key_header_cycle, write_key_header_fmt, write_root_file, write_root_file_with_dirs,
    write_root_file_with_streamers, ObjectRecord, Subdir, KSTART_BIG_FILE,
};
