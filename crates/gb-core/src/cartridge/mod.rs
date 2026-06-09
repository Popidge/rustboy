//! Cartridge hardware for the DMG Game Boy.
//!
//! This currently stores raw ROM bytes and parses the cartridge title.
//! Mapper selection will be added in later cartridge milestones.

mod header;

pub use header::{CartridgeHeader, CartridgeType, RamSize, RomSize};
use std::{error::Error, fmt};

const ROM_ADDRESS_END: u16 = 0x7FFF;

/// A loaded Game Boy cartridge ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartridge {
    rom: Vec<u8>,
    header: CartridgeHeader,
}

/// Errors that can occur while loading cartridge bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartridgeError {
    /// A cartridge image must contain at least one byte of ROM data.
    EmptyRom,
    /// The ROM is too small to contain the requested header field.
    HeaderTooShort,
    /// The header checksum byte does not match the computed checksum.
    InvalidHeaderChecksum {
        /// Checksum computed from bytes `0x0134..=0x014C`.
        expected: u8,
        /// Checksum stored at byte `0x014D`.
        actual: u8,
    },
}

/// Errors that can occur while reading cartridge ROM data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartridgeReadError {
    /// The address is outside the cartridge ROM address range.
    AddressOutOfRange {
        /// Address requested by the caller.
        address: u16,
    },
    /// The address is in the cartridge ROM range, but the loaded ROM is too short.
    MissingRomByte {
        /// Address requested by the caller.
        address: u16,
    },
}

impl Cartridge {
    /// Creates a cartridge from owned ROM bytes.
    ///
    /// This parses the currently supported header fields and validates the
    /// header checksum.
    ///
    /// # Errors
    ///
    /// Returns [`CartridgeError::EmptyRom`] when `bytes` is empty,
    /// [`CartridgeError::HeaderTooShort`] when a supported header field is not
    /// present, or [`CartridgeError::InvalidHeaderChecksum`] when the stored
    /// checksum does not match the computed checksum.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, CartridgeError> {
        if bytes.is_empty() {
            return Err(CartridgeError::EmptyRom);
        }

        let header = CartridgeHeader::from_rom(&bytes)?;

        Ok(Self { rom: bytes, header })
    }

    /// Returns the number of ROM bytes stored by this cartridge.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rom.len()
    }

    /// Returns true when the cartridge contains no ROM bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rom.is_empty()
    }

    /// Returns the parsed cartridge title.
    #[must_use]
    pub fn title(&self) -> &str {
        self.header.title()
    }

    /// Returns the decoded cartridge type.
    #[must_use]
    pub fn cartridge_type(&self) -> CartridgeType {
        self.header.cartridge_type()
    }

    /// Returns the decoded ROM size.
    #[must_use]
    pub fn rom_size(&self) -> RomSize {
        self.header.rom_size()
    }

    /// Returns the decoded external RAM size.
    #[must_use]
    pub fn ram_size(&self) -> RamSize {
        self.header.ram_size()
    }

    /// Reads one byte from the cartridge ROM address range.
    ///
    /// # Errors
    ///
    /// Returns [`CartridgeReadError::AddressOutOfRange`] when `address` is
    /// outside `0x0000..=0x7FFF`, or [`CartridgeReadError::MissingRomByte`]
    /// when the loaded ROM does not contain that address.
    pub fn read_rom(&self, address: u16) -> Result<u8, CartridgeReadError> {
        if address > ROM_ADDRESS_END {
            return Err(CartridgeReadError::AddressOutOfRange { address });
        }

        self.rom
            .get(usize::from(address))
            .copied()
            .ok_or(CartridgeReadError::MissingRomByte { address })
    }
}

impl fmt::Display for CartridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRom => formatter.write_str("cartridge ROM is empty"),
            Self::HeaderTooShort => formatter.write_str("cartridge ROM is too short for header"),
            Self::InvalidHeaderChecksum { expected, actual } => write!(
                formatter,
                "invalid cartridge header checksum: expected 0x{expected:02X}, got 0x{actual:02X}"
            ),
        }
    }
}

