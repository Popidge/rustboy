//! Address bus and basic memory map routing for the DMG Game Boy.
//!
//! The CPU talks to memory-mapped hardware through this type. At this stage the
//! bus owns the cartridge plus WRAM/HRAM and interrupt registers; later
//! milestones will add PPU, timer, serial, joypad, and APU routing.

use crate::{
    apu::{Apu, StereoSample},
    cartridge::Cartridge,
    cpu::TCycles,
    interrupt::{Interrupt, InterruptFlags},
    joypad::{Button, Joypad},
    ppu::{Ppu, FRAMEBUFFER_PIXELS},
    serial::Serial,
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
const JOYPAD_ADDR: u16 = 0xFF00;
const SERIAL_START: u16 = 0xFF01;
const SERIAL_END: u16 = 0xFF02;
const TIMER_START: u16 = 0xFF04;
const TIMER_END: u16 = 0xFF07;
const VRAM_START: u16 = 0x8000;
const VRAM_END: u16 = 0x9FFF;
const CARTRIDGE_RAM_START: u16 = 0xA000;
const CARTRIDGE_RAM_END: u16 = 0xBFFF;
const OAM_START: u16 = 0xFE00;
const OAM_END: u16 = 0xFE9F;
const OAM_SIZE: u16 = 0x00A0;
const UNUSABLE_OAM_START: u16 = 0xFEA0;
const UNUSABLE_OAM_END: u16 = 0xFEFF;
const DMA_ADDR: u16 = 0xFF46;
const PPU_REGISTER_START: u16 = 0xFF40;
const PPU_REGISTER_END: u16 = 0xFF4B;

/// CPU-facing memory bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bus {
    cartridge: Cartridge,
    ppu: Ppu,
    joypad: Joypad,
    serial: Serial,
    timer: Timer,
    apu: Apu,
    wram: [u8; WRAM_SIZE],
    hram: [u8; HRAM_SIZE],
    interrupt_enable: InterruptFlags,
    interrupt_flags: InterruptFlags,
    /// T-cycles already advanced by per-access reads/writes during the
    /// current instruction.  Subtracted from [`tick`](Self::tick) to avoid
    /// double-counting when the CPU contributes its own cycle total.
    pending_tick: u32,
    /// `true` while an OAM DMA transfer is in progress.  During DMA the
    /// CPU cannot access OAM ($FE00–$FE9F) — reads return `0xFF` and
    /// writes are silently dropped.
    dma_active: bool,
    /// Source address for an in-progress OAM DMA transfer.
    dma_source: u16,
    /// Current byte offset into the 160-byte OAM DMA transfer.
    dma_offset: u16,
}

/// Extracts the inner result of a `read8` match — identical dispatch used
/// by both [`read8`](Bus::read8) (ticking) and
/// [`read8_no_advance`](Bus::read8_no_advance) (debug inspection).
macro_rules! read8_body {
    ($self:expr, $address:expr) => {
        match $address {
            0x0000..=0x7FFF => $self.cartridge.read_rom($address).unwrap_or(0xFF),
            VRAM_START..=VRAM_END => $self.ppu.read_vram($address - VRAM_START),
            CARTRIDGE_RAM_START..=CARTRIDGE_RAM_END => $self.cartridge.read_ram($address),
            WRAM_START..=WRAM_END => $self.wram[wram_index($address)],
            OAM_START..=OAM_END => {
                if $self.dma_active || !$self.ppu.cpu_can_access_oam() {
                    0xFF
                } else {
                    $self.ppu.read_oam($address - OAM_START)
                }
            }
            UNUSABLE_OAM_START..=UNUSABLE_OAM_END => 0xFF,
            JOYPAD_ADDR => $self.joypad.read(),
            SERIAL_START..=SERIAL_END => $self.serial.read($address),
            TIMER_START..=TIMER_END => $self.timer.read($address),
            INTERRUPT_FLAGS_ADDR => $self.interrupt_flags.read_if(),
            0xFF10..=0xFF3F => $self.apu.read($address),
            PPU_REGISTER_START..=PPU_REGISTER_END => $self.ppu.read_register($address),
            HRAM_START..=HRAM_END => $self.hram[hram_index($address)],
            INTERRUPT_ENABLE_ADDR => $self.interrupt_enable.raw(),
            _ => 0xFF,
        }
    };
}

