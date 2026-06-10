//! Cartridge hardware for the DMG Game Boy.
//!
//! This currently stores raw ROM bytes and parses the cartridge title.
//! Mapper selection will be added in later cartridge milestones.

mod header;

pub use header::{CartridgeHeader, CartridgeType, RamSize, RomSize};
use std::{
    error::Error,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

const ROM_ADDRESS_END: u16 = 0x7FFF;
const ROM_BANK_SIZE: usize = 0x4000;
const RAM_BANK_SIZE: usize = 0x2000;
const MBC2_RAM_SIZE: usize = 512;
const MBC30_ACCESSIBLE_RAM_SIZE: usize = 64 * 1024;

/// A loaded Game Boy cartridge ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartridge {
    rom: Vec<u8>,
    ram: Vec<u8>,
    header: CartridgeHeader,
    lower_rom_bank_bits: u8,
    upper_bank_bits: u8,
    mbc3_rom_bank: u8,
    mbc3_ram_or_rtc_select: u8,
    mbc5_rom_bank: u16,
    mbc5_ram_bank: u8,
    rtc: Rtc,
    banking_mode: BankingMode,
    ram_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BankingMode {
    Rom,
    Ram,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rtc {
    offset_seconds: i64,
    halted: bool,
    day_carry: bool,
    latched: Option<RtcRegisters>,
    latch_previous: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RtcRegisters {
    seconds: u8,
    minutes: u8,
    hours: u8,
    day_low: u8,
    day_high: u8,
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
        let ram_size = cartridge_ram_size(header.cartridge_type(), header.ram_size());

        Ok(Self {
            rom: bytes,
            ram: vec![0; ram_size],
            header,
            lower_rom_bank_bits: 1,
            upper_bank_bits: 0,
            mbc3_rom_bank: 1,
            mbc3_ram_or_rtc_select: 0,
            mbc5_rom_bank: 1,
            mbc5_ram_bank: 0,
            rtc: Rtc::new(),
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

        let cartridge_type = self.header.cartridge_type();
        let index = match (cartridge_type, address) {
            (cartridge_type, 0x0000..=0x3FFF) if cartridge_type.is_mbc1() => {
                self.effective_rom_bank(self.mbc1_fixed_bank()) * ROM_BANK_SIZE
                    + usize::from(address)
            }
            (cartridge_type, 0x4000..=0x7FFF) if cartridge_type.is_mbc1() => {
                self.effective_rom_bank(self.mbc1_switchable_bank()) * ROM_BANK_SIZE
                    + usize::from(address - 0x4000)
            }
            (cartridge_type, 0x4000..=0x7FFF) if cartridge_type.is_mbc2() => {
                self.effective_rom_bank(usize::from(self.lower_rom_bank_bits)) * ROM_BANK_SIZE
                    + usize::from(address - 0x4000)
            }
            (cartridge_type, 0x4000..=0x7FFF) if cartridge_type.is_mbc3() => {
                self.effective_rom_bank(usize::from(self.mbc3_rom_bank)) * ROM_BANK_SIZE
                    + usize::from(address - 0x4000)
            }
            (cartridge_type, 0x4000..=0x7FFF) if cartridge_type.is_mbc5() => {
                self.effective_rom_bank(self.mbc5_selected_rom_bank()) * ROM_BANK_SIZE
                    + usize::from(address - 0x4000)
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
        let cartridge_type = self.header.cartridge_type();
        if cartridge_type.is_mbc1() {
            self.write_mbc1_rom(address, value);
        } else if cartridge_type.is_mbc2() {
            self.write_mbc2_rom(address, value);
        } else if cartridge_type.is_mbc3() {
            self.write_mbc3_rom(address, value);
        } else if cartridge_type.is_mbc5() {
            self.write_mbc5_rom(address, value);
        }
    }

    #[must_use]
    pub fn read_ram(&self, address: u16) -> u8 {
        if !(0xA000..=0xBFFF).contains(&address) || !self.ram_enabled {
            return 0xFF;
        }

        let cartridge_type = self.header.cartridge_type();
        if cartridge_type.is_mbc3() && (0x08..=0x0C).contains(&self.mbc3_ram_or_rtc_select) {
            return self.rtc.read(self.mbc3_ram_or_rtc_select);
        }
        if self.ram.is_empty() {
            return 0xFF;
        }
        if cartridge_type.is_mbc2() {
            let index = usize::from(address - 0xA000) & 0x01FF;
            return 0xF0 | (self.ram[index] & 0x0F);
        }

        let index = self.selected_ram_bank() * RAM_BANK_SIZE + usize::from(address - 0xA000);
        self.ram.get(index).copied().unwrap_or(0xFF)
    }

    pub fn write_ram(&mut self, address: u16, value: u8) {
        if !(0xA000..=0xBFFF).contains(&address) || !self.ram_enabled {
            return;
        }

        let cartridge_type = self.header.cartridge_type();
        if cartridge_type.is_mbc3() && (0x08..=0x0C).contains(&self.mbc3_ram_or_rtc_select) {
            self.rtc.write(self.mbc3_ram_or_rtc_select, value);
            return;
        }
        if self.ram.is_empty() {
            return;
        }
        if cartridge_type.is_mbc2() {
            let index = usize::from(address - 0xA000) & 0x01FF;
            self.ram[index] = value & 0x0F;
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
        let cartridge_type = self.header.cartridge_type();
        if cartridge_type.is_mbc1() {
            match self.banking_mode {
                BankingMode::Rom => 0,
                BankingMode::Ram => usize::from(self.upper_bank_bits),
            }
        } else if cartridge_type.is_mbc2() {
            0
        } else if cartridge_type.is_mbc3() {
            usize::from(self.mbc3_ram_or_rtc_select.min(0x03))
        } else if cartridge_type.is_mbc5() {
            if cartridge_type == CartridgeType::Mbc30 {
                usize::from(self.mbc5_ram_bank & 0x07)
            } else {
                usize::from(self.mbc5_ram_bank)
            }
        } else {
            0
        }
    }

    fn write_mbc1_rom(&mut self, address: u16, value: u8) {
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

    fn write_mbc2_rom(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x3FFF if address & 0x0100 == 0 => self.ram_enabled = value & 0x0F == 0x0A,
            0x0000..=0x3FFF => {
                let bank = value & 0x0F;
                self.lower_rom_bank_bits = if bank == 0 { 1 } else { bank };
            }
            _ => {}
        }
    }

    fn write_mbc3_rom(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
            0x2000..=0x3FFF => {
                let bank = value & 0x7F;
                self.mbc3_rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => self.mbc3_ram_or_rtc_select = value,
            0x6000..=0x7FFF => self.rtc.latch(value),
            _ => {}
        }
    }

    fn write_mbc5_rom(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
            0x2000..=0x2FFF => {
                self.mbc5_rom_bank = (self.mbc5_rom_bank & 0x0100) | u16::from(value);
            }
            0x3000..=0x3FFF => {
                self.mbc5_rom_bank = (self.mbc5_rom_bank & 0x00FF) | (u16::from(value & 0x01) << 8);
            }
            0x4000..=0x5FFF => self.mbc5_ram_bank = value & 0x0F,
            _ => {}
        }
    }

    fn effective_rom_bank(&self, bank: usize) -> usize {
        let bank_count = (self.rom.len() / ROM_BANK_SIZE).max(1);

        bank % bank_count
    }

    fn mbc5_selected_rom_bank(&self) -> usize {
        let bank = usize::from(self.mbc5_rom_bank);
        if self.header.cartridge_type() == CartridgeType::Mbc30 {
            bank & 0x1F
        } else {
            bank
        }
    }
}

fn cartridge_ram_size(cartridge_type: CartridgeType, ram_size: RamSize) -> usize {
    if cartridge_type.is_mbc2() {
        MBC2_RAM_SIZE
    } else if cartridge_type == CartridgeType::Mbc30 {
        MBC30_ACCESSIBLE_RAM_SIZE
    } else {
        ram_size.bytes()
    }
}

impl Rtc {
    fn new() -> Self {
        Self {
            offset_seconds: 0,
            halted: false,
            day_carry: false,
            latched: None,
            latch_previous: 0,
        }
    }

    fn read(&self, register: u8) -> u8 {
        let registers = self.current_registers();
        match register {
            0x08 => registers.seconds,
            0x09 => registers.minutes,
            0x0A => registers.hours,
            0x0B => registers.day_low,
            0x0C => registers.day_high,
            _ => 0xFF,
        }
    }

    fn write(&mut self, register: u8, value: u8) {
        let mut registers = self.current_registers();
        match register {
            0x08 => registers.seconds = value.min(59),
            0x09 => registers.minutes = value.min(59),
            0x0A => registers.hours = value.min(23),
            0x0B => registers.day_low = value,
            0x0C => {
                registers.day_high = value & 0xC1;
                self.day_carry = value & 0x80 != 0;
                let was_halted = self.halted;
                self.halted = value & 0x40 != 0;
                if self.halted {
                    self.latched = Some(registers);
                    return;
                }
                if was_halted {
                    self.set_time(registers);
                    self.latched = None;
                    return;
                }
            }
            _ => return,
        }

        self.set_time(registers);
        if self.halted {
            self.latched = Some(registers);
        }
    }

    fn latch(&mut self, value: u8) {
        if self.latch_previous == 0 && value == 1 {
            self.latched = Some(self.current_registers());
        }
        self.latch_previous = value;
    }

    fn current_registers(&self) -> RtcRegisters {
        if self.halted {
            return self.latched.unwrap_or_else(|| self.registers_from_total(0));
        }

        self.latched.unwrap_or_else(|| {
            let total = host_seconds().saturating_add(self.offset_seconds);
            self.registers_from_total(u64::try_from(total.max(0)).unwrap_or(0))
        })
    }

    fn set_time(&mut self, registers: RtcRegisters) {
        let target = i64::try_from(registers.total_seconds()).unwrap_or(i64::MAX);
        self.offset_seconds = target.saturating_sub(host_seconds());
    }

    fn registers_from_total(&self, total_seconds: u64) -> RtcRegisters {
        let seconds = (total_seconds % 60) as u8;
        let minutes = ((total_seconds / 60) % 60) as u8;
        let hours = ((total_seconds / 3600) % 24) as u8;
        let days = total_seconds / 86_400;
        let day_low = (days & 0xFF) as u8;
        let day_bit = ((days >> 8) & 0x01) as u8;
        let carry = self.day_carry || days > 511;
        let day_high = day_bit | (u8::from(self.halted) << 6) | (u8::from(carry) << 7);

        RtcRegisters {
            seconds,
            minutes,
            hours,
            day_low,
            day_high,
        }
    }
}

impl RtcRegisters {
    fn total_seconds(self) -> u64 {
        let days = u64::from(self.day_low) | (u64::from(self.day_high & 0x01) << 8);

        days * 86_400
            + u64::from(self.hours.min(23)) * 3600
            + u64::from(self.minutes.min(59)) * 60
            + u64::from(self.seconds.min(59))
    }
}

fn host_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
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

    fn fake_banked_rom(
        title: &[u8],
        cartridge_type_code: u8,
        rom_capacity_code: u8,
        external_ram_code: u8,
        banks: usize,
    ) -> Vec<u8> {
        let mut rom = fake_rom(
            title,
            cartridge_type_code,
            rom_capacity_code,
            external_ram_code,
        );
        rom.resize(banks * ROM_BANK_SIZE, 0);
        for bank in 0..banks {
            rom[bank * ROM_BANK_SIZE] = u8::try_from(bank & 0xFF).expect("masked bank fits in u8");
        }
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
        let rom = fake_rom(b"ODDTYPE", 0xFD, 0x00, 0x00);

        let cartridge = Cartridge::from_bytes(rom).expect("unsupported type should still parse");

        assert_eq!(
            cartridge.cartridge_type().to_string(),
            "Unsupported (0xFD)",
            "unrecognized cartridge type codes should be represented clearly"
        );
    }

    #[test]
    fn from_bytes_decodes_stage_22_cartridge_families() {
        let cases = [
            (0x06, "MBC2+BATTERY"),
            (0x10, "MBC3+TIMER+RAM+BATTERY"),
            (0x1E, "MBC5+RUMBLE+RAM+BATTERY"),
            (0x20, "MBC6"),
            (0x22, "MBC7+SENSOR+RUMBLE+RAM+BATTERY"),
            (0xFC, "MBC30"),
        ];

        for (code, expected) in cases {
            let rom = fake_rom(b"MAPPER", code, 0x00, 0x00);
            let cartridge = Cartridge::from_bytes(rom).expect("mapper header should parse");

            assert_eq!(
                cartridge.cartridge_type().to_string(),
                expected,
                "cartridge type 0x{code:02X} should decode by name"
            );
        }
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
    fn mbc1_small_roms_mirror_unavailable_upper_bank_bits() {
        let rom = fake_banked_rom(b"MBC1SMALL", 0x01, 0x01, 0x00, 4);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC1 header should parse");

        cartridge.write_rom(0x2000, 0x02);
        cartridge.write_rom(0x4000, 0x01);

        assert_eq!(
            cartridge.read_rom(0x4000),
            Ok(0x02),
            "4-bank MBC1 ROMs should ignore unavailable upper bank select bits"
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
    fn mbc2_uses_address_bit_for_ram_enable_and_bank_selects() {
        let rom = fake_banked_rom(b"MBC2", 0x06, 0x02, 0x00, 16);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC2 header should parse");

        cartridge.write_rom(0x2100, 0x03);
        assert_eq!(
            cartridge.read_rom(0x4000),
            Ok(0x03),
            "MBC2 writes with address bit 8 set should select ROM bank"
        );

        cartridge.write_ram(0xA000, 0x2A);
        assert_eq!(
            cartridge.read_ram(0xA000),
            0xFF,
            "MBC2 RAM should remain disabled until a low address-bit write enables it"
        );

        cartridge.write_rom(0x0000, 0x0A);
        cartridge.write_ram(0xA000, 0x2A);
        assert_eq!(
            cartridge.read_ram(0xA000),
            0xFA,
            "MBC2 internal RAM stores only the lower nibble and reads high bits set"
        );
    }

    #[test]
    fn mbc3_banks_rom_ram_and_rtc_registers() {
        let rom = fake_banked_rom(b"MBC3RTC", 0x10, 0x03, 0x03, 8);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC3 header should parse");

        cartridge.write_rom(0x2000, 0x03);
        assert_eq!(cartridge.read_rom(0x4000), Ok(0x03));

        cartridge.write_rom(0x0000, 0x0A);
        cartridge.write_rom(0x4000, 0x01);
        cartridge.write_ram(0xA000, 0x77);
        cartridge.write_rom(0x4000, 0x00);
        assert_eq!(cartridge.read_ram(0xA000), 0x00);
        cartridge.write_rom(0x4000, 0x01);
        assert_eq!(cartridge.read_ram(0xA000), 0x77);

        cartridge.write_rom(0x4000, 0x0C);
        cartridge.write_ram(0xA000, 0x40);
        cartridge.write_rom(0x4000, 0x08);
        cartridge.write_ram(0xA000, 12);
        cartridge.write_rom(0x4000, 0x09);
        cartridge.write_ram(0xA000, 34);
        cartridge.write_rom(0x4000, 0x0A);
        cartridge.write_ram(0xA000, 5);
        cartridge.write_rom(0x4000, 0x08);
        assert_eq!(cartridge.read_ram(0xA000), 12);
        cartridge.write_rom(0x4000, 0x09);
        assert_eq!(cartridge.read_ram(0xA000), 34);
        cartridge.write_rom(0x4000, 0x0A);
        assert_eq!(cartridge.read_ram(0xA000), 5);
    }

    #[test]
    fn mbc5_supports_nine_bit_rom_banks_and_ram_banks() {
        let rom = fake_banked_rom(b"MBC5", 0x1B, 0x08, 0x03, 258);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC5 header should parse");

        cartridge.write_rom(0x2000, 0x01);
        cartridge.write_rom(0x3000, 0x01);
        assert_eq!(
            cartridge.read_rom(0x4000),
            Ok(0x01),
            "bank 0x101 should be selected even though the test marker wraps to one byte"
        );

        cartridge.write_rom(0x0000, 0x0A);
        cartridge.write_rom(0x4000, 0x02);
        cartridge.write_ram(0xA000, 0x55);
        cartridge.write_rom(0x4000, 0x00);
        assert_eq!(cartridge.read_ram(0xA000), 0x00);
        cartridge.write_rom(0x4000, 0x02);
        assert_eq!(cartridge.read_ram(0xA000), 0x55);
    }

    #[test]
    fn mbc30_ram_bank_bit_three_is_ignored() {
        let rom = fake_banked_rom(b"MBC30", 0xFC, 0x00, 0x05, 2);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC30 header should parse");

        cartridge.write_rom(0x0000, 0x0A);
        cartridge.write_rom(0x4000, 0x00);
        cartridge.write_ram(0xA000, 0x31);
        cartridge.write_rom(0x4000, 0x08);

        assert_eq!(
            cartridge.read_ram(0xA000),
            0x31,
            "MBC30 ignores the high RAM bank select bit, so bank 8 mirrors bank 0"
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
