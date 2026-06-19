//! User-supplied DMG boot-ROM validation and storage.
//!
//! The original boot ROM is not distributed with the emulator. This type holds
//! an explicitly supplied 256-byte image that Bus maps until FF50 disables it.

use std::fmt;

/// Size in bytes of the DMG boot ROM.
pub const DMG_BOOT_ROM_SIZE: usize = 0x100;

/// A validated, user-supplied DMG boot ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmgBootRom {
    bytes: [u8; DMG_BOOT_ROM_SIZE],
}

impl DmgBootRom {
    /// Validates and stores a 256-byte DMG boot ROM image.
    ///
    /// # Errors
    ///
    /// Returns an error when `bytes` is not exactly 256 bytes long.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, BootRomError> {
        let actual = bytes.len();
        let bytes = bytes
            .try_into()
            .map_err(|_| BootRomError::InvalidLength { actual })?;

        Ok(Self { bytes })
    }

    pub(crate) fn read(&self, address: usize) -> u8 {
        self.bytes[address]
    }
}

/// Error returned while loading a DMG boot ROM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootRomError {
    /// A DMG boot ROM must be exactly 256 bytes.
    InvalidLength { actual: usize },
}

impl fmt::Display for BootRomError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { actual } => write!(
                formatter,
                "DMG boot ROM must be exactly {DMG_BOOT_ROM_SIZE} bytes, got {actual}"
            ),
        }
    }
}

impl std::error::Error for BootRomError {}

#[cfg(test)]
mod tests {
    use super::{BootRomError, DmgBootRom, DMG_BOOT_ROM_SIZE};

    #[test]
    fn accepts_exactly_256_bytes() {
        let rom = DmgBootRom::from_bytes(vec![0x42; DMG_BOOT_ROM_SIZE])
            .expect("a 256-byte boot ROM should validate");

        assert_eq!(rom.read(0), 0x42);
        assert_eq!(rom.read(0xFF), 0x42);
    }

    #[test]
    fn rejects_other_boot_rom_lengths() {
        let error = DmgBootRom::from_bytes(vec![0; DMG_BOOT_ROM_SIZE - 1])
            .expect_err("a short boot ROM must be rejected");

        assert_eq!(error, BootRomError::InvalidLength { actual: 255 });
    }
}
