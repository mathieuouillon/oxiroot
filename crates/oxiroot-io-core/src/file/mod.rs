//! The ROOT (TFile) on-disk container: header, keys, directories, free list,
//! and the [`FileReader`] reading entry point.

mod container;
mod directory;
mod free;
mod header;
#[cfg(feature = "http")]
mod http;
mod key;
mod reader;
mod source;
mod writer;
#[cfg(feature = "xrootd")]
mod xrootd;

pub(crate) use container::STREAMER_INFO_KEY_LEN;
pub use container::{
    compress_if_smaller, ContainerWriter, DirId, DATIME, FILE_VERSION, KSTART_BIG_FILE,
};
pub use directory::Directory;
pub use free::{read_free, FreeSegment};
pub use header::{FileHeader, Uuid, BIG_FILE_VERSION, MAGIC};
pub use key::{Datime, Key};
pub use reader::{find_key, split_cycle, FileReader};
#[cfg(feature = "mmap")]
pub use source::MmapSource;
pub use source::{ByteSource, BytesSource, FileSource};
pub use writer::{FileWriter, SubdirWriter};
#[cfg(feature = "xrootd")]
pub use xrootd::XrootdSource;
