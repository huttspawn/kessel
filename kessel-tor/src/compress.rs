//! Entry decompression (zstd and zlib).
//!
//! An archive entry flagged as compressed is zstd when it starts with the zstd
//! magic (the current game) and zlib otherwise (older builds).

use crate::error::{Error, Result};
use flate2::read::ZlibDecoder;
use std::io::Read;

/// The zstd frame magic, `28 B5 2F FD`.
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Decompress one entry's stored bytes. `flag` is the entry's compression
/// flag (0 = stored as is); `size_hint` is its uncompressed size.
///
/// # Errors
/// [`Error::Decompress`] when a compressed entry does not decode.
pub fn decompress(stored: &[u8], flag: u16, size_hint: usize) -> Result<Vec<u8>> {
    if flag == 0 {
        return Ok(stored.to_vec());
    }
    if stored.starts_with(&ZSTD_MAGIC) {
        return zstd::decode_all(stored).map_err(|source| Error::Decompress {
            codec: "zstd",
            source,
        });
    }
    let mut out = Vec::with_capacity(size_hint);
    ZlibDecoder::new(stored)
        .read_to_end(&mut out)
        .map_err(|source| Error::Decompress {
            codec: "zlib",
            source,
        })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    const SAMPLE: &[u8] = b"GOM payload bytes, repeated: GOM payload bytes, repeated.";

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn stored_entries_come_back_unchanged() -> TestResult {
        assert_eq!(decompress(SAMPLE, 0, SAMPLE.len())?, SAMPLE);
        Ok(())
    }

    #[test]
    fn zstd_entries_are_recognised_by_their_magic() -> TestResult {
        let stored = zstd::encode_all(SAMPLE, 3)?;
        assert!(stored.starts_with(&ZSTD_MAGIC));
        assert_eq!(decompress(&stored, 1, SAMPLE.len())?, SAMPLE);
        Ok(())
    }

    #[test]
    fn other_compressed_entries_are_zlib() -> TestResult {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(SAMPLE)?;
        let stored = enc.finish()?;
        assert_eq!(decompress(&stored, 1, SAMPLE.len())?, SAMPLE);
        Ok(())
    }

    #[test]
    fn corrupt_compressed_entries_are_errors() {
        assert!(decompress(b"not compressed", 1, 14).is_err());
    }
}
