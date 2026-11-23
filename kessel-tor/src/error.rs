//! What can go wrong reading an archive.

use kessel_bytes::OutOfBounds;
use std::path::PathBuf;

/// An archive could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The file could not be opened or read.
    #[error("reading {path}")]
    Io {
        /// The archive or dictionary file.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// The file does not start with `MYP`.
    #[error("not an MYP archive: bad magic")]
    BadMagic,
    /// The file ended inside a structure.
    #[error("truncated {what}")]
    Truncated {
        /// The structure being read.
        what: &'static str,
        /// Where it ran out.
        #[source]
        source: OutOfBounds,
    },
    /// The chain of entry tables points back at a table already read.
    #[error("entry table chain loops at offset {offset}")]
    TableLoop {
        /// The repeated table offset.
        offset: u64,
    },
    /// A compressed entry did not decompress.
    #[error("{codec} entry did not decode")]
    Decompress {
        /// `"zstd"` or `"zlib"`.
        codec: &'static str,
        /// The decoder's failure.
        #[source]
        source: std::io::Error,
    },
}

/// The result of reading an archive.
pub type Result<T, E = Error> = std::result::Result<T, E>;
