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
const ECHO_RAM_START: u16 = 0xE000;
const ECHO_RAM_END: u16 = 0xFDFF;
const ECHO_RAM_OFFSET: u16 = 0x2000;

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
const OAM_SIZE: u8 = 0xA0;
const UNUSABLE_OAM_START: u16 = 0xFEA0;
const UNUSABLE_OAM_END: u16 = 0xFEFF;
const DMA_ADDR: u16 = 0xFF46;
const PPU_REGISTER_START: u16 = 0xFF40;
const PPU_REGISTER_END: u16 = 0xFF4B;
const CPU_MACHINE_CYCLE: TCycles = TCycles(4);

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
    oam_dma: OamDma,
    clocked_cpu_cycles: TCycles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OamDma {
    source_high: u8,
    next_byte: u8,
    startup_mcycles: u8,
    active: bool,
    elapsed_tcycles: u32,
}

impl OamDma {
    const fn inactive() -> Self {
        Self {
            source_high: 0,
            next_byte: 0,
            startup_mcycles: 0,
            active: false,
            elapsed_tcycles: 0,
        }
    }

    fn start(&mut self, source_high: u8) {
        self.source_high = source_high;
        self.next_byte = 0;
        self.startup_mcycles = 2;
        self.active = true;
        self.elapsed_tcycles = 0;
    }

    fn is_active(self) -> bool {
        self.active
    }

    fn source_address(self) -> u16 {
        (u16::from(self.source_high) << 8) | u16::from(self.next_byte)
    }

