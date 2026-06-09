use super::CartridgeError;
use std::fmt;

const TITLE_START: usize = 0x0134;
const TITLE_END_INCLUSIVE: usize = 0x0143;
const TITLE_LEN: usize = TITLE_END_INCLUSIVE - TITLE_START + 1;
const CARTRIDGE_TYPE_ADDR: usize = 0x0147;
const ROM_SIZE_ADDR: usize = 0x0148;
const RAM_SIZE_ADDR: usize = 0x0149;
const HEADER_CHECKSUM_START: usize = 0x0134;
const HEADER_CHECKSUM_END_INCLUSIVE: usize = 0x014C;
const HEADER_CHECKSUM_ADDR: usize = 0x014D;

/// Parsed cartridge header fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeHeader {
    title: String,
    cartridge_type: CartridgeType,
    rom_size: RomSize,
    ram_size: RamSize,
}

impl CartridgeHeader {
    /// Parses the currently supported cartridge header fields from ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CartridgeError::HeaderTooShort`] when the ROM does not contain
    /// the supported header fields.
    pub fn from_rom(rom: &[u8]) -> Result<Self, CartridgeError> {
        let title_end = TITLE_START + TITLE_LEN;
        let title_bytes = rom
            .get(TITLE_START..title_end)
            .ok_or(CartridgeError::HeaderTooShort)?;
        let cartridge_type_code = *rom
            .get(CARTRIDGE_TYPE_ADDR)
            .ok_or(CartridgeError::HeaderTooShort)?;
        let rom_capacity_code = *rom
            .get(ROM_SIZE_ADDR)
            .ok_or(CartridgeError::HeaderTooShort)?;
        let external_ram_code = *rom
            .get(RAM_SIZE_ADDR)
            .ok_or(CartridgeError::HeaderTooShort)?;
        let actual_checksum = *rom
            .get(HEADER_CHECKSUM_ADDR)
            .ok_or(CartridgeError::HeaderTooShort)?;
        let expected_checksum = calculate_header_checksum(rom)?;

        if actual_checksum != expected_checksum {
            return Err(CartridgeError::InvalidHeaderChecksum {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        Ok(Self {
            title: parse_title(title_bytes),
            cartridge_type: CartridgeType::from_code(cartridge_type_code),
            rom_size: RomSize::from_code(rom_capacity_code),
            ram_size: RamSize::from_code(external_ram_code),
        })
    }

    /// Returns the parsed cartridge title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the decoded cartridge type.
    #[must_use]
    pub fn cartridge_type(&self) -> CartridgeType {
        self.cartridge_type
    }

    /// Returns the decoded ROM size.
    #[must_use]
    pub fn rom_size(&self) -> RomSize {
        self.rom_size
    }

    /// Returns the decoded external RAM size.
    #[must_use]
    pub fn ram_size(&self) -> RamSize {
        self.ram_size
    }
}

fn parse_title(title_bytes: &[u8]) -> String {
    let title_len = title_bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(title_bytes.len());

    String::from_utf8_lossy(&title_bytes[..title_len]).into_owned()
}

pub(crate) fn calculate_header_checksum(rom: &[u8]) -> Result<u8, CartridgeError> {
    let header_bytes = rom
        .get(HEADER_CHECKSUM_START..=HEADER_CHECKSUM_END_INCLUSIVE)
        .ok_or(CartridgeError::HeaderTooShort)?;

    Ok(header_bytes.iter().fold(0_u8, |checksum, byte| {
        checksum.wrapping_sub(*byte).wrapping_sub(1)
    }))
}

/// Decoded cartridge type header value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartridgeType {
    RomOnly,
    Unsupported(u8),
}

impl CartridgeType {
    #[must_use]
    fn from_code(code: u8) -> Self {
        match code {
            0x00 => Self::RomOnly,
            unsupported => Self::Unsupported(unsupported),
        }
    }
}

impl fmt::Display for CartridgeType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RomOnly => formatter.write_str("ROM ONLY"),
            Self::Unsupported(code) => write!(formatter, "Unsupported (0x{code:02X})"),
        }
    }
}

/// Decoded ROM size header value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RomSize {
    KiB32,
    KiB64,
    KiB128,
    KiB256,
    KiB512,
    MiB1,
    MiB2,
    MiB4,
    MiB8,
    MiB1Point1,
    MiB1Point2,
    MiB1Point5,
    Unknown(u8),
}

impl RomSize {
    #[must_use]
    fn from_code(code: u8) -> Self {
        match code {
            0x00 => Self::KiB32,
            0x01 => Self::KiB64,
            0x02 => Self::KiB128,
            0x03 => Self::KiB256,
            0x04 => Self::KiB512,
            0x05 => Self::MiB1,
            0x06 => Self::MiB2,
            0x07 => Self::MiB4,
            0x08 => Self::MiB8,
            0x52 => Self::MiB1Point1,
            0x53 => Self::MiB1Point2,
            0x54 => Self::MiB1Point5,
            unknown => Self::Unknown(unknown),
        }
    }
}

impl fmt::Display for RomSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KiB32 => formatter.write_str("32 KiB"),
            Self::KiB64 => formatter.write_str("64 KiB"),
            Self::KiB128 => formatter.write_str("128 KiB"),
            Self::KiB256 => formatter.write_str("256 KiB"),
            Self::KiB512 => formatter.write_str("512 KiB"),
            Self::MiB1 => formatter.write_str("1 MiB"),
            Self::MiB2 => formatter.write_str("2 MiB"),
            Self::MiB4 => formatter.write_str("4 MiB"),
            Self::MiB8 => formatter.write_str("8 MiB"),
            Self::MiB1Point1 => formatter.write_str("1.1 MiB"),
            Self::MiB1Point2 => formatter.write_str("1.2 MiB"),
            Self::MiB1Point5 => formatter.write_str("1.5 MiB"),
            Self::Unknown(code) => write!(formatter, "Unknown (0x{code:02X})"),
        }
    }
}

/// Decoded external RAM size header value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamSize {
    None,
    KiB2,
    KiB8,
    KiB32,
    KiB64,
    KiB128,
    Unknown(u8),
}

impl RamSize {
    #[must_use]
    fn from_code(code: u8) -> Self {
        match code {
            0x00 => Self::None,
            0x01 => Self::KiB2,
            0x02 => Self::KiB8,
            0x03 => Self::KiB32,
            0x04 => Self::KiB128,
            0x05 => Self::KiB64,
            unknown => Self::Unknown(unknown),
        }
    }
}

impl fmt::Display for RamSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("None"),
            Self::KiB2 => formatter.write_str("2 KiB"),
            Self::KiB8 => formatter.write_str("8 KiB"),
            Self::KiB32 => formatter.write_str("32 KiB"),
            Self::KiB64 => formatter.write_str("64 KiB"),
            Self::KiB128 => formatter.write_str("128 KiB"),
            Self::Unknown(code) => write!(formatter, "Unknown (0x{code:02X})"),
        }
    }
}
