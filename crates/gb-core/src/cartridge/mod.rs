//! Cartridge hardware for the DMG Game Boy.
//!
//! This currently stores raw ROM bytes and parses the cartridge title.
//! Mapper selection will be added in later cartridge milestones.

mod header;

pub use header::{CartridgeHeader, CartridgeType, RamSize, RomSize};
use std::{error::Error, fmt};

const ROM_ADDRESS_END: u16 = 0x7FFF;
const ROM_BANK_SIZE: usize = 0x4000;
const RAM_BANK_SIZE: usize = 0x2000;

/// A loaded Game Boy cartridge ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartridge {
    rom: Vec<u8>,
    ram: Vec<u8>,
    header: CartridgeHeader,
    lower_rom_bank_bits: u8,
    upper_bank_bits: u8,
    banking_mode: BankingMode,
    ram_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BankingMode {
    Rom,
    Ram,
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

        Ok(Self {
            rom: bytes,
            ram: vec![0; header.ram_size().bytes()],
            header,
            lower_rom_bank_bits: 1,
            upper_bank_bits: 0,
            banking_mode: BankingMode::Rom,
            ram_enabled: false,
        })
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

        let index = match (self.header.cartridge_type(), address) {
            (cartridge_type, 0x0000..=0x3FFF) if cartridge_type.is_mbc1() => {
                self.mbc1_fixed_bank() * ROM_BANK_SIZE + usize::from(address)
            }
            (cartridge_type, 0x4000..=0x7FFF) if cartridge_type.is_mbc1() => {
                self.mbc1_switchable_bank() * ROM_BANK_SIZE + usize::from(address - 0x4000)
            }
            _ => usize::from(address),
        };

        self.rom
            .get(index)
            .copied()
            .ok_or(CartridgeReadError::MissingRomByte { address })
    }

    /// Handles writes to cartridge control registers.
    ///
    /// ROM-only cartridges ignore these writes.
    pub fn write_rom(&mut self, address: u16, value: u8) {
        if !self.header.cartridge_type().is_mbc1() {
            return;
        }

        match address {
            0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
            0x2000..=0x3FFF => {
                let bank = value & 0x1F;
                self.lower_rom_bank_bits = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => self.upper_bank_bits = value & 0x03,
            0x6000..=0x7FFF => {
                self.banking_mode = if value & 0x01 == 0 {
                    BankingMode::Rom
                } else {
                    BankingMode::Ram
                };
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn read_ram(&self, address: u16) -> u8 {
        if !(0xA000..=0xBFFF).contains(&address) || !self.ram_enabled || self.ram.is_empty() {
            return 0xFF;
        }

        let index = self.selected_ram_bank() * RAM_BANK_SIZE + usize::from(address - 0xA000);
        self.ram.get(index).copied().unwrap_or(0xFF)
    }

    pub fn write_ram(&mut self, address: u16, value: u8) {
        if !(0xA000..=0xBFFF).contains(&address) || !self.ram_enabled || self.ram.is_empty() {
            return;
        }

        let index = self.selected_ram_bank() * RAM_BANK_SIZE + usize::from(address - 0xA000);
        if let Some(byte) = self.ram.get_mut(index) {
            *byte = value;
        }
    }

    #[must_use]
    pub fn save_ram(&self) -> Option<&[u8]> {
        self.header
            .cartridge_type()
            .has_battery()
            .then_some(self.ram.as_slice())
            .filter(|ram| !ram.is_empty())
    }

    /// Restores external cartridge RAM from save data.
    ///
    /// # Errors
    ///
    /// Returns [`SaveRamError::NoExternalRam`] when the cartridge has no
    /// external RAM, or [`SaveRamError::WrongLength`] when the save data does
    /// not match the cartridge RAM size.
    pub fn load_save_ram(&mut self, data: &[u8]) -> Result<(), SaveRamError> {
        if self.ram.is_empty() {
            return Err(SaveRamError::NoExternalRam);
        }

        if data.len() != self.ram.len() {
            return Err(SaveRamError::WrongLength {
                expected: self.ram.len(),
                actual: data.len(),
            });
        }

        self.ram.copy_from_slice(data);
        Ok(())
    }

    fn mbc1_fixed_bank(&self) -> usize {
        match self.banking_mode {
            BankingMode::Rom => 0,
            BankingMode::Ram => usize::from(self.upper_bank_bits) << 5,
        }
    }

    fn mbc1_switchable_bank(&self) -> usize {
        (usize::from(self.upper_bank_bits) << 5) | usize::from(self.lower_rom_bank_bits)
    }

    fn selected_ram_bank(&self) -> usize {
        match self.banking_mode {
            BankingMode::Rom => 0,
            BankingMode::Ram => usize::from(self.upper_bank_bits),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRamError {
    NoExternalRam,
    WrongLength { expected: usize, actual: usize },
}

impl fmt::Display for SaveRamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoExternalRam => formatter.write_str("cartridge has no external RAM"),
            Self::WrongLength { expected, actual } => write!(
                formatter,
                "save RAM length mismatch: expected {expected} bytes, got {actual}"
            ),
        }
    }
}

impl Error for SaveRamError {}

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
    use super::{
        header::calculate_header_checksum, Cartridge, CartridgeError, CartridgeReadError,
        SaveRamError, RAM_BANK_SIZE, ROM_BANK_SIZE,
    };

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

    fn fake_mbc1_rom_with_banks() -> Vec<u8> {
        let mut rom = fake_rom(b"MBC1BANK", 0x03, 0x05, 0x03);
        rom.resize(64 * ROM_BANK_SIZE, 0);
        for bank in 0..64 {
            rom[bank * ROM_BANK_SIZE] = u8::try_from(bank).expect("test bank fits in u8");
        }
        rom[0x0150] = 0x11;
        rom[HEADER_CHECKSUM_ADDR] =
            calculate_header_checksum(&rom).expect("fake ROM should contain full header");
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
        let rom = fake_rom(b"ODDTYPE", 0xFC, 0x00, 0x00);

        let cartridge = Cartridge::from_bytes(rom).expect("unsupported type should still parse");

        assert_eq!(
            cartridge.cartridge_type().to_string(),
            "Unsupported (0xFC)",
            "unrecognized cartridge type codes should be represented clearly"
        );
    }

    #[test]
    fn from_bytes_decodes_mbc1_header_for_external_test_roms() {
        let rom = fake_rom(b"MBC1TEST", 0x01, 0x00, 0x00);

        let cartridge = Cartridge::from_bytes(rom).expect("MBC1 header should parse");

        assert_eq!(
            cartridge.cartridge_type().to_string(),
            "MBC1",
            "cartridge type code 0x01 should decode as MBC1"
        );
    }

    #[test]
    fn read_rom_tolerates_32_kib_mbc1_roms_without_banking() {
        let mut rom = fake_rom(b"MBC1TEST", 0x01, 0x00, 0x00);
        rom[0x4000] = 0x99;
        let cartridge = Cartridge::from_bytes(rom).expect("MBC1 header should parse");

        let byte = cartridge
            .read_rom(0x4000)
            .expect("0x4000 should be readable from a 32 KiB MBC1-labelled ROM");

        assert_eq!(
            byte, 0x99,
            "32 KiB MBC1-labelled test ROMs should read like fixed ROM until MBC1 banking exists"
        );
    }

    #[test]
    fn mbc1_reads_selected_rom_bank_in_switchable_region() {
        let rom = fake_mbc1_rom_with_banks();
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC1 header should parse");

        assert_eq!(
            cartridge.read_rom(0x0150),
            Ok(0x11),
            "bank 0 region should always read from bank 0"
        );
        assert_eq!(
            cartridge.read_rom(0x4000),
            Ok(0x01),
            "switchable region should default to bank 1"
        );

        cartridge.write_rom(0x2000, 0x00);

        assert_eq!(
            cartridge.read_rom(0x4000),
            Ok(0x01),
            "MBC1 bank select value 0 should map to bank 1"
        );

        cartridge.write_rom(0x2000, 0x02);

        assert_eq!(
            cartridge.read_rom(0x4000),
            Ok(0x02),
            "switchable region should read from the selected ROM bank"
        );

        cartridge.write_rom(0x2000, 0x03);

        assert_eq!(
            cartridge.read_rom(0x4000),
            Ok(0x03),
            "MBC1 should support selecting another available lower-bit ROM bank"
        );
    }

    #[test]
    fn mbc1_uses_upper_rom_bank_bits_in_rom_banking_mode() {
        let rom = fake_mbc1_rom_with_banks();
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC1 header should parse");

        cartridge.write_rom(0x2000, 0x02);
        cartridge.write_rom(0x4000, 0x01);

        assert_eq!(
            cartridge.read_rom(0x0000),
            Ok(0x00),
            "bank 0 region should stay fixed while MBC1 is in ROM banking mode"
        );
        assert_eq!(
            cartridge.read_rom(0x4000),
            Ok(0x22),
            "switchable region should combine upper bits with lower ROM bank bits"
        );
    }

    #[test]
    fn mbc1_ram_banking_mode_remaps_bank_zero_region() {
        let rom = fake_mbc1_rom_with_banks();
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC1 header should parse");

        cartridge.write_rom(0x4000, 0x01);
        cartridge.write_rom(0x6000, 0x01);

        assert_eq!(
            cartridge.read_rom(0x0000),
            Ok(0x20),
            "MBC1 RAM banking mode should apply upper bank bits to the 0000-3FFF ROM region"
        );
    }

    #[test]
    fn mbc1_external_ram_requires_enable() {
        let rom = fake_mbc1_rom_with_banks();
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC1 header should parse");

        cartridge.write_ram(0xA000, 0x42);
        assert_eq!(
            cartridge.read_ram(0xA000),
            0xFF,
            "disabled external RAM should read as open bus"
        );

        cartridge.write_rom(0x0000, 0x0A);
        cartridge.write_ram(0xA000, 0x42);

        assert_eq!(
            cartridge.read_ram(0xA000),
            0x42,
            "enabled external RAM should store written bytes"
        );
    }

    #[test]
    fn mbc1_ram_banking_selects_external_ram_bank() {
        let rom = fake_mbc1_rom_with_banks();
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC1 header should parse");

        cartridge.write_rom(0x0000, 0x0A);
        cartridge.write_rom(0x6000, 0x01);
        cartridge.write_rom(0x4000, 0x00);
        cartridge.write_ram(0xA000, 0x10);
        cartridge.write_rom(0x4000, 0x01);
        cartridge.write_ram(0xA000, 0x20);

        cartridge.write_rom(0x4000, 0x00);
        assert_eq!(cartridge.read_ram(0xA000), 0x10);
        cartridge.write_rom(0x4000, 0x01);
        assert_eq!(cartridge.read_ram(0xA000), 0x20);
    }

    #[test]
    fn battery_backed_ram_can_be_extracted_and_restored() {
        let rom = fake_mbc1_rom_with_banks();
        let mut cartridge = Cartridge::from_bytes(rom.clone()).expect("MBC1 header should parse");
        cartridge.write_rom(0x0000, 0x0A);
        cartridge.write_ram(0xA000, 0x5A);

        let save = cartridge
            .save_ram()
            .expect("battery-backed MBC1 RAM should be exposed")
            .to_vec();

        assert_eq!(save.len(), 4 * RAM_BANK_SIZE);

        let mut restored = Cartridge::from_bytes(rom).expect("MBC1 header should parse");
        restored
            .load_save_ram(&save)
            .expect("matching save RAM length should load");
        restored.write_rom(0x0000, 0x0A);

        assert_eq!(
            restored.read_ram(0xA000),
            0x5A,
            "loaded save RAM should be visible through cartridge RAM reads"
        );
    }

    #[test]
    fn save_ram_restore_rejects_wrong_length() {
        let rom = fake_mbc1_rom_with_banks();
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC1 header should parse");

        let error = cartridge
            .load_save_ram(&[0x00])
            .expect_err("wrong save RAM length should be rejected");

        assert_eq!(
            error,
            SaveRamError::WrongLength {
                expected: 4 * RAM_BANK_SIZE,
                actual: 1
            }
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
