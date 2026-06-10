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
    Mbc1,
    Mbc1Ram,
    Mbc1RamBattery,
    Mbc2,
    Mbc2Battery,
    Mbc3TimerBattery,
    Mbc3TimerRamBattery,
    Mbc3,
    Mbc3Ram,
    Mbc3RamBattery,
    Mbc5,
    Mbc5Ram,
    Mbc5RamBattery,
    Mbc5Rumble,
    Mbc5RumbleRam,
    Mbc5RumbleRamBattery,
    Mbc6,
    Mbc7SensorRumbleRamBattery,
    Mmm01,
    Mmm01Ram,
    Mmm01RamBattery,
    Mbc30,
    Unsupported(u8),
}

impl CartridgeType {
    #[must_use]
    fn from_code(code: u8) -> Self {
        match code {
            0x00 => Self::RomOnly,
            0x01 => Self::Mbc1,
            0x02 => Self::Mbc1Ram,
            0x03 => Self::Mbc1RamBattery,
            0x05 => Self::Mbc2,
            0x06 => Self::Mbc2Battery,
            0x0F => Self::Mbc3TimerBattery,
            0x10 => Self::Mbc3TimerRamBattery,
            0x11 => Self::Mbc3,
            0x12 => Self::Mbc3Ram,
            0x13 => Self::Mbc3RamBattery,
            0x19 => Self::Mbc5,
            0x1A => Self::Mbc5Ram,
            0x1B => Self::Mbc5RamBattery,
            0x1C => Self::Mbc5Rumble,
            0x1D => Self::Mbc5RumbleRam,
            0x1E => Self::Mbc5RumbleRamBattery,
            0x20 => Self::Mbc6,
            0x22 => Self::Mbc7SensorRumbleRamBattery,
            0x0B => Self::Mmm01,
            0x0C => Self::Mmm01Ram,
            0x0D => Self::Mmm01RamBattery,
            0xFC => Self::Mbc30,
            unsupported => Self::Unsupported(unsupported),
        }
    }
}

impl CartridgeType {
    #[must_use]
    pub fn is_mbc1(self) -> bool {
        matches!(self, Self::Mbc1 | Self::Mbc1Ram | Self::Mbc1RamBattery)
    }

    #[must_use]
    pub fn is_mbc2(self) -> bool {
        matches!(self, Self::Mbc2 | Self::Mbc2Battery)
    }

    #[must_use]
    pub fn is_mbc3(self) -> bool {
        matches!(
            self,
            Self::Mbc3
                | Self::Mbc3Ram
                | Self::Mbc3RamBattery
                | Self::Mbc3TimerBattery
                | Self::Mbc3TimerRamBattery
        )
    }

    #[must_use]
    pub fn is_mbc5(self) -> bool {
        matches!(
            self,
            Self::Mbc5
                | Self::Mbc5Ram
                | Self::Mbc5RamBattery
                | Self::Mbc5Rumble
                | Self::Mbc5RumbleRam
                | Self::Mbc5RumbleRamBattery
                | Self::Mbc30
        )
    }

    #[must_use]
    pub fn has_rtc(self) -> bool {
        matches!(self, Self::Mbc3TimerBattery | Self::Mbc3TimerRamBattery)
    }

    #[must_use]
    pub fn has_battery(self) -> bool {
        matches!(
            self,
            Self::Mbc1RamBattery
                | Self::Mbc2Battery
                | Self::Mbc3TimerBattery
                | Self::Mbc3TimerRamBattery
                | Self::Mbc3RamBattery
                | Self::Mbc5RamBattery
                | Self::Mbc5RumbleRamBattery
                | Self::Mbc7SensorRumbleRamBattery
                | Self::Mmm01RamBattery
                | Self::Mbc30
        )
    }
}

impl fmt::Display for CartridgeType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RomOnly => formatter.write_str("ROM ONLY"),
            Self::Mbc1 => formatter.write_str("MBC1"),
            Self::Mbc1Ram => formatter.write_str("MBC1+RAM"),
            Self::Mbc1RamBattery => formatter.write_str("MBC1+RAM+BATTERY"),
            Self::Mbc2 => formatter.write_str("MBC2"),
            Self::Mbc2Battery => formatter.write_str("MBC2+BATTERY"),
            Self::Mbc3TimerBattery => formatter.write_str("MBC3+TIMER+BATTERY"),
            Self::Mbc3TimerRamBattery => formatter.write_str("MBC3+TIMER+RAM+BATTERY"),
            Self::Mbc3 => formatter.write_str("MBC3"),
            Self::Mbc3Ram => formatter.write_str("MBC3+RAM"),
            Self::Mbc3RamBattery => formatter.write_str("MBC3+RAM+BATTERY"),
            Self::Mbc5 => formatter.write_str("MBC5"),
            Self::Mbc5Ram => formatter.write_str("MBC5+RAM"),
            Self::Mbc5RamBattery => formatter.write_str("MBC5+RAM+BATTERY"),
            Self::Mbc5Rumble => formatter.write_str("MBC5+RUMBLE"),
            Self::Mbc5RumbleRam => formatter.write_str("MBC5+RUMBLE+RAM"),
            Self::Mbc5RumbleRamBattery => formatter.write_str("MBC5+RUMBLE+RAM+BATTERY"),
            Self::Mbc6 => formatter.write_str("MBC6"),
            Self::Mbc7SensorRumbleRamBattery => {
                formatter.write_str("MBC7+SENSOR+RUMBLE+RAM+BATTERY")
            }
            Self::Mmm01 => formatter.write_str("MMM01"),
            Self::Mmm01Ram => formatter.write_str("MMM01+RAM"),
            Self::Mmm01RamBattery => formatter.write_str("MMM01+RAM+BATTERY"),
            Self::Mbc30 => formatter.write_str("MBC30"),
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

    #[must_use]
    pub fn bytes(self) -> usize {
        match self {
            Self::None | Self::Unknown(_) => 0,
            Self::KiB2 => 2 * 1024,
            Self::KiB8 => 8 * 1024,
            Self::KiB32 => 32 * 1024,
            Self::KiB64 => 64 * 1024,
            Self::KiB128 => 128 * 1024,
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
