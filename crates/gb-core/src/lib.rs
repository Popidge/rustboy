#![doc = "Core Game Boy emulation primitives."]

pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod cpu;
pub mod interrupt;
pub mod joypad;
pub mod ppu;
pub mod serial;
pub mod timer;

use apu::StereoSample;
use bus::Bus;
use cartridge::{Cartridge, CartridgeError, SaveRamError};
use cpu::{Cpu, CpuError, CpuRegisters, TCycles};
use joypad::Button;
use ppu::FRAMEBUFFER_PIXELS;

/// Top-level owner for the emulated DMG Game Boy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameBoy {
    cpu: Cpu,
    bus: Bus,
}

impl GameBoy {
    /// Creates a Game Boy from an already-loaded cartridge.
    #[must_use]
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            cpu: Cpu::new_dmg_post_boot(),
            bus: Bus::new(cartridge),
        }
    }

    /// Creates a Game Boy from raw ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns a cartridge loading error if the ROM bytes are invalid or not
    /// supported by the current cartridge implementation.
    pub fn from_rom(rom: Vec<u8>) -> Result<Self, CartridgeError> {
        Cartridge::from_bytes(rom).map(Self::new)
    }

    /// Executes one CPU instruction and advances bus-owned hardware.
    ///
    /// # Errors
    ///
    /// Returns a CPU error when execution reaches an unimplemented opcode.
    pub fn step(&mut self) -> Result<TCycles, CpuError> {
        self.bus.begin_cpu_step();
        let cycles = self.cpu.step(&mut self.bus)?;
        let clocked_cycles = self.bus.clocked_cpu_cycles();

        if clocked_cycles.0 < cycles.0 {
            self.bus.tick(TCycles(cycles.0 - clocked_cycles.0));
        }

        Ok(cycles)
    }

    /// Steps the machine until the PPU has completed a frame.
    ///
    /// # Errors
    ///
    /// Returns a CPU error when execution reaches an unimplemented opcode
    /// before a frame is ready.
    pub fn run_until_frame(&mut self) -> Result<(), CpuError> {
        while !self.bus.take_frame_ready() {
            self.step()?;
        }

        Ok(())
    }

    /// Returns the current PPU framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32; FRAMEBUFFER_PIXELS] {
        self.bus.framebuffer()
    }

    /// Returns a read-only snapshot of the CPU registers for debugging UIs.
    #[must_use]
    pub fn registers(&self) -> &CpuRegisters {
        self.cpu.registers()
    }

    /// Reads a byte through the bus for debugging UIs.
    ///
    /// This is intentionally a read-only inspection helper. CPU execution still
    /// owns all normal memory access.
    #[must_use]
    pub fn debug_read8(&self, address: u16) -> u8 {
        self.bus.read8(address)
    }

    /// Returns collected serial debug output without draining it.
    #[must_use]
    pub fn serial_output(&self) -> &[u8] {
        self.bus.serial_output()
    }

    /// Drains collected serial debug output.
    pub fn take_serial_output(&mut self) -> Vec<u8> {
        self.bus.take_serial_output()
    }

    /// Drains the test-only CPU bus-cycle trace.
    #[cfg(any(test, feature = "test-trace"))]
    pub fn take_cycle_trace(&mut self) -> Vec<bus::BusCycleRecord> {
        self.bus.take_cycle_trace()
    }

    /// Drains the test-only one-T-cycle Bus dispatcher trace.
    #[cfg(any(test, feature = "test-trace"))]
    pub fn take_tcycle_trace(&mut self) -> Vec<bus::BusTcycleRecord> {
        self.bus.take_tcycle_trace()
    }

    /// Returns generated audio samples without draining them.
    #[must_use]
    pub fn audio_samples(&self) -> &[StereoSample] {
        self.bus.audio_samples()
    }

    /// Drains generated audio samples for frontend playback.
    pub fn take_audio_samples(&mut self) -> Vec<StereoSample> {
        self.bus.take_audio_samples()
    }

    /// Updates one joypad button state.
    pub fn set_button(&mut self, button: Button, pressed: bool) {
        self.bus.set_button(button, pressed);
    }

    #[must_use]
    pub fn save_ram(&self) -> Option<&[u8]> {
        self.bus.save_ram()
    }

    /// Restores external cartridge RAM from save data.
    ///
    /// # Errors
    ///
    /// Returns a save RAM error when the data cannot be applied to the loaded
    /// cartridge.
    pub fn load_save_ram(&mut self, data: &[u8]) -> Result<(), SaveRamError> {
        self.bus.load_save_ram(data)
    }
}

#[cfg(test)]
mod tests {
    use super::GameBoy;
    use crate::{cartridge::Cartridge, ppu::FRAMEBUFFER_PIXELS};

    fn minimal_rom() -> Vec<u8> {
        let mut rom = vec![0; 0x8000];
        rom[0x0147] = 0x00;
        rom[0x0148] = 0x00;
        rom[0x0149] = 0x00;
        rom[0x014D] = header_checksum(&rom);
        rom
    }

    fn header_checksum(rom: &[u8]) -> u8 {
        let mut checksum = 0_u8;

        for byte in &rom[0x0134..=0x014C] {
            checksum = checksum.wrapping_sub(*byte).wrapping_sub(1);
        }

        checksum
    }

    #[test]
    fn game_boy_owns_cpu_bus_and_exposes_framebuffer() {
        let cartridge = Cartridge::from_bytes(minimal_rom()).expect("test ROM should load");
        let game_boy = GameBoy::new(cartridge);

        assert_eq!(
            game_boy.framebuffer().len(),
            FRAMEBUFFER_PIXELS,
            "GameBoy should expose the PPU framebuffer through the core facade"
        );
    }

    #[test]
    fn game_boy_steps_cpu_and_bus_together() {
        let mut rom = minimal_rom();
        rom[0x0100] = 0x00;
        let mut game_boy = GameBoy::from_rom(rom).expect("test ROM should load");

        let cycles = game_boy.step();

        assert_eq!(
            cycles.map(|cycles| cycles.0),
            Ok(4),
            "NOP through GameBoy should return its T-cycle count"
        );
    }

    #[test]
    fn game_boy_does_not_double_tick_clocked_nop_fetches() {
        let mut rom = minimal_rom();
        rom[0x0100] = 0x00;
        let mut game_boy = GameBoy::from_rom(rom).expect("test ROM should load");

        for _ in 0..64 {
            game_boy.step().expect("NOP should execute");
        }

        assert_eq!(
            game_boy.debug_read8(0xFF04),
            0x01,
            "64 NOP opcode fetches should advance DIV by 256 T-cycles once"
        );
    }
}
