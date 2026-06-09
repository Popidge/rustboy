//! CPU state for the DMG Game Boy.
//!
//! The CPU owns register state and executes instructions by temporarily
//! borrowing the bus. Memory access stays routed through `Bus`.

mod flags;
mod registers;

use crate::bus::Bus;
use std::fmt;

pub use flags::CpuFlags;
pub use registers::CpuRegisters;

/// CPU cycle count measured in Game Boy T-cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TCycles(pub u32);

/// CPU execution error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuError {
    /// The opcode fetch succeeded, but this emulator does not implement it yet.
    UnimplementedOpcode { pc: u16, opcode: u8 },
}

impl fmt::Display for CpuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnimplementedOpcode { pc, opcode } => {
                write!(
                    formatter,
                    "unimplemented opcode {opcode:02X} at PC={pc:04X}"
                )
            }
        }
    }
}

impl std::error::Error for CpuError {}

/// DMG CPU state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    registers: CpuRegisters,
}

impl Cpu {
    /// Creates a CPU initialized as though the DMG boot ROM has already run.
    ///
    /// The emulator does not execute the boot ROM yet, so this constructor
    /// seeds the CPU with the commonly documented post-boot register values.
    #[must_use]
    pub fn new_dmg_post_boot() -> Self {
        Self {
            registers: CpuRegisters::new_dmg_post_boot(),
        }
    }

    /// Returns the CPU register state.
    #[must_use]
    pub fn registers(&self) -> &CpuRegisters {
        &self.registers
    }

    /// Fetches one byte from the address at `PC`, then increments `PC`.
    #[must_use]
    pub fn fetch8(&mut self, bus: &Bus) -> u8 {
        let address = self.registers.pc;
        let value = bus.read8(address);
        self.registers.pc = self.registers.pc.wrapping_add(1);
        value
    }

    /// Fetches a little-endian 16-bit immediate operand from `PC`.
    #[must_use]
    pub fn fetch16(&mut self, bus: &Bus) -> u16 {
        let low = self.fetch8(bus);
        let high = self.fetch8(bus);

        u16::from_le_bytes([low, high])
    }

    /// Executes one CPU instruction.
    ///
    /// Only `NOP` is implemented in this milestone.
    ///
    /// # Errors
    ///
    /// Returns [`CpuError::UnimplementedOpcode`] when the fetched opcode is not
    /// implemented yet.
    pub fn step(&mut self, bus: &mut Bus) -> Result<TCycles, CpuError> {
        let pc = self.registers.pc;
        let opcode = self.fetch8(bus);

        match opcode {
            0x00 => Ok(TCycles(4)),
            _ => Err(CpuError::UnimplementedOpcode { pc, opcode }),
        }
    }

    /// Formats CPU state with an opcode for stable trace logging.
    ///
    /// The core returns a string instead of printing; callers decide where
    /// traces should go.
    #[must_use]
    pub fn trace_with_opcode(&self, pc: u16, opcode: u8) -> String {
        format!(
            "PC={pc:04X} OP={opcode:02X} AF={:04X} BC={:04X} DE={:04X} HL={:04X} SP={:04X}",
            self.registers.af(),
            self.registers.bc(),
            self.registers.de(),
            self.registers.hl(),
            self.registers.sp
        )
    }

    /// Fetches the opcode at the current `PC` without changing CPU state and
    /// formats a trace line for the next instruction.
    #[must_use]
    pub fn trace_next_instruction(&self, bus: &Bus) -> String {
        let pc = self.registers.pc;
        let opcode = bus.read8(pc);

        self.trace_with_opcode(pc, opcode)
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new_dmg_post_boot()
    }
}

#[cfg(test)]
mod tests {
    use super::{Cpu, CpuError, TCycles};
    use crate::{bus::Bus, cartridge::Cartridge};

    const ROM_SIZE: usize = 32 * 1024;
    const TITLE_START: usize = 0x0134;
    const CARTRIDGE_TYPE_ADDR: usize = 0x0147;
    const ROM_SIZE_ADDR: usize = 0x0148;
    const RAM_SIZE_ADDR: usize = 0x0149;
    const HEADER_CHECKSUM_START: usize = 0x0134;
    const HEADER_CHECKSUM_END_INCLUSIVE: usize = 0x014C;
    const HEADER_CHECKSUM_ADDR: usize = 0x014D;

