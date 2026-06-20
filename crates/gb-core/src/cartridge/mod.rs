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

use crate::cpu::TCycles;

const ROM_ADDRESS_END: u16 = 0x7FFF;
const ROM_BANK_SIZE: usize = 0x4000;
const RAM_BANK_SIZE: usize = 0x2000;
const MBC2_RAM_SIZE: usize = 512;
const MBC30_ACCESSIBLE_RAM_SIZE: usize = 64 * 1024;
const RTC_SAVE_MAGIC: [u8; 4] = *b"RBRT";
const RTC_SAVE_VERSION: u8 = 3;
const RTC_SAVE_SIZE: usize = 22;
const DMG_TCYCLES_PER_SECOND: u64 = 4_194_304;

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
    registers: RtcRegisters,
    subsecond_tcycles: u32,
    halted: bool,
    day_carry: bool,
    latched: Option<RtcRegisters>,
    latch_previous: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

    /// Advances cartridge-owned hardware by DMG T-cycles.
    pub fn tick(&mut self, cycles: TCycles) {
        if self.header.cartridge_type().has_rtc() {
            self.rtc.tick(cycles);
        }
    }

    #[must_use]
    pub fn read_ram(&self, address: u16) -> u8 {
        if !(0xA000..=0xBFFF).contains(&address) || !self.ram_enabled {
            return 0xFF;
        }

        let cartridge_type = self.header.cartridge_type();
        if cartridge_type.has_rtc() && (0x08..=0x0C).contains(&self.mbc3_ram_or_rtc_select) {
            return self.rtc.read(self.mbc3_ram_or_rtc_select);
        }
        if cartridge_type.is_mbc3() && self.mbc3_ram_or_rtc_select > 0x03 {
            return 0xFF;
        }
        if self.ram.is_empty() {
            return 0xFF;
        }
        if cartridge_type.is_mbc2() {
            // The 512 four-bit cells repeat throughout the cartridge RAM
            // window; only the low nine address bits reach the internal RAM.
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
        if cartridge_type.has_rtc() && (0x08..=0x0C).contains(&self.mbc3_ram_or_rtc_select) {
            self.rtc.write(self.mbc3_ram_or_rtc_select, value);
            return;
        }
        if cartridge_type.is_mbc3() && self.mbc3_ram_or_rtc_select > 0x03 {
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

    /// Returns a versioned RTC sidecar for a battery-backed MBC3 timer cartridge.
    ///
    /// Save RAM stays raw for compatibility with existing `.sav` files. The RTC
    /// has independent state, so frontends should store this value in a separate
    /// sidecar and restore it with [`Self::load_save_rtc`].
    #[must_use]
    pub fn save_rtc(&self) -> Option<[u8; RTC_SAVE_SIZE]> {
        self.header
            .cartridge_type()
            .has_rtc()
            .then(|| self.rtc.save())
    }

    /// Restores a versioned RTC sidecar for a battery-backed MBC3 timer cartridge.
    ///
    /// # Errors
    ///
    /// Returns [`SaveRtcError`] when this cartridge has no RTC or `data` is not
    /// a compatible sidecar.
    pub fn load_save_rtc(&mut self, data: &[u8]) -> Result<(), SaveRtcError> {
        if !self.header.cartridge_type().has_rtc() {
            return Err(SaveRtcError::NoRtc);
        }

        self.rtc.load(data)
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
            0x6000..=0x7FFF if self.header.cartridge_type().has_rtc() => self.rtc.latch(value),
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
            registers: RtcRegisters::default(),
            subsecond_tcycles: 0,
            halted: false,
            day_carry: false,
            latched: None,
            latch_previous: 0,
        }
    }

    fn read(&self, register: u8) -> u8 {
        let registers = self.latched.unwrap_or_else(|| self.live_registers());
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
        match register {
            // Only an RTC-seconds write restarts the sub-second divider. The
            // other four register writes retain their distance to the next tick.
            0x08 => {
                self.registers.seconds = value & 0x3F;
                self.subsecond_tcycles = 0;
            }
            0x09 => self.registers.minutes = value & 0x3F,
            0x0A => self.registers.hours = value & 0x1F,
            0x0B => self.registers.day_low = value,
            0x0C => {
                self.registers.day_high = value & 0xC1;
                self.day_carry = value & 0x80 != 0;
                self.halted = value & 0x40 != 0;
            }
            _ => {}
        }
    }

    fn latch(&mut self, value: u8) {
        if self.latch_previous == 0 && value == 1 {
            self.latched = Some(self.live_registers());
        }
        self.latch_previous = value;
    }

    fn live_registers(&self) -> RtcRegisters {
        self.registers_from_state()
    }

    fn tick(&mut self, cycles: TCycles) {
        if self.halted {
            return;
        }

        let elapsed = u64::from(self.subsecond_tcycles) + u64::from(cycles.0);
        self.tick_seconds(elapsed / DMG_TCYCLES_PER_SECOND);
        self.subsecond_tcycles = u32::try_from(elapsed % DMG_TCYCLES_PER_SECOND)
            .expect("RTC sub-second T-cycle remainder fits u32");
    }

    fn apply_elapsed_wall_time(&mut self, elapsed_millis: u64) {
        if self.halted {
            return;
        }

        self.tick_seconds(elapsed_millis / 1_000);
        let fractional_tcycles = elapsed_millis % 1_000 * DMG_TCYCLES_PER_SECOND / 1_000;
        self.tick(TCycles(
            u32::try_from(fractional_tcycles).expect("fractional RTC T-cycles fit u32"),
        ));
    }

    fn tick_seconds(&mut self, ticks: u64) {
        for _ in 0..ticks {
            self.tick_second();
        }
    }

    fn tick_second(&mut self) {
        if self.registers.seconds == 59 {
            self.registers.seconds = 0;
            self.tick_minute();
        } else if self.registers.seconds == 63 {
            self.registers.seconds = 0;
        } else {
            self.registers.seconds += 1;
        }
    }

    fn tick_minute(&mut self) {
        if self.registers.minutes == 59 {
            self.registers.minutes = 0;
            self.tick_hour();
        } else if self.registers.minutes == 63 {
            self.registers.minutes = 0;
        } else {
            self.registers.minutes += 1;
        }
    }

    fn tick_hour(&mut self) {
        if self.registers.hours == 23 {
            self.registers.hours = 0;
            self.tick_day();
        } else if self.registers.hours == 31 {
            self.registers.hours = 0;
        } else {
            self.registers.hours += 1;
        }
    }

    fn tick_day(&mut self) {
        let day =
            u16::from(self.registers.day_low) | (u16::from(self.registers.day_high & 0x01) << 8);
        let next_day = day.wrapping_add(1);
        self.registers.day_low = u8::try_from(next_day & 0x00FF).expect("day low byte fits u8");
        self.registers.day_high =
            (self.registers.day_high & !0x01) | u8::from(next_day & 0x0100 != 0);
        if next_day > 511 {
            self.registers.day_low = 0;
            self.registers.day_high &= !0x01;
            self.day_carry = true;
        }
    }

    fn registers_from_state(&self) -> RtcRegisters {
        RtcRegisters {
            seconds: self.registers.seconds,
            minutes: self.registers.minutes,
            hours: self.registers.hours,
            day_low: self.registers.day_low,
            day_high: (self.registers.day_high & 0x01)
                | (u8::from(self.halted) << 6)
                | (u8::from(self.day_carry) << 7),
        }
    }

    fn save(&self) -> [u8; RTC_SAVE_SIZE] {
        let registers = self.registers_from_state();
        let mut data = [0; RTC_SAVE_SIZE];
        data[..4].copy_from_slice(&RTC_SAVE_MAGIC);
        data[4] = RTC_SAVE_VERSION;
        data[5] = registers.seconds;
        data[6] = registers.minutes;
        data[7] = registers.hours;
        data[8] = registers.day_low;
        data[9] = registers.day_high;
        data[10..14].copy_from_slice(&self.subsecond_tcycles.to_le_bytes());
        data[14..].copy_from_slice(&host_millis().to_le_bytes());
        data
    }

    fn load(&mut self, data: &[u8]) -> Result<(), SaveRtcError> {
        if data.len() != RTC_SAVE_SIZE {
            return Err(SaveRtcError::WrongLength {
                expected: RTC_SAVE_SIZE,
                actual: data.len(),
            });
        }
        if data[..4] != RTC_SAVE_MAGIC || data[4] != RTC_SAVE_VERSION {
            return Err(SaveRtcError::InvalidFormat);
        }

        let registers = RtcRegisters {
            seconds: data[5] & 0x3F,
            minutes: data[6] & 0x3F,
            hours: data[7] & 0x1F,
            day_low: data[8],
            day_high: data[9] & 0xC1,
        };
        self.registers = registers;
        self.subsecond_tcycles =
            u32::from_le_bytes(data[10..14].try_into().expect("RTC phase length is fixed"));
        let saved_at_millis = i64::from_le_bytes(
            data[14..]
                .try_into()
                .expect("RTC timestamp length is fixed"),
        );
        self.halted = registers.day_high & 0x40 != 0;
        self.day_carry = registers.day_high & 0x80 != 0;
        self.latched = None;
        self.latch_previous = 0;
        self.apply_elapsed_wall_time(
            u64::try_from(host_millis().saturating_sub(saved_at_millis)).unwrap_or(0),
        );
        Ok(())
    }
}

fn host_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRamError {
    NoExternalRam,
    WrongLength { expected: usize, actual: usize },
}

/// Errors returned while restoring a persisted MBC3 RTC sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRtcError {
    NoRtc,
    WrongLength { expected: usize, actual: usize },
    InvalidFormat,
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

impl fmt::Display for SaveRtcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRtc => formatter.write_str("cartridge has no MBC3 RTC"),
            Self::WrongLength { expected, actual } => write!(
                formatter,
                "RTC save length mismatch: expected {expected} bytes, got {actual}"
            ),
            Self::InvalidFormat => formatter.write_str("invalid RTC save format"),
        }
    }
}

