//! Address bus and basic memory map routing for the DMG Game Boy.
//!
//! The CPU talks to memory-mapped hardware through this type. At this stage the
//! bus owns the cartridge plus WRAM/HRAM and interrupt registers; later
//! milestones will add PPU, timer, serial, joypad, and APU routing.

use crate::{
    cartridge::Cartridge,
    cpu::TCycles,
    interrupt::{Interrupt, InterruptFlags},
    timer::Timer,
};

const WRAM_START: u16 = 0xC000;
const WRAM_END: u16 = 0xDFFF;
const WRAM_SIZE: usize = 0x2000;

const HRAM_START: u16 = 0xFF80;
const HRAM_END: u16 = 0xFFFE;
const HRAM_SIZE: usize = 0x007F;

const INTERRUPT_ENABLE_ADDR: u16 = 0xFFFF;
const INTERRUPT_FLAGS_ADDR: u16 = 0xFF0F;
const TIMER_START: u16 = 0xFF04;
const TIMER_END: u16 = 0xFF07;

/// CPU-facing memory bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bus {
    cartridge: Cartridge,
    timer: Timer,
    wram: [u8; WRAM_SIZE],
    hram: [u8; HRAM_SIZE],
    interrupt_enable: InterruptFlags,
    interrupt_flags: InterruptFlags,
}

impl Bus {
    /// Creates a bus that owns the loaded cartridge and empty internal memory.
    #[must_use]
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            cartridge,
            timer: Timer::new(),
            wram: [0; WRAM_SIZE],
            hram: [0; HRAM_SIZE],
            interrupt_enable: InterruptFlags::default(),
            interrupt_flags: InterruptFlags::default(),
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
            TIMER_START..=TIMER_END => self.timer.read(address),
            INTERRUPT_FLAGS_ADDR => self.interrupt_flags.read_if(),
            HRAM_START..=HRAM_END => self.hram[hram_index(address)],
            INTERRUPT_ENABLE_ADDR => self.interrupt_enable.raw(),
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
            TIMER_START..=TIMER_END => self.timer.write(address, value),
            INTERRUPT_FLAGS_ADDR => self.interrupt_flags.write_if(value),
            HRAM_START..=HRAM_END => self.hram[hram_index(address)] = value,
            INTERRUPT_ENABLE_ADDR => self.interrupt_enable.set_raw(value),
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

    /// Advances bus-owned hardware components by the given T-cycles.
    pub fn tick(&mut self, cycles: TCycles) {
        self.timer.tick(cycles, &mut self.interrupt_flags);
    }

    /// Returns the raw interrupt flags register storage.
    #[must_use]
    pub fn interrupt_flags(&self) -> u8 {
        self.interrupt_flags.raw()
    }

    /// Returns the raw interrupt enable register storage.
    #[must_use]
    pub fn interrupt_enable(&self) -> u8 {
        self.interrupt_enable.raw()
    }

    /// Requests an interrupt in the IF register.
    pub fn request_interrupt(&mut self, interrupt: Interrupt) {
        self.interrupt_flags.request(interrupt);
    }

    /// Clears an interrupt request from the IF register.
    pub fn clear_interrupt(&mut self, interrupt: Interrupt) {
        self.interrupt_flags.clear(interrupt);
    }

    /// Returns the highest-priority interrupt that is both enabled and requested.
    #[must_use]
    pub fn pending_interrupt(&self) -> Option<Interrupt> {
        InterruptFlags::first_pending(self.interrupt_enable, self.interrupt_flags)
    }
}

fn wram_index(address: u16) -> usize {
    usize::from(address - WRAM_START)
}

fn hram_index(address: u16) -> usize {
    usize::from(address - HRAM_START)
}