    fn bus_with_bytes(bytes: &[(usize, u8)]) -> Bus {
        let mut rom = vec![0; ROM_SIZE];
        rom[TITLE_START..TITLE_START + 7].copy_from_slice(b"CPUTEST");
        rom[CARTRIDGE_TYPE_ADDR] = 0x00;
        rom[ROM_SIZE_ADDR] = 0x00;
        rom[RAM_SIZE_ADDR] = 0x00;

        for (address, value) in bytes {
            rom[*address] = *value;
        }

        rom[HEADER_CHECKSUM_ADDR] = calculate_header_checksum(&rom);

        let cartridge = Cartridge::from_bytes(rom).expect("test ROM should parse");
        Bus::new(cartridge)
    }

    fn calculate_header_checksum(rom: &[u8]) -> u8 {
        rom[HEADER_CHECKSUM_START..=HEADER_CHECKSUM_END_INCLUSIVE]
            .iter()
            .fold(0_u8, |checksum, byte| {
                checksum.wrapping_sub(*byte).wrapping_sub(1)
            })
    }

    #[test]
    fn new_dmg_post_boot_sets_standard_cpu_registers() {
        let cpu = Cpu::new_dmg_post_boot();
        let registers = cpu.registers();

        assert_eq!(registers.af(), 0x01B0, "post-boot AF should be 0x01B0");
        assert_eq!(registers.bc(), 0x0013, "post-boot BC should be 0x0013");
        assert_eq!(registers.de(), 0x00D8, "post-boot DE should be 0x00D8");
        assert_eq!(registers.hl(), 0x014D, "post-boot HL should be 0x014D");
        assert_eq!(registers.sp, 0xFFFE, "post-boot SP should be 0xFFFE");
        assert_eq!(registers.pc, 0x0100, "post-boot PC should be 0x0100");
    }

    #[test]
    fn fetch8_reads_at_pc_and_increments_pc() {
        let bus = bus_with_bytes(&[(0x0100, 0x42)]);
        let mut cpu = Cpu::new_dmg_post_boot();

        let value = cpu.fetch8(&bus);

        assert_eq!(value, 0x42, "fetch8 should read the byte at PC");
        assert_eq!(
            cpu.registers().pc,
            0x0101,
            "fetch8 should advance PC by one"
        );
    }

    #[test]
    fn fetch8_wraps_pc_after_ffff() {
        let bus = bus_with_bytes(&[(0x0000, 0x99)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.pc = 0xFFFF;

        let value = cpu.fetch8(&bus);

        assert_eq!(value, 0x00, "0xFFFF currently reads interrupt enable");
        assert_eq!(cpu.registers().pc, 0x0000, "fetch8 should wrap PC");
    }

    #[test]
    fn fetch16_reads_little_endian_operand_and_advances_pc_twice() {
        let bus = bus_with_bytes(&[(0x0100, 0x34), (0x0101, 0x12)]);
        let mut cpu = Cpu::new_dmg_post_boot();

        let value = cpu.fetch16(&bus);

        assert_eq!(
            value, 0x1234,
            "fetch16 should combine low byte first, then high byte"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0102,
            "fetch16 should advance PC by two"
        );
    }

    #[test]
    fn step_executes_nop_and_returns_four_t_cycles() {
        let mut bus = bus_with_bytes(&[(0x0100, 0x00)]);
        let mut cpu = Cpu::new_dmg_post_boot();

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(4)), "NOP should take 4 T-cycles");
        assert_eq!(
            cpu.registers().pc,
            0x0101,
            "NOP should leave PC after the opcode byte"
        );
    }

    #[test]
    fn step_reports_unknown_opcode_with_original_pc() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xDD)]);
        let mut cpu = Cpu::new_dmg_post_boot();

        let error = cpu.step(&mut bus);

        assert_eq!(
            error,
            Err(CpuError::UnimplementedOpcode {
                pc: 0x0100,
                opcode: 0xDD
            }),
            "unknown opcode errors should include the fetch address and opcode"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0101,
            "PC should still reflect that the opcode byte was fetched"
        );
    }

    #[test]
    fn trace_next_instruction_formats_stable_cpu_state_without_advancing_pc() {
        let bus = bus_with_bytes(&[(0x0100, 0x00)]);
        let cpu = Cpu::new_dmg_post_boot();

        let trace = cpu.trace_next_instruction(&bus);

        assert_eq!(
            trace, "PC=0100 OP=00 AF=01B0 BC=0013 DE=00D8 HL=014D SP=FFFE",
            "trace output should be stable for debugger and test tooling"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0100,
            "trace formatting should not fetch or mutate PC"
        );
    }
}
