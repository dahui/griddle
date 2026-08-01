//! Bounds-checked reading of the byte slice, and the two ways v27/v28 and v29 encode a key.
//!
//! Split out of the parser because it is the half with no domain knowledge in it at all: every
//! function here is about staying inside the slice. The parser next door is then about what the
//! bytes *mean*.

use super::Error;

/// How keys are encoded: u32 indices into a table (v29) or inline NUL-terminated (v27/v28).
pub(super) struct Keys<'a> {
    pub(super) table: &'a [&'a [u8]],
    pub(super) indexed: bool,
}

pub(super) fn read_key<'a>(c: &mut Cursor<'a>, keys: &Keys<'a>) -> Result<&'a [u8], Error> {
    if keys.indexed {
        let index = c.u32()?;
        keys.table
            .get(index as usize)
            .copied()
            .ok_or(Error::KeyIndexOutOfRange {
                index,
                count: keys.table.len(),
            })
    } else {
        c.cstring()
    }
}

pub(super) fn read_string_table(data: &[u8], offset: i64) -> Result<Vec<&[u8]>, Error> {
    let start = usize::try_from(offset)
        .ok()
        .filter(|o| *o < data.len())
        .ok_or(Error::StringTableOutOfRange {
            offset,
            len: data.len(),
        })?;

    let mut c = Cursor { data, pos: start };
    let count = c.u32()? as usize;

    // Each string costs at least its NUL, so the count cannot exceed the bytes left. This
    // turns a corrupt offset into an error instead of a multi-gigabyte allocation.
    let left = data.len() - c.pos;
    if count > left {
        return Err(Error::StringTableTooLarge { count, left });
    }

    let mut table = Vec::with_capacity(count);
    for _ in 0..count {
        table.push(c.cstring()?);
    }
    Ok(table)
}

pub(super) struct Cursor<'a> {
    pub(super) data: &'a [u8],
    pub(super) pos: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    pub(super) fn take(&mut self, n: usize, expected: &'static str) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).ok_or(Error::UnexpectedEof {
            offset: self.pos,
            expected,
        })?;
        let slice = self.data.get(self.pos..end).ok_or(Error::UnexpectedEof {
            offset: self.pos,
            expected,
        })?;
        self.pos = end;
        Ok(slice)
    }

    pub(super) fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1, "u8")?[0])
    }

    pub(super) fn u32(&mut self) -> Result<u32, Error> {
        let mut b = [0u8; 4];
        b.copy_from_slice(self.take(4, "u32")?);
        Ok(u32::from_le_bytes(b))
    }

    pub(super) fn u64(&mut self) -> Result<u64, Error> {
        let mut b = [0u8; 8];
        b.copy_from_slice(self.take(8, "u64")?);
        Ok(u64::from_le_bytes(b))
    }

    pub(super) fn i64(&mut self) -> Result<i64, Error> {
        let mut b = [0u8; 8];
        b.copy_from_slice(self.take(8, "i64")?);
        Ok(i64::from_le_bytes(b))
    }

    pub(super) fn cstring(&mut self) -> Result<&'a [u8], Error> {
        let start = self.pos;
        let len = self.data[start..]
            .iter()
            .position(|&b| b == 0)
            .ok_or(Error::UnterminatedString { offset: start })?;
        self.pos = start + len + 1;
        Ok(&self.data[start..start + len])
    }

    /// UTF-16, NUL-terminated by a *pair* of zero bytes. Never seen in practice; handled so an
    /// exotic entry is skipped correctly rather than desyncing its blob.
    pub(super) fn wstring(&mut self) -> Result<(), Error> {
        let start = self.pos;
        loop {
            let pair = self.take(2, "wide string")?;
            if pair == [0, 0] {
                return Ok(());
            }
            if self.pos <= start {
                return Err(Error::UnterminatedString { offset: start });
            }
        }
    }
}
