//! Reads SWTOR's .tor archives: the MYP container, compressed entries and the filename hash dictionary.
//! Depends on nothing that touches SQLite or images, so a tool builder can take it alone.

pub mod archive;
pub mod compress;
pub mod error;
pub mod hash;

pub use archive::{Archive, Entry};
pub use error::{Error, Result};
pub use hash::{Dictionary, filename_hash};