impl Error for SaveRtcError {}

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
    use crate::cpu::TCycles;

    use super::{
        header::calculate_header_checksum, Cartridge, CartridgeError, CartridgeReadError,
        RtcRegisters, SaveRamError, SaveRtcError, RAM_BANK_SIZE, ROM_BANK_SIZE,
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
    fn mbc1_forbidden_switchable_banks_remap_to_the_next_bank() {
        let rom = fake_banked_rom(b"MBC1BANK", 0x01, 0x06, 0x00, 128);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC1 header should parse");

        for (upper_bits, expected_bank) in [(1, 0x21), (2, 0x41), (3, 0x61)] {
            cartridge.write_rom(0x4000, upper_bits);
            cartridge.write_rom(0x2000, 0x00);

            assert_eq!(
                cartridge.read_rom(0x4000),
                Ok(expected_bank),
                "MBC1 bank 0x{expected_bank:02X} must replace forbidden bank 0x{:02X}",
                upper_bits << 5
            );
        }
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
    fn mbc2_ram_mirrors_its_512_nibbles_through_the_ram_window() {
        let rom = fake_banked_rom(b"MBC2", 0x06, 0x02, 0x00, 16);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC2 header should parse");
        cartridge.write_rom(0x0000, 0x0A);

        cartridge.write_ram(0xA000, 0x05);
        cartridge.write_ram(0xA200, 0x0C);

        assert_eq!(
            cartridge.read_ram(0xA200),
            0xFC,
            "only the low nine address bits reach MBC2's 512 internal nibble cells"
        );
        assert_eq!(
            cartridge.read_ram(0xA000),
            0xFC,
            "mirrored writes update the same internal MBC2 nibble cell"
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
    fn mbc3_without_a_timer_does_not_expose_rtc_registers() {
        let rom = fake_banked_rom(b"MBC3RAM", 0x13, 0x03, 0x03, 8);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC3 header should parse");
        cartridge.write_rom(0x0000, 0x0A);
        cartridge.write_rom(0x4000, 0x08);
        cartridge.write_ram(0xA000, 12);

        assert_eq!(
            cartridge.read_ram(0xA000),
            0xFF,
            "only timer-equipped MBC3 cartridges decode RTC register selections"
        );
    }

    #[test]
    fn mbc3_latch_freezes_a_snapshot_without_stopping_the_live_clock() {
        let rom = fake_banked_rom(b"MBC3RTC", 0x10, 0x03, 0x03, 8);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC3 header should parse");
        cartridge.write_rom(0x0000, 0x0A);
        cartridge.write_rom(0x4000, 0x08);
        cartridge.rtc.registers.seconds = 10;

        cartridge.write_rom(0x6000, 0x00);
        cartridge.write_rom(0x6000, 0x01);
        cartridge.rtc.registers.seconds = 15;

        assert_eq!(
            cartridge.read_ram(0xA000),
            10,
            "latched seconds should remain stable"
        );

        cartridge.write_rom(0x6000, 0x00);
        cartridge.write_rom(0x6000, 0x01);
        assert_eq!(
            cartridge.read_ram(0xA000),
            15,
            "a new 0-to-1 latch captures live time"
        );
    }

    #[test]
    fn mbc3_rtc_halt_and_day_carry_are_separate_control_bits() {
        let rom = fake_banked_rom(b"MBC3RTC", 0x10, 0x03, 0x03, 8);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC3 header should parse");
        cartridge.write_rom(0x0000, 0x0A);
        cartridge.write_rom(0x4000, 0x0C);
        cartridge.write_ram(0xA000, 0x40);
        assert!(cartridge.rtc.halted, "day-high bit 6 halts the RTC");

        cartridge.write_ram(0xA000, 0x80);
        assert!(!cartridge.rtc.halted, "clearing bit 6 resumes the RTC");
        assert!(
            cartridge.rtc.day_carry,
            "day-high bit 7 controls the carry latch"
        );

        cartridge.rtc.registers.day_low = 0;
        cartridge.rtc.registers.day_high = 0;
        cartridge.rtc.day_carry = true;
        assert_eq!(
            cartridge.read_ram(0xA000) & 0x81,
            0x80,
            "the 9-bit day counter wraps and raises carry after day 511"
        );
    }

    #[test]
    fn mbc3_rtc_keeps_raw_register_ranges_and_invalid_rollovers() {
        let rom = fake_banked_rom(b"MBC3RTC", 0x10, 0x03, 0x03, 8);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC3 header should parse");
        cartridge.write_rom(0x0000, 0x0A);

        for (select, value, expected) in
            [(0x08, 0xFF, 0x3F), (0x09, 0xFF, 0x3F), (0x0A, 0xFF, 0x1F)]
        {
            cartridge.write_rom(0x4000, select);
            cartridge.write_ram(0xA000, value);
            assert_eq!(cartridge.read_ram(0xA000), expected);
        }

        cartridge.rtc.registers.seconds = 63;
        cartridge.rtc.registers.minutes = 17;
        cartridge.rtc.tick_second();
        assert_eq!(cartridge.rtc.registers.seconds, 0);
        assert_eq!(
            cartridge.rtc.registers.minutes, 17,
            "an invalid seconds rollover does not increment minutes"
        );
    }

    #[test]
    fn mbc3_only_seconds_writes_reset_the_subsecond_phase() {
        let mut rtc = super::Rtc::new();
        rtc.subsecond_tcycles = 900;
        rtc.write(0x09, 12);
        assert_eq!(rtc.subsecond_tcycles, 900);

        rtc.write(0x08, 12);
        assert_eq!(rtc.subsecond_tcycles, 0);
    }

    #[test]
    fn mbc3_rtc_advances_from_bus_tcycles_and_respects_halt() {
        let rom = fake_banked_rom(b"MBC3RTC", 0x10, 0x03, 0x03, 8);
        let mut cartridge = Cartridge::from_bytes(rom).expect("MBC3 header should parse");
        cartridge.tick(TCycles(4_194_304));
        assert_eq!(cartridge.rtc.registers.seconds, 1);

        cartridge.rtc.halted = true;
        cartridge.tick(TCycles(4_194_304));
        assert_eq!(
            cartridge.rtc.registers.seconds, 1,
            "a halted RTC must not advance when the bus continues ticking"
        );
    }

    #[test]
    fn mbc3_rtc_sidecar_is_available_without_external_ram() {
        let rom = fake_banked_rom(b"MBC3RTC", 0x0F, 0x03, 0x00, 8);
        let mut cartridge =
            Cartridge::from_bytes(rom.clone()).expect("MBC3 timer header should parse");
        cartridge.rtc.registers = RtcRegisters {
            seconds: 45,
            minutes: 25,
            hours: 3,
            day_low: 0,
            day_high: 0,
        };
        let save = cartridge
            .save_rtc()
            .expect("timer cartridge should export an RTC sidecar");

        assert!(
            cartridge.save_ram().is_none(),
            "timer-only cartridges have no raw RAM save"
        );

        let mut restored = Cartridge::from_bytes(rom).expect("MBC3 timer header should parse");
        restored
            .load_save_rtc(&save)
            .expect("valid RTC sidecar should restore");
        assert_eq!(restored.rtc.registers.seconds, 45);
        assert_eq!(restored.rtc.registers.minutes, 25);
        assert_eq!(restored.rtc.registers.hours, 3);
        assert_eq!(
            restored.load_save_rtc(&[0; 1]),
            Err(SaveRtcError::WrongLength {
                expected: 22,
                actual: 1
            })
        );
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
