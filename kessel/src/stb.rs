//! STB (String Table Binary) Parser
//!
//! STB files contain localized strings for SWTOR game objects.
//! They are found at paths like `/resources/en-us/str/abl.stb`.
//!
//! Format (discovered by analysis):
//! Header (7 bytes):
//!   byte 0:      Version (0x01)
//!   bytes 1-2:   Reserved (0x0000)
//!   bytes 3-6:   String count (u32 LE)
//!
//! Entry (26 bytes each):
//!   bytes 0-3:   ID1 (u32 LE)
//!   bytes 4-7:   ID2 (u32 LE)
//!   bytes 8-9:   Flags (u16 LE)
//!   bytes 10-13: Version (u32 LE)
//!   bytes 14-17: Text length (u32 LE)
//!   bytes 18-21: Text offset (u32 LE)
//!   bytes 22-25: Additional length (u32 LE)
//!
//! Text data follows entries at specified offsets.

use crate::hash::{combine_hash, hashlittle2};
use anyhow::{bail, Result};
use std::collections::HashMap;

const HEADER_SIZE: usize = 7;
const ENTRY_SIZE: usize = 26;

/// A single string entry from an STB file
#[derive(Debug, Clone)]
pub struct StbEntry {
    /// Primary ID
    pub id1: u32,
    /// Secondary ID
    pub id2: u32,
    /// Entry flags
    #[allow(dead_code)]
    pub flags: u16,
    /// Version number
    pub version: u32,
    /// The localized text
    pub text: String,
}

/// Parsed STB file with metadata
#[derive(Debug, Clone)]
pub struct StbFile {
    /// All string entries
    pub entries: Vec<StbEntry>,
    /// Locale extracted from path (e.g., "en-us")
    pub locale: String,
    /// FQN prefix derived from path (e.g., "str.abl")
    pub fqn_prefix: String,
}

/// Parse an STB file from raw bytes
///
/// # Arguments
/// * `data` - Raw STB file bytes
/// * `path` - File path (used to extract locale and FQN prefix)
///
/// # Returns
/// Parsed StbFile with entries, locale, and FQN prefix
pub fn parse(data: &[u8], path: &str) -> Result<StbFile> {
    if data.len() < HEADER_SIZE {
        bail!("STB file too small: {} bytes", data.len());
    }

    // Verify version byte
    if data[0] != 0x01 {
        bail!("Invalid STB version: {:02X} (expected 0x01)", data[0]);
    }

    // Read string count
    let count = u32::from_le_bytes([data[3], data[4], data[5], data[6]]) as usize;

    // Calculate expected minimum size
    let entries_end = HEADER_SIZE + (count * ENTRY_SIZE);
    if data.len() < entries_end {
        bail!(
            "STB file truncated: {} bytes, need {} for {} entries",
            data.len(),
            entries_end,
            count
        );
    }

    let mut entries = Vec::with_capacity(count);

    for i in 0..count {
        let entry_offset = HEADER_SIZE + (i * ENTRY_SIZE);
        let entry_data = &data[entry_offset..entry_offset + ENTRY_SIZE];

        let id1 = u32::from_le_bytes([entry_data[0], entry_data[1], entry_data[2], entry_data[3]]);
        let id2 = u32::from_le_bytes([entry_data[4], entry_data[5], entry_data[6], entry_data[7]]);
        let flags = u16::from_le_bytes([entry_data[8], entry_data[9]]);
        let version = u32::from_le_bytes([
            entry_data[10],
            entry_data[11],
            entry_data[12],
            entry_data[13],
        ]);
        let text_len = u32::from_le_bytes([
            entry_data[14],
            entry_data[15],
            entry_data[16],
            entry_data[17],
        ]) as usize;
        let text_offset = u32::from_le_bytes([
            entry_data[18],
            entry_data[19],
            entry_data[20],
            entry_data[21],
        ]) as usize;
        // bytes 22-25 are additional_length, not needed for basic parsing

        // Read text at offset
        let text = if text_len > 0 && text_offset + text_len <= data.len() {
            let text_bytes = &data[text_offset..text_offset + text_len];
            // STB text is UTF-8, but may have null terminator
            let text_str = String::from_utf8_lossy(text_bytes);
            text_str.trim_end_matches('\0').to_string()
        } else {
            String::new()
        };

        entries.push(StbEntry {
            id1,
            id2,
            flags,
            version,
            text,
        });
    }

    let locale = extract_locale_from_path(path);
    let fqn_prefix = extract_fqn_from_path(path);

    Ok(StbFile {
        entries,
        locale,
        fqn_prefix,
    })
}

/// Extract locale from STB file path
///
/// Path format: `/resources/en-us/str/abl.stb`
/// Returns: "en-us"
fn extract_locale_from_path(path: &str) -> String {
    // Look for pattern /resources/{locale}/str/
    let normalized = path.to_lowercase().replace('\\', "/");

    if let Some(resources_pos) = normalized.find("/resources/") {
        let after_resources = &normalized[resources_pos + 11..]; // skip "/resources/"
        if let Some(slash_pos) = after_resources.find('/') {
            return after_resources[..slash_pos].to_string();
        }
    }

    // Default to en-us if we can't parse
    "en-us".to_string()
}

