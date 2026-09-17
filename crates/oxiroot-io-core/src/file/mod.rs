//! The ROOT (TFile) on-disk container: header, keys, directories, free list,
//! and the [`RFile`] reading entry point.

mod builder;
mod container;
mod directory;
mod free;
mod header;
#[cfg(feature = "http")]
mod http;
mod key;
mod rfile;
mod source;
#[cfg(feature = "xrootd")]
mod xrootd;

pub use builder::{RootFile, SubdirBuilder};
pub use container::{
    compress_if_smaller, ContainerWriter, DirId, DATIME, FILE_VERSION, KSTART_BIG_FILE,
};
pub use directory::Directory;
pub use free::{read_free, FreeSegment};
pub use header::{FileHeader, TUuid, BIG_FILE_VERSION, MAGIC};
pub use key::{TDatime, TKey};
pub use rfile::RFile;
#[cfg(feature = "mmap")]
pub use source::MmapSource;
pub use source::{ByteSource, BytesSource, FileSource};
#[cfg(feature = "xrootd")]
pub use xrootd::XrootdSource;
