//! Address bus and basic memory map routing for the DMG Game Boy.
//!
//! The CPU talks to memory-mapped hardware through this type. At this stage the
//! bus owns the cartridge plus WRAM/HRAM and interrupt registers; later
//! milestones will add PPU, timer, serial, joypad, and APU routing.

use crate::cartridge::Cartridge;

const WRAM_START: u16 = 0xC000;
const WRAM_END: u16 = 0xDFFF;
const WRAM_SIZE: usize = 0x2000;

const HRAM_START: u16 = 0xFF80;
const HRAM_END: u16 = 0xFFFE;
const HRAM_SIZE: usize = 0x007F;

const INTERRUPT_ENABLE_ADDR: u16 = 0xFFFF;

/// CPU-facing memory bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bus {
    cartridge: Cartridge,
    wram: [u8; WRAM_SIZE],
    hram: [u8; HRAM_SIZE],
    interrupt_enable: u8,
    interrupt_flags: u8,
}

impl Bus {
    /// Creates a bus that owns the loaded cartridge and empty internal memory.
    #[must_use]
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            cartridge,
            wram: [0; WRAM_SIZE],
            hram: [0; HRAM_SIZE],
            interrupt_enable: 0,
            interrupt_flags: 0,
        }
    }

    /// Reads one byte from the CPU address space.
    ///
    /// Unsupported regions return `0xFF` until their hardware components exist.
    #[must_use]
    pub fn read8(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x7FFF => self.cartridge.read_rom(address).unwrap_or(0xFF),
            WRAM_START..=WRAM_END => self.wram[wram_index(address)],
            HRAM_START..=HRAM_END => self.hram[hram_index(address)],
            INTERRUPT_ENABLE_ADDR => self.interrupt_enable,
            _ => 0xFF,
        }
    }

    /// Writes one byte into the CPU address space.
    ///
    /// Writes to ROM and unsupported regions are ignored for now.
    pub fn write8(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x7FFF => {}
            WRAM_START..=WRAM_END => self.wram[wram_index(address)] = value,
            HRAM_START..=HRAM_END => self.hram[hram_index(address)] = value,
            INTERRUPT_ENABLE_ADDR => self.interrupt_enable = value,
            _ => {}
        }
    }

    /// Reads a little-endian 16-bit value from the CPU address space.
    #[must_use]
    pub fn read16(&self, address: u16) -> u16 {
        let low = self.read8(address);
        let high = self.read8(address.wrapping_add(1));

        u16::from_le_bytes([low, high])
    }

    /// Writes a little-endian 16-bit value into the CPU address space.
    pub fn write16(&mut self, address: u16, value: u16) {
        let [low, high] = value.to_le_bytes();

        self.write8(address, low);
        self.write8(address.wrapping_add(1), high);
    }

    /// Returns the raw interrupt flags register storage.
    ///
    /// Stage 10 will route this through `0xFF0F` with typed interrupt helpers.
    #[must_use]
    pub fn interrupt_flags(&self) -> u8 {
        self.interrupt_flags
    }
}

fn wram_index(address: u16) -> usize {
    usize::from(address - WRAM_START)
}

fn hram_index(address: u16) -> usize {
    usize::from(address - HRAM_START)
}