    fn complete_byte(&mut self) {
        self.next_byte = self.next_byte.wrapping_add(1);

        if self.next_byte == OAM_SIZE {
            self.active = false;
            self.elapsed_tcycles = 0;
        }
    }
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
            oam_dma: OamDma::inactive(),
            clocked_cpu_cycles: TCycles(0),
        }
    }

    /// Reads one byte from the CPU address space.
    ///
    /// Unsupported regions return `0xFF` until their hardware components exist.
    #[must_use]
    pub fn read8(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x7FFF => self.cartridge.read_rom(address).unwrap_or(0xFF),
            VRAM_START..=VRAM_END => self.ppu.read_vram(address - VRAM_START),
            CARTRIDGE_RAM_START..=CARTRIDGE_RAM_END => self.cartridge.read_ram(address),
            WRAM_START..=WRAM_END => self.wram[wram_index(address)],
            ECHO_RAM_START..=ECHO_RAM_END => self.wram[wram_index(address - ECHO_RAM_OFFSET)],
            OAM_START..=OAM_END => self.ppu.read_oam(address - OAM_START),
            UNUSABLE_OAM_START..=UNUSABLE_OAM_END => 0xFF,
            JOYPAD_ADDR => self.joypad.read(),
            SERIAL_START..=SERIAL_END => self.serial.read(address),
            TIMER_START..=TIMER_END => self.timer.read(address),
            INTERRUPT_FLAGS_ADDR => self.interrupt_flags.read_if(),
            0xFF10..=0xFF3F => self.apu.read(address),
            DMA_ADDR => self.oam_dma.source_high,
            PPU_REGISTER_START..=PPU_REGISTER_END => self.ppu.read_register(address),
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
            0x0000..=0x7FFF => self.cartridge.write_rom(address, value),
            VRAM_START..=VRAM_END => self.ppu.write_vram(address - VRAM_START, value),
            CARTRIDGE_RAM_START..=CARTRIDGE_RAM_END => self.cartridge.write_ram(address, value),
            WRAM_START..=WRAM_END => self.wram[wram_index(address)] = value,
            ECHO_RAM_START..=ECHO_RAM_END => {
                self.wram[wram_index(address - ECHO_RAM_OFFSET)] = value;
            }
            OAM_START..=OAM_END => self.ppu.write_oam(address - OAM_START, value),
            UNUSABLE_OAM_START..=UNUSABLE_OAM_END => {}
            JOYPAD_ADDR => self.joypad.write(value),
            SERIAL_START..=SERIAL_END => self.serial.write(address, value),
            TIMER_START..=TIMER_END => self.timer.write(address, value),
            INTERRUPT_FLAGS_ADDR => self.interrupt_flags.write_if(value),
            0xFF10..=0xFF3F => self.apu.write(address, value),
            DMA_ADDR => self.oam_dma.start(value),
            PPU_REGISTER_START..=PPU_REGISTER_END => self.ppu.write_register(address, value),
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
        self.ppu.tick(cycles, &mut self.interrupt_flags);
        self.apu.tick(cycles);
        self.tick_oam_dma(cycles);
    }

    /// Starts accounting for CPU bus cycles during one logical CPU step.
    pub(crate) fn begin_cpu_step(&mut self) {
        self.clocked_cpu_cycles = TCycles(0);
    }

    /// Returns how many T-cycles were already advanced by clocked CPU bus access.
    #[must_use]
    pub(crate) fn clocked_cpu_cycles(&self) -> TCycles {
        self.clocked_cpu_cycles
    }

    /// Fetches an opcode byte for the CPU and advances one machine cycle.
    pub fn cpu_fetch8(&mut self, address: u16) -> u8 {
        let value = self.cpu_read8_during_dma(address);
        self.cpu_idle_mcycle();
        value
    }

    /// Reads one byte for the CPU and advances one machine cycle.
    pub fn cpu_read8(&mut self, address: u16) -> u8 {
        let value = self.cpu_read8_during_dma(address);
        self.cpu_idle_mcycle();
        value
    }

    /// Writes one byte for the CPU and advances one machine cycle.
    pub fn cpu_write8(&mut self, address: u16, value: u8) {
        self.cpu_write8_during_dma(address, value);
        self.cpu_idle_mcycle();
    }

    /// Advances one internal CPU machine cycle without a memory transfer.
    pub fn cpu_idle_mcycle(&mut self) {
        self.tick(CPU_MACHINE_CYCLE);
        self.clocked_cpu_cycles.0 += CPU_MACHINE_CYCLE.0;
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

    fn cpu_read8_during_dma(&self, address: u16) -> u8 {
        if self.oam_dma.is_active() && cpu_oam_dma_blocks_address(address) {
            return 0xFF;
        }

        self.read8(address)
    }

    fn cpu_write8_during_dma(&mut self, address: u16, value: u8) {
        if self.oam_dma.is_active() && cpu_oam_dma_blocks_address(address) {
            return;
        }

        self.write8(address, value);
    }

    fn tick_oam_dma(&mut self, cycles: TCycles) {
        if !self.oam_dma.is_active() {
            return;
        }

        self.oam_dma.elapsed_tcycles += cycles.0;

        while self.oam_dma.is_active() && self.oam_dma.elapsed_tcycles >= CPU_MACHINE_CYCLE.0 {
            self.oam_dma.elapsed_tcycles -= CPU_MACHINE_CYCLE.0;

            if self.oam_dma.startup_mcycles > 0 {
                self.oam_dma.startup_mcycles -= 1;
                continue;
            }

            let source_address = self.oam_dma.source_address();
            let destination_offset = u16::from(self.oam_dma.next_byte);
            let byte = self.read_oam_dma_source(source_address);

            self.ppu.write_oam(destination_offset, byte);
            self.oam_dma.complete_byte();
        }
    }

    fn read_oam_dma_source(&self, address: u16) -> u8 {
        let address = if address >= ECHO_RAM_START {
            address - ECHO_RAM_OFFSET
        } else {
            address
        };

        self.read8(address)
    }
}

fn cpu_oam_dma_blocks_address(address: u16) -> bool {
    matches!(address, OAM_START..=UNUSABLE_OAM_END)
}

fn wram_index(address: u16) -> usize {
    usize::from(address - WRAM_START)
}

fn hram_index(address: u16) -> usize {
    usize::from(address - HRAM_START)
}
