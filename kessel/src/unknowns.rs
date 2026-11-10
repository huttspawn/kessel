//! Unknown pattern tracker for iterative format discovery
//!
//! Captures anything we don't fully understand during extraction:
//! - Unknown FQN prefixes
//! - Failed decompression
//! - Missing expected fields
//! - Unrecognized byte patterns
//!
//! Output: JSONL file for analysis and ML training

use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

/// Types of unknown patterns we track
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
pub enum Unknown {
    /// FQN prefix not in our known list
    #[serde(rename = "unknown_prefix")]
    UnknownPrefix {
        prefix: String,
        fqn: String,
        sample_count: u32,
    },

    /// ZSTD decompression failed
    #[serde(rename = "decompress_failed")]
    DecompressFailed {
        fqn: String,
        header_hex: String,
        error: String,
    },

    /// Icon referenced but not found in any object
    #[serde(rename = "unmapped_icon")]
    UnmappedIcon {
        icon_name: String,
        source_file: String,
    },

    /// Object missing string_id when we expected one
    #[serde(rename = "missing_string_id")]
    MissingStringId {
        fqn: String,
        kind: String,
        has_icon: bool,
    },

    /// Unrecognized file in archive (not PBUK, DBLB, STB, DDS)
    #[serde(rename = "unknown_file_type")]
    UnknownFileType {
        path: String,
        magic_hex: String,
        size: usize,
    },

    /// MYP entry with hash but no known filename
    #[serde(rename = "orphan_hash")]
    OrphanHash {
        hash1: u32,
        hash2: u32,
        size: usize,
        compressed: bool,
    },

    /// Byte pattern we haven't decoded yet
    #[serde(rename = "unknown_pattern")]
    UnknownPattern {
        context: String,
        offset: usize,
        pattern_hex: String,
        surrounding_hex: String,
    },
}

/// Buffered JSONL writer for unknowns
pub struct UnknownsWriter {
    writer: Mutex<Option<BufWriter<File>>>,
    prefix_counts: Mutex<std::collections::HashMap<String, u32>>,
}

impl UnknownsWriter {
    /// Create a new unknowns writer
    pub fn new(path: &Path) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        Ok(Self {
            writer: Mutex::new(Some(BufWriter::new(file))),
            prefix_counts: Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Create a no-op writer (for when unknowns tracking is disabled)
    pub fn disabled() -> Self {
        Self {
            writer: Mutex::new(None),
            prefix_counts: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Record an unknown pattern
    pub fn record(&self, unknown: Unknown) {
        let mut writer_guard = self.writer.lock().unwrap();
        if let Some(ref mut writer) = *writer_guard {
            if let Ok(json) = serde_json::to_string(&unknown) {
                let _ = writeln!(writer, "{}", json);
            }
        }
    }

    /// Flush and finalize - writes summary of prefix counts
    pub fn finalize(&self) -> std::io::Result<()> {
        // Write final prefix counts
        let counts = self.prefix_counts.lock().unwrap();
        for (prefix, count) in counts.iter() {
            if *count > 1 {
                self.record(Unknown::UnknownPrefix {
                    prefix: prefix.clone(),
                    fqn: format!("(final count: {} occurrences)", count),
                    sample_count: *count,
                });
            }
        }

        // Flush writer
        let mut writer_guard = self.writer.lock().unwrap();
        if let Some(ref mut writer) = *writer_guard {
            writer.flush()?;
        }

        Ok(())
    }
}

/// What an extraction saw in the build's archives, for the report of what it did not read
/// (FR-KESSEL-026). The extraction notes each archive entry, each object it leaves out and each
/// string table it reads; `Database::write_unknowns_report` turns these into findings.
#[derive(Debug, Default)]
pub struct BuildFacts {
    /// The hash of every archive entry.
    pub entries: std::collections::HashSet<u64>,
    /// en-us string tables found by their content: hash to the archive holding it.
    pub string_tables: std::collections::BTreeMap<u64, String>,
    /// The hashes of the string tables the extraction read.
    pub tables_read: std::collections::HashSet<u64>,
    /// area.dat files found by their content, by hash.
    pub area_files: std::collections::BTreeSet<u64>,
    /// The type tag client.gom declares per field id low32, or why it could not be read;
    /// `None` when no archive holds client.gom.
    pub declared: Option<Result<std::collections::HashMap<u32, u8>, String>>,
    /// (field id, wire type tag) to the number of values read, over the objects left out.
    pub field_tags: std::collections::HashMap<(u64, u8), u64>,
    /// (kind, FQN prefix) to the number of objects the extract left out.
    pub left_out: std::collections::BTreeMap<(String, String), u64>,
}

impl BuildFacts {
    /// Note one archive entry, read from the archive named `archive`.
    pub fn note_entry(&mut self, archive: &str, hash: u64, data: &[u8]) {
        self.entries.insert(hash);
        static CLIENT_GOM: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        if hash == *CLIENT_GOM.get_or_init(|| path_hash(crate::gom_schema::CLIENT_GOM_PATH)) {
            self.declared =
                Some(crate::gom_schema::declared_tags(data).map_err(|e| format!("{e:#}")));
        } else if archive.contains("_en-us_") && crate::stb::is_stb(data) {
            self.string_tables.insert(hash, archive.to_string());
        } else if crate::area::is_area_dat(data) {
            self.area_files.insert(hash);
        }
    }

    /// Note an object the extract leaves out of its output: its kind and FQN prefix, and the
    /// type tag of each field value it carries.
    pub fn note_left_out(&mut self, kind: &str, fqn: &str, header: &[u8], payload: &[u8]) {
        let prefix = fqn.split('.').next().unwrap_or_default();
        *self
            .left_out
            .entry((kind.to_string(), prefix.to_string()))
            .or_default() += 1;
        self.note_fields(kind, header, payload);
    }

    /// Count the type tag of each field value an object carries. A conversation's fields are
    /// those of its node graph.
    pub fn note_fields(&mut self, kind: &str, header: &[u8], payload: &[u8]) {
        let tags = crate::gom_reader::field_tags(header, payload, kind == "Conversation");
        for key in tags.into_iter().flatten() {
            *self.field_tags.entry(key).or_default() += 1;
        }
    }
}

/// The archive key of a path.
pub(crate) fn path_hash(path: &str) -> u64 {
    let (ph, sh) = crate::hash::hashlittle2(path.as_bytes(), 0, 0);
    crate::hash::combine_hash(ph, sh)
}
