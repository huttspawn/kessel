//! The MYP header, entry tables and reading one entry's bytes.
//!
//! A `.tor` archive is an MYP container:
//!
//! - header (40 bytes): `MYP` at 0, u64 offset of the first entry table at 12,
//!   u32 total entry count at 24;
//! - entry tables, chained: u32 slot count, u64 offset of the next table (0 ends
//!   the chain), then that many 34-byte slots;
//! - slot: u64 position, u32 header size, u32 stored size, u32 uncompressed
//!   size, u64 filename hash, u32 crc32, u16 compression flag. A slot whose
//!   position is 0 is empty.
//!
//! All integers are little endian.

use crate::compress::decompress;
use crate::error::{Error, Result};
use kessel_bytes::Bytes;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const MAGIC: &[u8; 3] = b"MYP";
const HEADER_LEN: usize = 40;
const TABLE_HEADER_LEN: usize = 12;
const SLOT_LEN: usize = 34;

/// One file stored in an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Where the entry's own header starts in the archive.
    pub position: u64,
    /// Bytes of per-entry header before the stored data.
    pub header_len: u32,
    /// Bytes stored in the archive.
    pub stored_len: u32,
    /// Bytes after decompression.
    pub len: u32,
    /// The path's archive hash, `(PH << 32) | SH`.
    pub hash: u64,
    /// The stored data's CRC-32.
    pub crc32: u32,
    /// 0 when stored as is; otherwise compressed (zstd or zlib).
    pub compression: u16,
}

impl Entry {
    /// Parse one 34-byte slot; `None` for an empty slot.
    fn from_slot(slot: &[u8; SLOT_LEN]) -> Result<Option<Self>> {
        let truncated = |source| Error::Truncated {
            what: "entry slot",
            source,
        };
        let mut b = Bytes::new(slot);
        let position = b.u64().map_err(truncated)?;
        if position == 0 {
            return Ok(None);
        }
        Ok(Some(Self {
            position,
            header_len: b.u32().map_err(truncated)?,
            stored_len: b.u32().map_err(truncated)?,
            len: b.u32().map_err(truncated)?,
            hash: b.u64().map_err(truncated)?,
            crc32: b.u32().map_err(truncated)?,
            compression: b.u16().map_err(truncated)?,
        }))
    }
}

/// An open archive and its entry list.
pub struct Archive<R> {
    reader: R,
    entries: Vec<Entry>,
}

impl Archive<BufReader<File>> {
    /// Open a `.tor` file and read its entry tables.
    ///
    /// # Errors
    /// [`Error::Io`] when the file cannot be read, or any error
    /// [`Archive::from_reader`] reports.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_reader(BufReader::new(file)).map_err(|e| match e {
            Error::Io { source, .. } => Error::Io {
                path: path.to_path_buf(),
                source,
            },
            other => other,
        })
    }
}

impl<R: Read + Seek> Archive<R> {
    /// Read the header and every entry table from any seekable source.
    ///
    /// # Errors
    /// [`Error::BadMagic`] when the source is not an MYP archive,
    /// [`Error::TableLoop`] when the table chain repeats an offset, and
    /// [`Error::Io`] when the source ends early.
    pub fn from_reader(mut reader: R) -> Result<Self> {
        let header: [u8; HEADER_LEN] = read_array(&mut reader)?;
        let mut h = Bytes::new(&header);
        let truncated = |source| Error::Truncated {
            what: "archive header",
            source,
        };
        if h.take(3).map_err(truncated)? != MAGIC {
            return Err(Error::BadMagic);
        }
        h.seek(12).map_err(truncated)?;
        let mut table = h.u64().map_err(truncated)?;
        h.seek(24).map_err(truncated)?;
        let total = h.u32().map_err(truncated)?;

        let mut entries = Vec::with_capacity(usize::try_from(total).unwrap_or(0));
        let mut seen = HashSet::new();
        while table != 0 {
            if !seen.insert(table) {
                return Err(Error::TableLoop { offset: table });
            }
            reader.seek(SeekFrom::Start(table)).map_err(io)?;
            let th: [u8; TABLE_HEADER_LEN] = read_array(&mut reader)?;
            let mut t = Bytes::new(&th);
            let truncated = |source| Error::Truncated {
                what: "entry table header",
                source,
            };
            let slots = t.u32().map_err(truncated)?;
            let next = t.u64().map_err(truncated)?;
            for _ in 0..slots {
                let slot: [u8; SLOT_LEN] = read_array(&mut reader)?;
                entries.extend(Entry::from_slot(&slot)?);
            }
            table = next;
        }
        Ok(Self { reader, entries })
    }

    /// Every non-empty entry, in table order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry whose path hashes to `hash`.
    #[must_use]
    pub fn find(&self, hash: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.hash == hash)
    }

    /// The entry's own header bytes (before its stored data).
    ///
    /// # Errors
    /// [`Error::Io`] when the archive ends inside the header.
    pub fn read_header(&mut self, entry: &Entry) -> Result<Vec<u8>> {
        self.reader
            .seek(SeekFrom::Start(entry.position))
            .map_err(io)?;
        read_vec(&mut self.reader, entry.header_len)
    }

    /// The entry's file contents, decompressed.
    ///
    /// # Errors
    /// [`Error::Io`] when the archive ends inside the data, or
    /// [`Error::Decompress`] when compressed data does not decode.
    pub fn read(&mut self, entry: &Entry) -> Result<Vec<u8>> {
        let start = entry.position + u64::from(entry.header_len);
        self.reader.seek(SeekFrom::Start(start)).map_err(io)?;
        let stored = read_vec(&mut self.reader, entry.stored_len)?;
        decompress(
            &stored,
            entry.compression,
            usize::try_from(entry.len).unwrap_or(0),
        )
    }
}