impl Error for CartridgeError {}

impl fmt::Display for CartridgeReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddressOutOfRange { address } => {
                write!(
                    formatter,
                    "cartridge ROM address 0x{address:04X} is out of range"
                )
            }
            Self::MissingRomByte { address } => {
                write!(
                    formatter,
                    "cartridge ROM byte 0x{address:04X} is not loaded"
                )
            }
        }
    }
}

impl Error for CartridgeReadError {}

#[cfg(test)]
mod tests {
    use super::{header::calculate_header_checksum, Cartridge, CartridgeError, CartridgeReadError};

    const ROM_SIZE: usize = 32 * 1024;
    const TITLE_START: usize = 0x0134;
    const CARTRIDGE_TYPE_ADDR: usize = 0x0147;
    const ROM_SIZE_ADDR: usize = 0x0148;
    const RAM_SIZE_ADDR: usize = 0x0149;
    const HEADER_CHECKSUM_ADDR: usize = 0x014D;
    const ENTRY_POINT: usize = 0x0100;

    fn fake_rom(
        title_bytes: &[u8],
        cartridge_type_code: u8,
        rom_capacity_code: u8,
        external_ram_code: u8,
    ) -> Vec<u8> {
        let mut rom = vec![0; ROM_SIZE];
        let title_end = TITLE_START + title_bytes.len();
        rom[TITLE_START..title_end].copy_from_slice(title_bytes);
        rom[CARTRIDGE_TYPE_ADDR] = cartridge_type_code;
        rom[ROM_SIZE_ADDR] = rom_capacity_code;
        rom[RAM_SIZE_ADDR] = external_ram_code;
        rom[HEADER_CHECKSUM_ADDR] =
            calculate_header_checksum(&rom).expect("fake ROM should contain full header");
        rom
    }

    fn fake_rom_with_title(title_bytes: &[u8]) -> Vec<u8> {
        fake_rom(title_bytes, 0x00, 0x00, 0x00)
    }

    fn fake_rom_with_entry_byte(address: usize, value: u8) -> Vec<u8> {
        let mut rom = fake_rom_with_title(b"READTEST");
        rom[address] = value;
        rom
    }

    #[test]
    fn from_bytes_stores_raw_rom_bytes() {
        let rom = fake_rom_with_title(b"RUSTBOY");

        let cartridge = Cartridge::from_bytes(rom).expect("ROM bytes should load");

        assert_eq!(
            cartridge.len(),
            ROM_SIZE,
            "cartridge length should match the raw ROM byte count"
        );
    }

    #[test]
    fn from_bytes_rejects_empty_rom() {
        let error = Cartridge::from_bytes(Vec::new()).expect_err("empty ROM should be rejected");

        assert_eq!(error, CartridgeError::EmptyRom);
    }

    #[test]
    fn from_bytes_parses_cartridge_title() {
        let rom = fake_rom_with_title(b"RUSTBOY");

        let cartridge = Cartridge::from_bytes(rom).expect("ROM title should parse");

        assert_eq!(
            cartridge.title(),
            "RUSTBOY",
            "title should come from bytes 0x0134..=0x0143"
        );
    }

    #[test]
    fn from_bytes_trims_zero_padded_title() {
        let rom = fake_rom_with_title(b"RUSTBOY\0\0\0\0\0\0\0\0\0");

        let cartridge = Cartridge::from_bytes(rom).expect("zero-padded ROM title should parse");

        assert_eq!(
            cartridge.title(),
            "RUSTBOY",
            "zero padding after the title should not be included"
        );
    }

    #[test]
    fn from_bytes_rejects_rom_without_full_title_field() {
        let rom = vec![0; TITLE_START];

        let error = Cartridge::from_bytes(rom).expect_err("short ROM should be rejected");

        assert_eq!(error, CartridgeError::HeaderTooShort);
    }

    #[test]
    fn from_bytes_accepts_valid_header_checksum() {
        let rom = fake_rom(b"CHECKOK", 0x00, 0x00, 0x00);

        let cartridge = Cartridge::from_bytes(rom).expect("valid header checksum should parse");

        assert_eq!(
            cartridge.title(),
            "CHECKOK",
            "valid checksum should allow header parsing to complete"
        );
    }

