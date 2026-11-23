//! A bounds-checked little-endian byte reader for SWTOR's binary formats.
//!
//! Every format kessel reads (archives, the GOM, string tables, models,
//! animations) is little-endian bytes from the game. [`Bytes`] reads them
//! without copying and without panicking: a read past the end is an
//! [`OutOfBounds`] error naming where it happened.
//!
//! ```
//! use kessel_bytes::Bytes;
//!
//! let mut b = Bytes::new(&[0x4D, 0x59, 0x50, 0x00, 0x2A, 0x00, 0x00, 0x00]);
//! assert_eq!(b.take(3)?, b"MYP");
//! b.skip(1)?;
//! assert_eq!(b.u32()?, 42);
//! assert!(b.u8().is_err());
//! # Ok::<(), kessel_bytes::OutOfBounds>(())
//! ```

/// A read went past the end of the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("needed {wanted} bytes at offset {at}, only {available} left")]
pub struct OutOfBounds {
    /// Offset of the read from the start of the bytes.
    pub at: usize,
    /// Bytes the read needed.
    pub wanted: usize,
    /// Bytes that were left.
    pub available: usize,
}

/// A cursor over borrowed bytes.
#[derive(Debug, Clone, Copy)]
pub struct Bytes<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bytes<'a> {
    /// A cursor at the start of `data`.
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// The cursor's offset from the start.
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Bytes left after the cursor.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// Whether the cursor is at the end.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Move the cursor to `offset` from the start.
    ///
    /// # Errors
    /// [`OutOfBounds`] when `offset` is past the end.
    pub fn seek(&mut self, offset: usize) -> Result<(), OutOfBounds> {
        if offset > self.data.len() {
            return Err(OutOfBounds {
                at: offset,
                wanted: 0,
                available: 0,
            });
        }
        self.pos = offset;
        Ok(())
    }

    /// Borrow the next `n` bytes and advance past them.
    ///
    /// # Errors
    /// [`OutOfBounds`] when fewer than `n` bytes are left.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], OutOfBounds> {
        let err = OutOfBounds {
            at: self.pos,
            wanted: n,
            available: self.remaining(),
        };
        let end = self.pos.checked_add(n).ok_or(err)?;
        let slice = self.data.get(self.pos..end).ok_or(err)?;
        self.pos = end;
        Ok(slice)
    }

    /// Advance past `n` bytes.
    ///
    /// # Errors
    /// [`OutOfBounds`] when fewer than `n` bytes are left.
    pub fn skip(&mut self, n: usize) -> Result<(), OutOfBounds> {
        self.take(n).map(|_| ())
    }

    /// Read `N` bytes as an array.
    ///
    /// # Errors
    /// [`OutOfBounds`] when fewer than `N` bytes are left.
    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], OutOfBounds> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    /// Read a byte.
    ///
    /// # Errors
    /// [`OutOfBounds`] at the end.
    pub fn u8(&mut self) -> Result<u8, OutOfBounds> {
        self.array::<1>().map(|[b]| b)
    }

    /// Read a little-endian `u16`.
    ///
    /// # Errors
    /// [`OutOfBounds`] when fewer than 2 bytes are left.
    pub fn u16(&mut self) -> Result<u16, OutOfBounds> {
        self.array().map(u16::from_le_bytes)
    }

    /// Read a little-endian `u32`.
    ///
    /// # Errors
    /// [`OutOfBounds`] when fewer than 4 bytes are left.
    pub fn u32(&mut self) -> Result<u32, OutOfBounds> {
        self.array().map(u32::from_le_bytes)
    }

    /// Read a little-endian `u64`.
    ///
    /// # Errors
    /// [`OutOfBounds`] when fewer than 8 bytes are left.
    pub fn u64(&mut self) -> Result<u64, OutOfBounds> {
        self.array().map(u64::from_le_bytes)
    }

    /// Read a little-endian `f32`.
    ///
    /// # Errors
    /// [`OutOfBounds`] when fewer than 4 bytes are left.
    pub fn f32(&mut self) -> Result<f32, OutOfBounds> {
        self.array().map(f32::from_le_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_values_in_order() {
        let data = [
            1, 0x34, 0x12, 0x78, 0x56, 0x34, 0x12, 0, 0, 0x80, 0x3F, 9, 8, 7, 6, 5, 4, 3, 2,
        ];
        let mut b = Bytes::new(&data);
        assert_eq!(b.u8(), Ok(1));
        assert_eq!(b.u16(), Ok(0x1234));
        assert_eq!(b.u32(), Ok(0x1234_5678));
        assert_eq!(b.f32(), Ok(1.0));
        assert_eq!(b.u64(), Ok(0x0203_0405_0607_0809));
        assert!(b.is_empty());
    }

    #[test]
    fn a_short_read_names_where_it_failed_and_does_not_move() {
        let mut b = Bytes::new(&[1, 2, 3]);
        b.skip(2).unwrap();
        assert_eq!(
            b.u32(),
            Err(OutOfBounds {
                at: 2,
                wanted: 4,
                available: 1
            })
        );
        assert_eq!(b.position(), 2);
    }

    #[test]
    fn huge_lengths_do_not_overflow() {
        let mut b = Bytes::new(&[0; 4]);
        b.skip(1).unwrap();
        assert!(b.take(usize::MAX).is_err());
    }

    #[test]
    fn seek_rejects_offsets_past_the_end() {
        let mut b = Bytes::new(&[0; 4]);
        assert!(b.seek(4).is_ok());
        assert!(b.seek(5).is_err());
    }
}
