use dpedal_config::{PICO_FLASH_SIZE, RP2040_FLASH_OFFSET};
use embedded_storage_async::nor_flash::{
    ErrorType, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};
use picoboot_rs::{PICO_PAGE_SIZE, PICO_SECTOR_SIZE, PicobootConnection};
use rusb::UsbContext;

#[derive(Debug)]
pub struct PicobootNorFlashError(String);

impl core::fmt::Display for PicobootNorFlashError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl NorFlashError for PicobootNorFlashError {
    fn kind(&self) -> NorFlashErrorKind {
        NorFlashErrorKind::Other
    }
}

/// A NorFlash implementation backed by a picoboot USB connection.
///
/// Picoboot requires writes to be 256-byte page aligned, so sub-page writes are
/// buffered and flushed when a different page is accessed or when `flush` is called.
pub struct PicobootNorFlash<'a, T: UsbContext> {
    conn: &'a mut PicobootConnection<T>,
    /// Buffered page contents (written to device on flush)
    page_buf: [u8; PICO_PAGE_SIZE as usize],
    /// Absolute physical address of the buffered page, or None if clean
    page_buf_addr: Option<u32>,
}

impl<'a, T: UsbContext> PicobootNorFlash<'a, T> {
    pub fn new(conn: &'a mut PicobootConnection<T>) -> Self {
        Self {
            conn,
            page_buf: [0xFF; PICO_PAGE_SIZE as usize],
            page_buf_addr: None,
        }
    }

    /// Write the dirty page buffer to the device if one exists.
    pub fn flush(&mut self) -> Result<(), PicobootNorFlashError> {
        if let Some(addr) = self.page_buf_addr.take() {
            self.conn
                .flash_write(addr, &self.page_buf)
                .map_err(|e| PicobootNorFlashError(e.to_string()))?;
            self.page_buf = [0xFF; PICO_PAGE_SIZE as usize];
        }
        Ok(())
    }
}

impl<T: UsbContext> ErrorType for PicobootNorFlash<'_, T> {
    type Error = PicobootNorFlashError;
}

impl<T: UsbContext> ReadNorFlash for PicobootNorFlash<'_, T> {
    // Match embassy-rp's async Flash READ_SIZE so WORD_SIZE (= max(WRITE_SIZE, READ_SIZE))
    // is identical to the firmware's, keeping the sequential-storage binary layout compatible.
    const READ_SIZE: usize = 4;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        // Flush dirty buffer so the device reflects current writes before reading
        self.flush()?;
        // Picoboot requires page-aligned reads of at least PICO_PAGE_SIZE bytes.
        // Read full pages and return the requested slice.
        let mut pos = 0usize;
        while pos < bytes.len() {
            let abs_addr = RP2040_FLASH_OFFSET as u32 + offset + pos as u32;
            let page_base = abs_addr - (abs_addr % PICO_PAGE_SIZE);
            let page_offset = (abs_addr - page_base) as usize;
            let chunk_len = (PICO_PAGE_SIZE as usize - page_offset).min(bytes.len() - pos);
            let data = self
                .conn
                .flash_read(page_base, PICO_PAGE_SIZE)
                .map_err(|e| PicobootNorFlashError(e.to_string()))?;
            bytes[pos..pos + chunk_len]
                .copy_from_slice(&data[page_offset..page_offset + chunk_len]);
            pos += chunk_len;
        }
        Ok(())
    }

    fn capacity(&self) -> usize {
        PICO_FLASH_SIZE
    }
}

impl<T: UsbContext> NorFlash for PicobootNorFlash<'_, T> {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = PICO_SECTOR_SIZE as usize;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let abs_from = RP2040_FLASH_OFFSET as u32 + from;
        let abs_to = RP2040_FLASH_OFFSET as u32 + to;

        // Discard the dirty buffer if it falls within the erased range; otherwise flush it
        if let Some(buf_addr) = self.page_buf_addr {
            if buf_addr >= abs_from && buf_addr < abs_to {
                self.page_buf_addr = None;
                self.page_buf = [0xFF; PICO_PAGE_SIZE as usize];
            } else {
                self.flush()?;
            }
        }

        let mut addr = abs_from;
        while addr < abs_to {
            self.conn
                .flash_erase(addr, PICO_SECTOR_SIZE)
                .map_err(|e| PicobootNorFlashError(e.to_string()))?;
            addr += PICO_SECTOR_SIZE;
        }
        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let mut remaining = bytes;
        let mut current_offset = offset;

        while !remaining.is_empty() {
            let abs_addr = RP2040_FLASH_OFFSET as u32 + current_offset;
            let page_base = abs_addr - (abs_addr % PICO_PAGE_SIZE);
            let page_offset = (abs_addr - page_base) as usize;
            let chunk_len = (PICO_PAGE_SIZE as usize - page_offset).min(remaining.len());

            if self.page_buf_addr != Some(page_base) {
                self.flush()?;
                // No read-modify-write needed: sequential-storage always erases (→ 0xFF)
                // before writing, so unwritten bytes in the page are already 0xFF.
                self.page_buf = [0xFF; PICO_PAGE_SIZE as usize];
                self.page_buf_addr = Some(page_base);
            }

            // NOR flash: can only clear bits (1→0), never set them (0→1)
            for (i, &byte) in remaining[..chunk_len].iter().enumerate() {
                self.page_buf[page_offset + i] &= byte;
            }

            remaining = &remaining[chunk_len..];
            current_offset += chunk_len as u32;
        }

        Ok(())
    }
}

impl<T: UsbContext> Drop for PicobootNorFlash<'_, T> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}