/// Extract FQN prefix from STB file path
///
/// Path format: `/resources/en-us/str/abl.stb`
/// Returns: "str.abl"
///
/// Path format: `/resources/en-us/str/abl/agent/skill.stb`
/// Returns: "str.abl.agent.skill"
fn extract_fqn_from_path(path: &str) -> String {
    let normalized = path.to_lowercase().replace('\\', "/");

    // Find /str/ and extract everything after it
    if let Some(str_pos) = normalized.find("/str/") {
        let after_str = &normalized[str_pos + 5..]; // skip "/str/"

        // Remove .stb extension and convert slashes to dots
        let without_ext = after_str.trim_end_matches(".stb");
        let fqn_suffix = without_ext.replace('/', ".");

        return format!("str.{}", fqn_suffix);
    }

    // Fallback: just use filename without extension
    if let Some(filename) = normalized.rsplit('/').next() {
        let without_ext = filename.trim_end_matches(".stb");
        return format!("str.{}", without_ext);
    }

    "str.unknown".to_string()
}

/// The STB tables extracted from the archive, by path. Each is found by the
/// hash of its path, so no dictionary is needed. Root tables hold the primary
/// object kinds; the gui tables hold category labels the root tables lack
/// (conquest themes, Galactic Command reward tracks, achievement categories
/// and ventures). Conversation tables (`/str/cnv/`) are not listed: they are
/// derived from the `cnv.*` objects in `populate_conversation_strings`.
const KNOWN_STB_PATHS: &[&str] = &[
    "/resources/en-us/str/abl.stb",
    "/resources/en-us/str/tal.stb",
    "/resources/en-us/str/itm.stb",
    "/resources/en-us/str/npc.stb",
    "/resources/en-us/str/qst.stb",
    "/resources/en-us/str/cdx.stb",
    "/resources/en-us/str/ach.stb",
    "/resources/en-us/str/gui/planetaryconquest.stb",
    "/resources/en-us/str/gui/galacticcommand.stb",
    "/resources/en-us/str/gui/achievementcategories.stb",
];

/// Archive filename hash -> path for every known STB table. The hash is
/// `combine_hash(ph, sh)`, the key archive entries carry.
pub fn known_stb_hashes() -> HashMap<u64, &'static str> {
    KNOWN_STB_PATHS
        .iter()
        .map(|path| {
            let (ph, sh) = hashlittle2(path.as_bytes(), 0, 0);
            (combine_hash(ph, sh), *path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_locale() {
        assert_eq!(
            extract_locale_from_path("/resources/en-us/str/abl.stb"),
            "en-us"
        );
        assert_eq!(
            extract_locale_from_path("/resources/de-de/str/itm.stb"),
            "de-de"
        );
        assert_eq!(
            extract_locale_from_path("/resources/fr-fr/str/npc.stb"),
            "fr-fr"
        );
    }

    #[test]
    fn test_extract_fqn() {
        assert_eq!(
            extract_fqn_from_path("/resources/en-us/str/abl.stb"),
            "str.abl"
        );
        assert_eq!(
            extract_fqn_from_path("/resources/en-us/str/abl/agent/skill.stb"),
            "str.abl.agent.skill"
        );
        assert_eq!(
            extract_fqn_from_path("/resources/en-us/str/itm/loot/quality.stb"),
            "str.itm.loot.quality"
        );
    }

    #[test]
    fn known_stb_hashes_match_dictionary() {
        // Real PH/SH pairs from hashes_filename.txt.
        let cases: &[(&str, u32, u32)] = &[
            ("/resources/en-us/str/abl.stb", 0x8154_956D, 0x5430_5B3B),
            ("/resources/en-us/str/tal.stb", 0x93A7_AD6F, 0x5DE1_7C3F),
            ("/resources/en-us/str/itm.stb", 0x32EF_86D8, 0xFB5D_CB98),
            ("/resources/en-us/str/npc.stb", 0xB2A2_6731, 0x0DDC_0C21),
            ("/resources/en-us/str/qst.stb", 0xD097_4307, 0x8ED1_2356),
            ("/resources/en-us/str/cdx.stb", 0xE488_9EE0, 0x10A7_098C),
            ("/resources/en-us/str/ach.stb", 0x8BBE_1958, 0x45F4_B329),
            (
                "/resources/en-us/str/gui/planetaryconquest.stb",
                0xA3B4_7B7C,
                0x1203_52AA,
            ),
            (
                "/resources/en-us/str/gui/galacticcommand.stb",
                0x7552_57CA,
                0xE929_6959,
            ),
            (
                "/resources/en-us/str/gui/achievementcategories.stb",
                0x2B44_73F6,
                0x98B3_EA51,
            ),
        ];
        let derived = known_stb_hashes();
        assert_eq!(derived.len(), cases.len());
        for (path, ph, sh) in cases {
            assert_eq!(derived.get(&combine_hash(*ph, *sh)), Some(path));
        }
    }
}
