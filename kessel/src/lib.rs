//! Kessel - SWTOR data miner library
//!
//! Extracts game objects from .tor archives to SQLite.

// Archive, GOM, and text layers live in their own crates; re-exported at
// their old paths so `crate::myp`, `kessel::pbuk`, etc. keep resolving.
pub use kessel_gom::{gom_reader, node, pbuk, prototypes_info};
pub use kessel_text::{grammar, stb, xml_utf16};
pub use kessel_tor::myp;

pub mod buckets_info;
pub mod db;
pub mod dds;
pub mod gifts;
pub mod gom_schema;
pub mod gsf_stat_dictionary;
pub mod hash;
pub mod icon_overrides;
pub mod quest;
pub mod schema;
pub mod scpt;
pub mod xml_parser;
