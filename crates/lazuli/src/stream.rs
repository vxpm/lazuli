use std::collections::VecDeque;

use crate::Primitive;

/// Trait for types which can be seen as a binary data source.
pub trait BinaryStream {
    /// Prepares the stream for reading.
    fn prepare(&mut self) {}

    /// The currently available data.
    fn data(&self) -> &[u8];

    /// Consumes `amount` bytes from the data.
    fn consume(&mut self, amount: usize);

    /// Returns a reader for the data.
    fn reader(&mut self) -> BinReader<'_>
    where
        Self: Sized,
    {
        BinReader::new(self)
    }
}

impl BinaryStream for &[u8] {
    fn data(&self) -> &[u8] {
        self
    }

    fn consume(&mut self, amount: usize) {
        *self = &self[amount..];
    }
}

pub struct BinReader<'a> {
    data: &'a mut dyn BinaryStream,
    read: usize,
}

impl<'a> BinReader<'a> {
    pub fn new(data: &'a mut dyn BinaryStream) -> Self {
        data.prepare();
        Self { data, read: 0 }
    }

    /// Reads a primitive if there is enough data for it.
    pub fn read_be<P>(&mut self) -> Option<P>
    where
        P: Primitive,
    {
        let slice = &self.data.data()[self.read..];
        (slice.len() >= size_of::<P>()).then(|| {
            self.read += size_of::<P>();
            P::read_be_bytes(slice)
        })
    }

    /// Reads a sequence of `length` bytes if there is enough data for it.
    ///
    /// # Note
    /// The returned vector will have capacity equal to `length + 16`. This guarantees safety for
    /// reading a little bit past the end of the vector when parsing the attributes in a JIT, for
    /// example.
    pub fn read_bytes(&mut self, length: usize) -> Option<Vec<u8>> {
        let slice = &self.data.data()[self.read..];
        (slice.len() >= length).then(|| {
            self.read += length;

            let mut vec = Vec::with_capacity(length + 16);
            vec.extend_from_slice(&slice[..length]);
            vec
        })
    }

    /// Returns how many bytes of data are remaining in the data.
    pub fn remaining(&mut self) -> usize {
        self.data.data().len() - self.read
    }

    /// Consumes the read bytes returns how many bytes were read
    pub fn finish(self) -> usize {
        self.data.consume(self.read);
        self.read
    }
}

/// A ring buffer of binary data.
#[derive(Debug, Clone, Default)]
pub struct BinRingBuffer {
    data: VecDeque<u8>,
}

impl BinRingBuffer {
    /// Pushes the given primitive onto the buffer encoded as big-endian.
    pub fn push_be<P>(&mut self, value: P)
    where
        P: Primitive,
    {
        let mut bytes = [0; 8];
        value.write_be_bytes(&mut bytes);

        for byte in &bytes[0..size_of::<P>()] {
            self.data.push_back(*byte);
        }
    }

    pub fn push_front_bytes(&mut self, bytes: &[u8]) {
        self.data.prepend(bytes.iter().copied());
    }

    /// Current length of the buffer.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the buffer is empty or not.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl BinaryStream for BinRingBuffer {
    fn prepare(&mut self) {
        self.data.make_contiguous();
    }

    fn data(&self) -> &[u8] {
        self.data.as_slices().0
    }

    fn consume(&mut self, amount: usize) {
        self.data.drain(..amount);
    }
}