/// An I/O failure while reading an already opened source.
fn io(source: std::io::Error) -> Error {
    Error::Io {
        path: std::path::PathBuf::new(),
        source,
    }
}

fn read_array<const N: usize>(reader: &mut impl Read) -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    reader.read_exact(&mut buf).map_err(io)?;
    Ok(buf)
}

fn read_vec(reader: &mut impl Read, len: u32) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let read = reader
        .take(u64::from(len))
        .read_to_end(&mut buf)
        .map_err(io)?;
    if read != buf.capacity().min(read) || buf.len() < usize::try_from(len).unwrap_or(usize::MAX) {
        return Err(io(std::io::ErrorKind::UnexpectedEof.into()));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::filename_hash;
    use std::io::Cursor;

    type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

    /// A two-table archive: one stored file, one zstd file, one empty slot.
    fn sample() -> TestResult<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let plain = b"stored file".to_vec();
        let packed_src = b"zstd file, zstd file, zstd file".to_vec();
        let packed = zstd::encode_all(&packed_src[..], 3)?;
        let mut a = vec![0u8; HEADER_LEN];
        a[0..3].copy_from_slice(MAGIC);
        // data: entry header (2 bytes) + data, for each file
        let p1 = a.len() as u64;
        a.extend_from_slice(&[0xAA, 0xBB]);
        a.extend_from_slice(&plain);
        let p2 = a.len() as u64;
        a.extend_from_slice(&packed);
        let slot = |pos: u64, hl: u32, sl: u32, l: u32, h: u64, c: u16| {
            let mut s = Vec::with_capacity(SLOT_LEN);
            s.extend_from_slice(&pos.to_le_bytes());
            s.extend_from_slice(&hl.to_le_bytes());
            s.extend_from_slice(&sl.to_le_bytes());
            s.extend_from_slice(&l.to_le_bytes());
            s.extend_from_slice(&h.to_le_bytes());
            s.extend_from_slice(&0u32.to_le_bytes());
            s.extend_from_slice(&c.to_le_bytes());
            s
        };
        let t1 = a.len() as u64;
        let t2 = t1 + (TABLE_HEADER_LEN + 2 * SLOT_LEN) as u64;
        a.extend_from_slice(&2u32.to_le_bytes());
        a.extend_from_slice(&t2.to_le_bytes());
        a.extend(slot(
            p1,
            2,
            u32::try_from(plain.len())?,
            u32::try_from(plain.len())?,
            filename_hash("/a/plain.txt"),
            0,
        ));
        a.extend(slot(0, 0, 0, 0, 0, 0));
        a.extend_from_slice(&1u32.to_le_bytes());
        a.extend_from_slice(&0u64.to_le_bytes());
        a.extend(slot(
            p2,
            0,
            u32::try_from(packed.len())?,
            u32::try_from(packed_src.len())?,
            filename_hash("/a/packed.bin"),
            1,
        ));
        a[12..20].copy_from_slice(&t1.to_le_bytes());
        a[24..28].copy_from_slice(&2u32.to_le_bytes());
        Ok((a, plain, packed_src))
    }

    #[test]
    fn reads_every_table_and_skips_empty_slots() -> TestResult {
        let (bytes, _, _) = sample()?;
        let ar = Archive::from_reader(Cursor::new(bytes))?;
        assert_eq!(ar.entries().len(), 2);
        Ok(())
    }

    #[test]
    fn reads_stored_and_compressed_entries_by_path_hash() -> TestResult {
        let (bytes, plain, packed_src) = sample()?;
        let mut ar = Archive::from_reader(Cursor::new(bytes))?;
        let e = ar
            .find(filename_hash("/a/plain.txt"))
            .cloned()
            .ok_or("plain entry")?;
        assert_eq!(ar.read_header(&e)?, vec![0xAA, 0xBB]);
        assert_eq!(ar.read(&e)?, plain);
        let e = ar
            .find(filename_hash("/a/packed.bin"))
            .cloned()
            .ok_or("packed entry")?;
        assert_eq!(ar.read(&e)?, packed_src);
        Ok(())
    }

    #[test]
    fn rejects_files_that_are_not_myp() {
        assert!(matches!(
            Archive::from_reader(Cursor::new(vec![0u8; HEADER_LEN])),
            Err(Error::BadMagic)
        ));
    }

    #[test]
    fn rejects_a_table_chain_that_loops() -> TestResult {
        let (mut bytes, _, _) = sample()?;
        let t1 = u64::from_le_bytes(bytes[12..20].try_into()?);
        let at = usize::try_from(t1)?;
        bytes[at + 4..at + 12].copy_from_slice(&t1.to_le_bytes());
        assert!(matches!(
            Archive::from_reader(Cursor::new(bytes)),
            Err(Error::TableLoop { .. })
        ));
        Ok(())
    }
}