    #[test]
    fn from_bytes_rejects_invalid_header_checksum() {
        let mut rom = fake_rom(b"CHECKBAD", 0x00, 0x00, 0x00);
        let expected = rom[HEADER_CHECKSUM_ADDR];
        rom[HEADER_CHECKSUM_ADDR] = expected.wrapping_add(1);

        let error = Cartridge::from_bytes(rom).expect_err("invalid checksum should be rejected");

        assert_eq!(
            error,
            CartridgeError::InvalidHeaderChecksum {
                expected,
                actual: expected.wrapping_add(1)
            }
        );
    }

    #[test]
    fn from_bytes_decodes_rom_only_32_kib_with_no_ram() {
        let rom = fake_rom(b"TETRIS", 0x00, 0x00, 0x00);

        let cartridge = Cartridge::from_bytes(rom).expect("ROM-only header should parse");

        assert_eq!(
            cartridge.cartridge_type().to_string(),
            "ROM ONLY",
            "cartridge type code 0x00 should decode as ROM ONLY"
        );
        assert_eq!(
            cartridge.rom_size().to_string(),
            "32 KiB",
            "ROM size code 0x00 should decode as 32 KiB"
        );
        assert_eq!(
            cartridge.ram_size().to_string(),
            "None",
            "RAM size code 0x00 should decode as no external RAM"
        );
    }

    #[test]
    fn from_bytes_represents_unsupported_cartridge_type() {
        let rom = fake_rom(b"MBC1GAME", 0x01, 0x00, 0x00);

        let cartridge = Cartridge::from_bytes(rom).expect("unsupported type should still parse");

        assert_eq!(
            cartridge.cartridge_type().to_string(),
            "Unsupported (0x01)",
            "non-ROM-only cartridge types should be represented clearly"
        );
    }

    #[test]
    fn from_bytes_decodes_larger_rom_and_ram_size_codes() {
        let rom = fake_rom(b"RAMGAME", 0x00, 0x02, 0x03);

        let cartridge = Cartridge::from_bytes(rom).expect("size codes should parse");

        assert_eq!(
            cartridge.rom_size().to_string(),
            "128 KiB",
            "ROM size code 0x02 should decode as 128 KiB"
        );
        assert_eq!(
            cartridge.ram_size().to_string(),
            "32 KiB",
            "RAM size code 0x03 should decode as 32 KiB"
        );
    }

    #[test]
    fn read_rom_returns_byte_at_address() {
        let rom = fake_rom_with_entry_byte(ENTRY_POINT, 0x42);
        let cartridge = Cartridge::from_bytes(rom).expect("ROM should load");

        let byte = cartridge
            .read_rom(0x0100)
            .expect("0x0100 should be in ROM range");

        assert_eq!(
            byte, 0x42,
            "reading 0x0100 should return the ROM byte stored at 0x0100"
        );
    }

    #[test]
    fn read_rom_rejects_address_above_rom_range() {
        let rom = fake_rom_with_title(b"READTEST");
        let cartridge = Cartridge::from_bytes(rom).expect("ROM should load");

        let error = cartridge
            .read_rom(0x8000)
            .expect_err("0x8000 is outside cartridge ROM range");

        assert_eq!(
            error,
            CartridgeReadError::AddressOutOfRange { address: 0x8000 }
        );
    }

    #[test]
    fn read_rom_rejects_missing_rom_byte() {
        let mut rom = fake_rom_with_title(b"SHORTROM");
        rom.truncate(0x2000);
        rom[HEADER_CHECKSUM_ADDR] =
            calculate_header_checksum(&rom).expect("truncated ROM should still contain header");
        let cartridge = Cartridge::from_bytes(rom).expect("short ROM with header should load");

        let error = cartridge
            .read_rom(0x4000)
            .expect_err("0x4000 is in range but missing from the loaded ROM");

        assert_eq!(
            error,
            CartridgeReadError::MissingRomByte { address: 0x4000 }
        );
    }
}
