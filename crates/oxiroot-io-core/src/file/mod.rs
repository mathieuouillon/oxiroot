//! The ROOT (TFile) on-disk container: header, keys, directories, free list,
//! and the [`RFile`] reading entry point.

mod directory;
mod free;
mod header;
#[cfg(feature = "http")]
mod http;
mod key;
mod rfile;
mod source;
mod writer;
#[cfg(feature = "xrootd")]
mod xrootd;

pub use directory::Directory;
pub use free::{read_free, FreeSegment};
pub use header::{FileHeader, TUuid, BIG_FILE_VERSION, MAGIC};
pub use key::{TDatime, TKey};
pub use rfile::RFile;
#[cfg(feature = "mmap")]
pub use source::MmapSource;
pub use source::{ByteSource, BytesSource, FileSource};
pub use writer::{
    dir_record_total, guard_small_format, key_len, key_len_fmt, seek_value, seek_zero,
    update_root_file, write_dir_record_fmt, write_key_header, write_key_header_cycle,
    write_key_header_fmt, write_key_list_fmt, write_root_file, write_root_file_with_dirs,
    write_root_file_with_dirs_threshold, write_root_file_with_streamers,
    write_root_file_with_streamers_threshold, ObjectRecord, Subdir, KSTART_BIG_FILE,
};
#[cfg(feature = "xrootd")]
pub use xrootd::XrootdSource;