impl Bus {
    /// Creates a bus that owns the loaded cartridge and empty internal memory.
    #[must_use]
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            cartridge,
            ppu: Ppu::new(),
            joypad: Joypad::new(),
            serial: Serial::new(),
            timer: Timer::new(),
            apu: Apu::new(),
            wram: [0; WRAM_SIZE],
            hram: [0; HRAM_SIZE],
            interrupt_enable: InterruptFlags::default(),
            interrupt_flags: InterruptFlags::default(),
            pending_tick: 0,
            dma_active: false,
            dma_source: 0,
            dma_offset: 0,
        }
    }

    /// Reads one byte from the CPU address space.
    ///
    /// Each read advances bus-owned hardware by one M-cycle (4 T-cycles) so
    /// that the timer and other components stay in sync during multi-access
    /// instructions.  The cycle accounting is reconciled in [`tick`].
    ///
    /// Unsupported regions return `0xFF` until their hardware components exist.
    #[must_use]
    pub fn read8(&mut self, address: u16) -> u8 {
        self.pending_tick += 4;
        self.timer.tick(TCycles(4), &mut self.interrupt_flags);
        self.ppu.tick(TCycles(4), &mut self.interrupt_flags);
        self.apu.tick(TCycles(4));

        read8_body!(self, address)
    }

    /// Reads one byte without advancing any bus-owned hardware.
    ///
    /// Intended for debug inspection and test tooling that must remain
    /// read-only.
    #[must_use]
    pub fn read8_no_advance(&self, address: u16) -> u8 {
        read8_body!(self, address)
    }

    /// Writes one byte into the CPU address space.
    ///
    /// Each write advances bus-owned hardware by one M-cycle (4 T-cycles)
    /// **before** the register write takes effect, matching real hardware
    /// where the bus-cycle elapses ahead of the value being latched.
    ///
    /// Writes to ROM and unsupported regions are ignored for now.
    pub fn write8(&mut self, address: u16, value: u8) {
        self.pending_tick += 4;
        self.timer.tick(TCycles(4), &mut self.interrupt_flags);
        self.ppu.tick(TCycles(4), &mut self.interrupt_flags);
        self.apu.tick(TCycles(4));

        match address {
            0x0000..=0x7FFF => self.cartridge.write_rom(address, value),
            VRAM_START..=VRAM_END => self.ppu.write_vram(address - VRAM_START, value),
            CARTRIDGE_RAM_START..=CARTRIDGE_RAM_END => self.cartridge.write_ram(address, value),
            WRAM_START..=WRAM_END => self.wram[wram_index(address)] = value,
            OAM_START..=OAM_END => {
                if !self.dma_active && self.ppu.cpu_can_access_oam() {
                    self.ppu.write_oam(address - OAM_START, value);
                }
            }
            UNUSABLE_OAM_START..=UNUSABLE_OAM_END => {}
            JOYPAD_ADDR => self.joypad.write(value),
            SERIAL_START..=SERIAL_END => self.serial.write(address, value, &mut self.interrupt_flags),
            TIMER_START..=TIMER_END => self.timer.write(address, value),
            INTERRUPT_FLAGS_ADDR => self.interrupt_flags.write_if(value),
            0xFF10..=0xFF3F => self.apu.write(address, value),
            DMA_ADDR => self.start_oam_dma(value),
            PPU_REGISTER_START..=PPU_REGISTER_END => self.ppu.write_register(address, value),
            HRAM_START..=HRAM_END => self.hram[hram_index(address)] = value,
            INTERRUPT_ENABLE_ADDR => self.interrupt_enable.set_raw(value),
            _ => {}
        }
    }

    /// Reads a little-endian 16-bit value from the CPU address space.
    #[must_use]
    pub fn read16(&mut self, address: u16) -> u16 {
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
    ///
    /// Subtracts cycles that were already consumed by individual `read8` /
    /// `write8` calls during the current instruction so that the total
    /// hardware time matches the CPU-reported instruction length.
    pub fn tick(&mut self, cycles: TCycles) {
        let per_access = std::mem::take(&mut self.pending_tick);
        let remaining = cycles.0.saturating_sub(per_access);

        if remaining > 0 {
            self.timer.tick(TCycles(remaining), &mut self.interrupt_flags);
            self.ppu.tick(TCycles(remaining), &mut self.interrupt_flags);
            self.apu.tick(TCycles(remaining));
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32; FRAMEBUFFER_PIXELS] {
        self.ppu.framebuffer()
    }

    #[must_use]
    pub fn frame_ready(&self) -> bool {
        self.ppu.frame_ready()
    }

    pub fn take_frame_ready(&mut self) -> bool {
        self.ppu.take_frame_ready()
    }

    /// Returns collected serial debug output without draining it.
    #[must_use]
    pub fn serial_output(&self) -> &[u8] {
        self.serial.output()
    }

    /// Drains collected serial debug output.
    pub fn take_serial_output(&mut self) -> Vec<u8> {
        self.serial.take_output()
    }

    #[must_use]
    pub fn audio_samples(&self) -> &[StereoSample] {
        self.apu.samples()
    }

    pub fn take_audio_samples(&mut self) -> Vec<StereoSample> {
        self.apu.take_samples()
    }

    /// Updates one joypad button state and requests an interrupt on new presses.
    pub fn set_button(&mut self, button: Button, pressed: bool) {
        self.joypad
            .set_button(button, pressed, &mut self.interrupt_flags);
    }

    #[must_use]
    pub fn save_ram(&self) -> Option<&[u8]> {
        self.cartridge.save_ram()
    }

    /// Restores external cartridge RAM from save data.
    ///
    /// # Errors
    ///
    /// Returns a save RAM error when the data cannot be applied to the loaded
    /// cartridge.
    pub fn load_save_ram(&mut self, data: &[u8]) -> Result<(), crate::cartridge::SaveRamError> {
        self.cartridge.load_save_ram(data)
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

    fn start_oam_dma(&mut self, value: u8) {
        self.dma_active = true;
        self.dma_source = u16::from(value) << 8;
        self.dma_offset = 0;
    }

    /// Transfers the next byte of an in-progress OAM DMA, advancing
    /// hardware by 1 M-cycle (4 T-cycles).  Returns the number of T-cycles
    /// consumed (always 4, or 0 when DMA is complete).
    ///
    /// Callers should call [`tick`](Self::tick) with the returned value
    /// after each byte transfer.
    pub fn dma_step(&mut self) -> u32 {
        if !self.dma_active || self.dma_offset >= OAM_SIZE {
            self.dma_active = false;
            return 0;
        }
        let source_addr = self.dma_source.wrapping_add(self.dma_offset);
        // Use no-advance so the hardware tick is controlled by the caller.
        let byte = self.read8_no_advance(source_addr);
        self.ppu.write_oam(self.dma_offset, byte);
        self.dma_offset += 1;
        if self.dma_offset >= OAM_SIZE {
            self.dma_active = false;
        }
        4
    }

    /// Returns `true` when an OAM DMA transfer is in progress.
    #[must_use]
    pub fn dma_active(&self) -> bool {
        self.dma_active
    }
}

fn wram_index(address: u16) -> usize {
    usize::from(address - WRAM_START)
}

fn hram_index(address: u16) -> usize {
    usize::from(address - HRAM_START)
}
