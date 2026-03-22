use embedded_storage_async::nor_flash::{
    ErrorType, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};
use picoboot_rs::PICO_SECTOR_SIZE;

/// A NorFlash implementation backed by an in-memory buffer.
/// Used to generate a MapStorage containing the devices config,
/// which can then be written directly to the device.
///
/// Tracks the maximum offset written/erased (`max_used`) so that only the
/// relevant portion of the buffer needs to be written to the device.
pub struct BufferNorFlash {
    data: Vec<u8>,
    max_used: usize,
}

impl BufferNorFlash {
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0xFF; size],
            max_used: 0,
        }
    }

    /// Consumes self returning the finalized buffer
    pub fn into_buffer(mut self) -> Vec<u8> {
        self.data.truncate(self.max_used);
        self.data
    }
}

impl ReadNorFlash for BufferNorFlash {
    // Might be required to match ReadNorFlash implementation used by embassy-rp
    const READ_SIZE: usize = 4;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        bytes.copy_from_slice(&self.data[offset..offset + bytes.len()]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        self.data.len()
    }
}

impl NorFlash for BufferNorFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = PICO_SECTOR_SIZE as usize;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let from = from as usize;
        let to = to as usize;
        self.data[from..to].fill(0xFF);
        self.max_used = self.max_used.max(to);
        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let offset = offset as usize;
        for (i, &byte) in bytes.iter().enumerate() {
            self.data[offset + i] &= byte;
        }
        self.max_used = self.max_used.max(offset + bytes.len());
        Ok(())
    }
}

#[derive(Debug)]
pub struct BufferNorFlashError;

impl core::fmt::Display for BufferNorFlashError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "BufferNorFlash error")
    }
}

impl NorFlashError for BufferNorFlashError {
    fn kind(&self) -> NorFlashErrorKind {
        NorFlashErrorKind::Other
    }
}

impl ErrorType for BufferNorFlash {
    type Error = BufferNorFlashError;
}
