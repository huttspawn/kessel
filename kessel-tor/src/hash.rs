//! SWTOR's filename hash and the dictionary that maps hashes to paths.
//!
//! Archives name entries by Bob Jenkins' lookup3 `hashlittle2` of the full
//! path (case preserved, UTF-8, no terminator). The dictionary file uses the
//! `EasyMYP` format, one `PH#SH#path#CRC` line per file, and an archive entry's
//! 64-bit hash is `(PH << 32) | SH`.

use crate::error::{Error, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// The 64-bit archive hash of a path: `(PH << 32) | SH`.
#[must_use]
pub fn filename_hash(path: &str) -> u64 {
    let (ph, sh) = hashlittle2(path.as_bytes(), 0, 0);
    (u64::from(ph) << 32) | u64::from(sh)
}

/// Bob Jenkins' lookup3 `hashlittle2`, returning `(primary, secondary)` as the
/// game's dictionary labels them: lookup3's final `b` then `c`.
#[must_use]
pub fn hashlittle2(key: &[u8], initval: u32, initval2: u32) -> (u32, u32) {
    // lookup3 folds the length in as a u32; paths are far shorter, and the
    // truncation is part of the algorithm for longer keys
    #[allow(clippy::cast_possible_truncation)]
    let len = key.len() as u32;
    let mut a = 0xdead_beef_u32.wrapping_add(len).wrapping_add(initval);
    let mut b = a;
    let mut c = a.wrapping_add(initval2);
    let mut blocks = key.chunks(12).peekable();
    while let Some(block) = blocks.next() {
        // every block is zero-padded to 12 bytes; only the last one gets the final mix
        let mut w = [0u8; 12];
        for (dst, src) in w.iter_mut().zip(block) {
            *dst = *src;
        }
        let [a0, a1, a2, a3, b0, b1, b2, b3, c0, c1, c2, c3] = w;
        a = a.wrapping_add(word([a0, a1, a2, a3]));
        b = b.wrapping_add(word([b0, b1, b2, b3]));
        c = c.wrapping_add(word([c0, c1, c2, c3]));
        if blocks.peek().is_some() {
            (a, b, c) = mix(a, b, c);
        }
    }
    if key.is_empty() {
        return (b, c);
    }
    let (_, b, c) = final_mix(a, b, c);
    (b, c)
}

fn word(bytes: [u8; 4]) -> u32 {
    u32::from_le_bytes(bytes)
}

fn mix(mut a: u32, mut b: u32, mut c: u32) -> (u32, u32, u32) {
    a = a.wrapping_sub(c) ^ c.rotate_left(4);
    c = c.wrapping_add(b);
    b = b.wrapping_sub(a) ^ a.rotate_left(6);
    a = a.wrapping_add(c);
    c = c.wrapping_sub(b) ^ b.rotate_left(8);
    b = b.wrapping_add(a);
    a = a.wrapping_sub(c) ^ c.rotate_left(16);
    c = c.wrapping_add(b);
    b = b.wrapping_sub(a) ^ a.rotate_left(19);
    a = a.wrapping_add(c);
    c = c.wrapping_sub(b) ^ b.rotate_left(4);
    b = b.wrapping_add(a);
    (a, b, c)
}

fn final_mix(mut a: u32, mut b: u32, mut c: u32) -> (u32, u32, u32) {
    c = (c ^ b).wrapping_sub(b.rotate_left(14));
    a = (a ^ c).wrapping_sub(c.rotate_left(11));
    b = (b ^ a).wrapping_sub(a.rotate_left(25));
    c = (c ^ b).wrapping_sub(b.rotate_left(16));
    a = (a ^ c).wrapping_sub(c.rotate_left(4));
    b = (b ^ a).wrapping_sub(a.rotate_left(14));
    c = (c ^ b).wrapping_sub(b.rotate_left(24));
    (a, b, c)
}

/// Paths by archive hash, loaded from an `EasyMYP` dictionary.
#[derive(Default)]
pub struct Dictionary {
    paths: HashMap<u64, String>,
}

impl Dictionary {
    /// Load an `EasyMYP` dictionary file. Lines that do not parse are skipped.
    ///
    /// # Errors
    /// [`Error::Io`] when the file cannot be read.
    pub fn load(path: &Path) -> Result<Self> {
        let io = |source| Error::Io {
            path: path.to_path_buf(),
            source,
        };
        let file = File::open(path).map_err(io)?;
        let mut dict = Self::default();
        for line in BufReader::new(file).lines() {
            dict.add_line(&line.map_err(io)?);
        }
        Ok(dict)
    }

    /// Add one `PH#SH#path#CRC` line; returns whether it parsed.
    pub fn add_line(&mut self, line: &str) -> bool {
        let mut parts = line.split('#');
        let (Some(ph), Some(sh), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            return false;
        };
        let (Ok(ph), Ok(sh)) = (u32::from_str_radix(ph, 16), u32::from_str_radix(sh, 16)) else {
            return false;
        };
        self.paths
            .insert((u64::from(ph) << 32) | u64::from(sh), path.to_string());
        true
    }

    /// The path for an archive hash, when the dictionary knows it.
    #[must_use]
    pub fn path(&self, hash: u64) -> Option<&str> {
        self.paths.get(&hash).map(String::as_str)
    }

    /// Every known path containing `pattern`, with its hash.
    pub fn matching<'a>(&'a self, pattern: &'a str) -> impl Iterator<Item = (u64, &'a str)> + 'a {
        self.paths
            .iter()
            .filter(move |(_, p)| p.contains(pattern))
            .map(|(h, p)| (*h, p.as_str()))
    }

    /// How many paths the dictionary holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Whether the dictionary holds no paths.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PH/SH pairs as they appear in the game's hashes_filename.txt.
    const KNOWN: &[(&str, u32, u32)] = &[
        ("/resources/en-us/str/abl.stb", 0x8154_956D, 0x5430_5B3B),
        (
            "/resources/systemgenerated/client.gom",
            0x6107_069D,
            0xB7C7_0D58,
        ),
    ];

    #[test]
    fn hashes_match_the_game_dictionary() {
        for (path, ph, sh) in KNOWN {
            assert_eq!(hashlittle2(path.as_bytes(), 0, 0), (*ph, *sh), "{path}");
            assert_eq!(filename_hash(path), (u64::from(*ph) << 32) | u64::from(*sh));
        }
    }

    #[test]
    fn every_tail_length_hashes() {
        // exercises each 1..=12 byte tail plus the 12-byte block loop
        let path = "/resources/art/dynamic/weapon/model/saber_sithhigh06_a01_v01.gr2";
        let hashes: Vec<u64> = (1..=path.len())
            .map(|n| filename_hash(&path[..n]))
            .collect();
        let distinct: std::collections::HashSet<_> = hashes.iter().collect();
        assert_eq!(distinct.len(), hashes.len());
    }

    #[test]
    fn empty_input_returns_the_seed() {
        assert_eq!(hashlittle2(b"", 0, 0), (0xdead_beef, 0xdead_beef));
    }

    #[test]
    fn hashing_preserves_case() {
        assert_ne!(
            filename_hash("/resources/en-us/str/abl.stb"),
            filename_hash("/resources/EN-US/STR/abl.stb")
        );
    }

    #[test]
    fn dictionary_lines_key_by_primary_then_secondary() {
        let mut dict = Dictionary::default();
        assert!(dict.add_line("8154956D#54305B3B#/resources/en-us/str/abl.stb#00000000"));
        assert!(!dict.add_line("not a dictionary line"));
        assert_eq!(dict.len(), 1);
        assert_eq!(
            dict.path(filename_hash("/resources/en-us/str/abl.stb")),
            Some("/resources/en-us/str/abl.stb")
        );
        assert_eq!(dict.matching("/str/").count(), 1);
    }
}
