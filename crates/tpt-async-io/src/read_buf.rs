// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A write-once, cursor-tracked byte buffer for zero-copy async reads.

/// A wrapper around a mutable byte slice that tracks how many bytes have been
/// filled by a single async read operation.
///
/// Callers provide the underlying storage; `ReadBuf` tracks the filled
/// sub-slice so that readers can write directly into the unfilled portion
/// without extra copies.
pub struct ReadBuf<'a> {
    buf: &'a mut [u8],
    filled: usize,
}

impl<'a> ReadBuf<'a> {
    /// Creates a new `ReadBuf` wrapping `buf`, with zero bytes filled.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, filled: 0 }
    }

    /// Resets the filled cursor to zero so the same storage can be reused
    /// for another read.
    #[inline]
    pub fn clear(&mut self) {
        self.filled = 0;
    }

    /// Returns the filled sub-slice (bytes written so far).
    #[inline]
    pub fn filled(&self) -> &[u8] {
        &self.buf[..self.filled]
    }

    /// Returns a mutable reference to the unfilled portion of the buffer.
    ///
    /// Readers should write into this slice and then call [`advance`].
    ///
    /// [`advance`]: ReadBuf::advance
    #[inline]
    pub fn unfilled(&mut self) -> &mut [u8] {
        &mut self.buf[self.filled..]
    }

    /// Returns the number of bytes remaining (unfilled capacity).
    #[inline]
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.filled
    }

    /// Returns the total capacity of the buffer.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Advances the filled cursor by `n` bytes.
    ///
    /// # Panics
    ///
    /// Panics if `self.filled + n > self.buf.len()`.
    #[inline]
    pub fn advance(&mut self, n: usize) {
        assert!(
            self.filled + n <= self.buf.len(),
            "ReadBuf::advance would exceed capacity"
        );
        self.filled += n;
    }

    /// Appends `data` into the unfilled portion and advances the cursor.
    ///
    /// # Panics
    ///
    /// Panics if `data.len() > self.remaining()`.
    #[inline]
    pub fn put_slice(&mut self, data: &[u8]) {
        let n = data.len();
        assert!(
            n <= self.remaining(),
            "ReadBuf::put_slice: data exceeds remaining capacity"
        );
        self.buf[self.filled..self.filled + n].copy_from_slice(data);
        self.filled += n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_empty() {
        let mut storage = [0u8; 16];
        let mut buf = ReadBuf::new(&mut storage);
        assert_eq!(buf.filled().len(), 0);
        assert_eq!(buf.remaining(), 16);
        assert_eq!(buf.capacity(), 16);
        assert_eq!(buf.unfilled().len(), 16);
    }

    #[test]
    fn advance_tracks_filled() {
        let mut storage = [0u8; 8];
        let mut buf = ReadBuf::new(&mut storage);
        buf.advance(3);
        assert_eq!(buf.filled().len(), 3);
        assert_eq!(buf.remaining(), 5);
        assert_eq!(buf.unfilled().len(), 5);
    }

    #[test]
    #[should_panic(expected = "would exceed capacity")]
    fn advance_past_capacity_panics() {
        let mut storage = [0u8; 4];
        let mut buf = ReadBuf::new(&mut storage);
        buf.advance(5);
    }

    #[test]
    fn put_slice_copies_and_advances() {
        let mut storage = [0u8; 8];
        let mut buf = ReadBuf::new(&mut storage);
        buf.put_slice(b"abc");
        assert_eq!(buf.filled(), b"abc");
        buf.put_slice(b"de");
        assert_eq!(buf.filled(), b"abcde");
        assert_eq!(buf.remaining(), 3);
    }

    #[test]
    #[should_panic(expected = "exceeds remaining capacity")]
    fn put_slice_overflow_panics() {
        let mut storage = [0u8; 2];
        let mut buf = ReadBuf::new(&mut storage);
        buf.put_slice(b"abc");
    }

    #[test]
    fn clear_resets_cursor() {
        let mut storage = [0u8; 8];
        let mut buf = ReadBuf::new(&mut storage);
        buf.advance(6);
        buf.clear();
        assert_eq!(buf.filled().len(), 0);
        assert_eq!(buf.remaining(), 8);
    }
}
