//! Decodes the GOM: buckets, object headers, the value tree, prototypes and the client schema.
//! Depends on kessel-tor only.

pub mod bucket;
pub mod fields;
pub mod object;
pub mod prototype;
pub mod reader;
pub mod recover;
pub mod schema;
