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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Register8 {
    A,
    B,
    C,
    D,
    E,
    H,
    L,
}

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
    /// # Errors
    ///
    /// Returns [`CpuError::UnimplementedOpcode`] when the fetched opcode is not
    /// implemented yet.
    pub fn step(&mut self, bus: &mut Bus) -> Result<TCycles, CpuError> {
        let pc = self.registers.pc;
        let opcode = self.fetch8(bus);

        match opcode {
            0x00 => Ok(TCycles(4)),
            0x01 => Ok(self.ld_rr_d16(RegisterPair::BC, bus)),
            0x02 => Ok(self.ld_addr_rr_a(RegisterPair::BC, bus)),
            0x06 => Ok(self.ld_r_d8(Register8::B, bus)),
            0x0A => Ok(self.ld_a_addr_rr(RegisterPair::BC, bus)),
            0x0E => Ok(self.ld_r_d8(Register8::C, bus)),
            0x11 => Ok(self.ld_rr_d16(RegisterPair::DE, bus)),
            0x12 => Ok(self.ld_addr_rr_a(RegisterPair::DE, bus)),
            0x16 => Ok(self.ld_r_d8(Register8::D, bus)),
            0x1A => Ok(self.ld_a_addr_rr(RegisterPair::DE, bus)),
            0x1E => Ok(self.ld_r_d8(Register8::E, bus)),
            0x21 => Ok(self.ld_rr_d16(RegisterPair::HL, bus)),
            0x26 => Ok(self.ld_r_d8(Register8::H, bus)),
            0x2E => Ok(self.ld_r_d8(Register8::L, bus)),
            0x31 => Ok(self.ld_rr_d16(RegisterPair::SP, bus)),
            0x36 => Ok(self.ld_addr_hl_d8(bus)),
            0x3E => Ok(self.ld_r_d8(Register8::A, bus)),
            0x40 => Ok(self.ld_r_r(Register8::B, Register8::B)),
            0x41 => Ok(self.ld_r_r(Register8::B, Register8::C)),
            0x42 => Ok(self.ld_r_r(Register8::B, Register8::D)),
            0x43 => Ok(self.ld_r_r(Register8::B, Register8::E)),
            0x44 => Ok(self.ld_r_r(Register8::B, Register8::H)),
            0x45 => Ok(self.ld_r_r(Register8::B, Register8::L)),
            0x46 => Ok(self.ld_r_addr_hl(Register8::B, bus)),
            0x47 => Ok(self.ld_r_r(Register8::B, Register8::A)),
            0x48 => Ok(self.ld_r_r(Register8::C, Register8::B)),
            0x49 => Ok(self.ld_r_r(Register8::C, Register8::C)),
            0x4A => Ok(self.ld_r_r(Register8::C, Register8::D)),
            0x4B => Ok(self.ld_r_r(Register8::C, Register8::E)),
            0x4C => Ok(self.ld_r_r(Register8::C, Register8::H)),
            0x4D => Ok(self.ld_r_r(Register8::C, Register8::L)),
            0x4E => Ok(self.ld_r_addr_hl(Register8::C, bus)),
            0x4F => Ok(self.ld_r_r(Register8::C, Register8::A)),
            0x50 => Ok(self.ld_r_r(Register8::D, Register8::B)),
            0x51 => Ok(self.ld_r_r(Register8::D, Register8::C)),
            0x52 => Ok(self.ld_r_r(Register8::D, Register8::D)),
            0x53 => Ok(self.ld_r_r(Register8::D, Register8::E)),
            0x54 => Ok(self.ld_r_r(Register8::D, Register8::H)),
            0x55 => Ok(self.ld_r_r(Register8::D, Register8::L)),
            0x56 => Ok(self.ld_r_addr_hl(Register8::D, bus)),
            0x57 => Ok(self.ld_r_r(Register8::D, Register8::A)),
            0x58 => Ok(self.ld_r_r(Register8::E, Register8::B)),
            0x59 => Ok(self.ld_r_r(Register8::E, Register8::C)),
            0x5A => Ok(self.ld_r_r(Register8::E, Register8::D)),
            0x5B => Ok(self.ld_r_r(Register8::E, Register8::E)),
            0x5C => Ok(self.ld_r_r(Register8::E, Register8::H)),
            0x5D => Ok(self.ld_r_r(Register8::E, Register8::L)),
            0x5E => Ok(self.ld_r_addr_hl(Register8::E, bus)),
            0x5F => Ok(self.ld_r_r(Register8::E, Register8::A)),
            0x60 => Ok(self.ld_r_r(Register8::H, Register8::B)),
            0x61 => Ok(self.ld_r_r(Register8::H, Register8::C)),
            0x62 => Ok(self.ld_r_r(Register8::H, Register8::D)),
            0x63 => Ok(self.ld_r_r(Register8::H, Register8::E)),
            0x64 => Ok(self.ld_r_r(Register8::H, Register8::H)),
            0x65 => Ok(self.ld_r_r(Register8::H, Register8::L)),
            0x66 => Ok(self.ld_r_addr_hl(Register8::H, bus)),
            0x67 => Ok(self.ld_r_r(Register8::H, Register8::A)),
            0x68 => Ok(self.ld_r_r(Register8::L, Register8::B)),
            0x69 => Ok(self.ld_r_r(Register8::L, Register8::C)),
            0x6A => Ok(self.ld_r_r(Register8::L, Register8::D)),
            0x6B => Ok(self.ld_r_r(Register8::L, Register8::E)),
            0x6C => Ok(self.ld_r_r(Register8::L, Register8::H)),
            0x6D => Ok(self.ld_r_r(Register8::L, Register8::L)),
            0x6E => Ok(self.ld_r_addr_hl(Register8::L, bus)),
            0x6F => Ok(self.ld_r_r(Register8::L, Register8::A)),
            0x70 => Ok(self.ld_addr_hl_r(Register8::B, bus)),
            0x71 => Ok(self.ld_addr_hl_r(Register8::C, bus)),
            0x72 => Ok(self.ld_addr_hl_r(Register8::D, bus)),
            0x73 => Ok(self.ld_addr_hl_r(Register8::E, bus)),
            0x74 => Ok(self.ld_addr_hl_r(Register8::H, bus)),
            0x75 => Ok(self.ld_addr_hl_r(Register8::L, bus)),
            0x77 => Ok(self.ld_addr_hl_a(bus)),
            0x78 => Ok(self.ld_r_r(Register8::A, Register8::B)),
            0x79 => Ok(self.ld_r_r(Register8::A, Register8::C)),
            0x7A => Ok(self.ld_r_r(Register8::A, Register8::D)),
            0x7B => Ok(self.ld_r_r(Register8::A, Register8::E)),
            0x7C => Ok(self.ld_r_r(Register8::A, Register8::H)),
            0x7D => Ok(self.ld_r_r(Register8::A, Register8::L)),
            0x7E => Ok(self.ld_a_addr_hl(bus)),
            0x7F => Ok(self.ld_r_r(Register8::A, Register8::A)),
            0xE0 => Ok(self.ldh_addr_a8_a(bus)),
            0xEA => Ok(self.ld_addr_a16_a(bus)),
            0xF0 => Ok(self.ldh_a_addr_a8(bus)),
            0xFA => Ok(self.ld_a_addr_a16(bus)),
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

    fn read_register8(&self, register: Register8) -> u8 {
        match register {
            Register8::A => self.registers.a,
            Register8::B => self.registers.b,
            Register8::C => self.registers.c,
            Register8::D => self.registers.d,
            Register8::E => self.registers.e,
            Register8::H => self.registers.h,
            Register8::L => self.registers.l,
        }
    }

    fn write_register8(&mut self, register: Register8, value: u8) {
        match register {
            Register8::A => self.registers.a = value,
            Register8::B => self.registers.b = value,
            Register8::C => self.registers.c = value,
            Register8::D => self.registers.d = value,
            Register8::E => self.registers.e = value,
            Register8::H => self.registers.h = value,
            Register8::L => self.registers.l = value,
        }
    }

    fn ld_r_d8(&mut self, register: Register8, bus: &Bus) -> TCycles {
        let value = self.fetch8(bus);
        self.write_register8(register, value);

        TCycles(8)
    }

    fn ld_r_r(&mut self, destination: Register8, source: Register8) -> TCycles {
        let value = self.read_register8(source);
        self.write_register8(destination, value);

        TCycles(4)
    }

    fn ld_r_addr_hl(&mut self, register: Register8, bus: &Bus) -> TCycles {
        let value = bus.read8(self.registers.hl());
        self.write_register8(register, value);

        TCycles(8)
    }

    fn ld_addr_hl_r(&mut self, register: Register8, bus: &mut Bus) -> TCycles {
        bus.write8(self.registers.hl(), self.read_register8(register));

        TCycles(8)
    }

    fn ld_a_addr_hl(&mut self, bus: &Bus) -> TCycles {
        self.registers.a = bus.read8(self.registers.hl());

        TCycles(8)
    }

    fn ld_addr_hl_a(&mut self, bus: &mut Bus) -> TCycles {
        bus.write8(self.registers.hl(), self.registers.a);

        TCycles(8)
    }

    fn ld_addr_hl_d8(&mut self, bus: &mut Bus) -> TCycles {
        let value = self.fetch8(bus);
        bus.write8(self.registers.hl(), value);

        TCycles(12)
    }

    fn ld_a_addr_rr(&mut self, pair: RegisterPair, bus: &Bus) -> TCycles {
        self.registers.a = bus.read8(self.read_register_pair(pair));

        TCycles(8)
    }

    fn ld_addr_rr_a(&mut self, pair: RegisterPair, bus: &mut Bus) -> TCycles {
        bus.write8(self.read_register_pair(pair), self.registers.a);

        TCycles(8)
    }

    fn ldh_a_addr_a8(&mut self, bus: &Bus) -> TCycles {
        let offset = self.fetch8(bus);
        self.registers.a = bus.read8(0xFF00 + u16::from(offset));

        TCycles(12)
    }

    fn ldh_addr_a8_a(&mut self, bus: &mut Bus) -> TCycles {
        let offset = self.fetch8(bus);
        bus.write8(0xFF00 + u16::from(offset), self.registers.a);

        TCycles(12)
    }

    fn ld_a_addr_a16(&mut self, bus: &Bus) -> TCycles {
        let address = self.fetch16(bus);
        self.registers.a = bus.read8(address);

        TCycles(16)
    }

    fn ld_addr_a16_a(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.fetch16(bus);
        bus.write8(address, self.registers.a);

        TCycles(16)
    }

    fn ld_rr_d16(&mut self, pair: RegisterPair, bus: &Bus) -> TCycles {
        let value = self.fetch16(bus);
        self.write_register_pair(pair, value);

        TCycles(12)
    }

    fn read_register_pair(&self, pair: RegisterPair) -> u16 {
        match pair {
            RegisterPair::BC => self.registers.bc(),
            RegisterPair::DE => self.registers.de(),
            RegisterPair::HL => self.registers.hl(),
            RegisterPair::SP => self.registers.sp,
        }
    }

    fn write_register_pair(&mut self, pair: RegisterPair, value: u16) {
        match pair {
            RegisterPair::BC => self.registers.set_bc(value),
            RegisterPair::DE => self.registers.set_de(value),
            RegisterPair::HL => self.registers.set_hl(value),
            RegisterPair::SP => self.registers.sp = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisterPair {
    BC,
    DE,
    HL,
    SP,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new_dmg_post_boot()
    }
}

#[cfg(test)]
mod tests {
    use super::{Cpu, CpuError, Register8, TCycles};
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

    fn run_instruction(bytes: &[u8]) -> (Cpu, Bus, TCycles) {
        let mut bus = bus_with_bytes(
            &bytes
                .iter()
                .enumerate()
                .map(|(offset, value)| (0x0100 + offset, *value))
                .collect::<Vec<_>>(),
        );
        let mut cpu = Cpu::new_dmg_post_boot();
        let cycles = cpu.step(&mut bus).expect("instruction should execute");

        (cpu, bus, cycles)
    }

    fn read_register(cpu: &Cpu, register: Register8) -> u8 {
        cpu.read_register8(register)
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

    #[test]
    fn ld_r_d8_sets_each_8_bit_register() {
        let cases = [
            (0x06, Register8::B, 0xB1),
            (0x0E, Register8::C, 0xC2),
            (0x16, Register8::D, 0xD3),
            (0x1E, Register8::E, 0xE4),
            (0x26, Register8::H, 0x55),
            (0x2E, Register8::L, 0x66),
            (0x3E, Register8::A, 0xA7),
        ];

        for (opcode, register, value) in cases {
            let (cpu, _bus, cycles) = run_instruction(&[opcode, value]);

            assert_eq!(
                read_register(&cpu, register),
                value,
                "opcode {opcode:02X} should load immediate value into {register:?}"
            );
            assert_eq!(cycles, TCycles(8), "LD r,d8 should take 8 T-cycles");
            assert_eq!(
                cpu.registers().pc,
                0x0102,
                "LD r,d8 should consume two bytes"
            );
        }
    }

    #[test]
    fn ld_rr_d16_sets_register_pairs_little_endian() {
        let cases: [(u8, &str, u16); 4] = [
            (0x01, "BC", 0x1234),
            (0x11, "DE", 0x5678),
            (0x21, "HL", 0x9ABC),
            (0x31, "SP", 0xC001),
        ];

        for (opcode, pair_name, expected) in cases {
            let [low, high] = expected.to_le_bytes();
            let (cpu, _bus, cycles) = run_instruction(&[opcode, low, high]);
            let actual = match pair_name {
                "BC" => cpu.registers().bc(),
                "DE" => cpu.registers().de(),
                "HL" => cpu.registers().hl(),
                "SP" => cpu.registers().sp,
                _ => unreachable!("test case uses known register pairs"),
            };

            assert_eq!(
                actual, expected,
                "opcode {opcode:02X} should load {pair_name} from little-endian d16"
            );
            assert_eq!(cycles, TCycles(12), "LD rr,d16 should take 12 T-cycles");
            assert_eq!(
                cpu.registers().pc,
                0x0103,
                "LD rr,d16 should consume three bytes"
            );
        }
    }

    #[test]
    fn ld_r_r_moves_values_between_8_bit_registers() {
        let cases = [
            (0x40, Register8::B, Register8::B),
            (0x41, Register8::B, Register8::C),
            (0x42, Register8::B, Register8::D),
            (0x43, Register8::B, Register8::E),
            (0x44, Register8::B, Register8::H),
            (0x45, Register8::B, Register8::L),
            (0x47, Register8::B, Register8::A),
            (0x48, Register8::C, Register8::B),
            (0x49, Register8::C, Register8::C),
            (0x4A, Register8::C, Register8::D),
            (0x4B, Register8::C, Register8::E),
            (0x4C, Register8::C, Register8::H),
            (0x4D, Register8::C, Register8::L),
            (0x4F, Register8::C, Register8::A),
            (0x50, Register8::D, Register8::B),
            (0x51, Register8::D, Register8::C),
            (0x52, Register8::D, Register8::D),
            (0x53, Register8::D, Register8::E),
            (0x54, Register8::D, Register8::H),
            (0x55, Register8::D, Register8::L),
            (0x57, Register8::D, Register8::A),
            (0x58, Register8::E, Register8::B),
            (0x59, Register8::E, Register8::C),
            (0x5A, Register8::E, Register8::D),
            (0x5B, Register8::E, Register8::E),
            (0x5C, Register8::E, Register8::H),
            (0x5D, Register8::E, Register8::L),
            (0x5F, Register8::E, Register8::A),
            (0x60, Register8::H, Register8::B),
            (0x61, Register8::H, Register8::C),
            (0x62, Register8::H, Register8::D),
            (0x63, Register8::H, Register8::E),
            (0x64, Register8::H, Register8::H),
            (0x65, Register8::H, Register8::L),
            (0x67, Register8::H, Register8::A),
            (0x68, Register8::L, Register8::B),
            (0x69, Register8::L, Register8::C),
            (0x6A, Register8::L, Register8::D),
            (0x6B, Register8::L, Register8::E),
            (0x6C, Register8::L, Register8::H),
            (0x6D, Register8::L, Register8::L),
            (0x6F, Register8::L, Register8::A),
            (0x78, Register8::A, Register8::B),
            (0x79, Register8::A, Register8::C),
            (0x7A, Register8::A, Register8::D),
            (0x7B, Register8::A, Register8::E),
            (0x7C, Register8::A, Register8::H),
            (0x7D, Register8::A, Register8::L),
            (0x7F, Register8::A, Register8::A),
        ];

        for (opcode, destination, source) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.a = 0xA1;
            cpu.registers.b = 0xB2;
            cpu.registers.c = 0xC3;
            cpu.registers.d = 0xD4;
            cpu.registers.e = 0xE5;
            cpu.registers.h = 0xF6;
            cpu.registers.l = 0x17;
            let expected = read_register(&cpu, source);

            let cycles = cpu.step(&mut bus).expect("LD r,r should execute");

            assert_eq!(
                read_register(&cpu, destination),
                expected,
                "opcode {opcode:02X} should copy {source:?} into {destination:?}"
            );
            assert_eq!(cycles, TCycles(4), "LD r,r should take 4 T-cycles");
            assert_eq!(cpu.registers().pc, 0x0101, "LD r,r should consume one byte");
        }
    }

    #[test]
    fn ld_a_addr_hl_reads_from_memory_pointed_to_by_hl() {
        let mut bus = bus_with_bytes(&[(0x0100, 0x7E)]);
        bus.write8(0xC123, 0x5A);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.set_hl(0xC123);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(8)), "LD A,(HL) should take 8 T-cycles");
        assert_eq!(
            cpu.registers().a,
            0x5A,
            "A should receive the byte read through Bus"
        );
    }

    #[test]
    fn ld_addr_hl_a_writes_a_to_memory_pointed_to_by_hl() {
        let mut bus = bus_with_bytes(&[(0x0100, 0x77)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.set_hl(0xC456);
        cpu.registers.a = 0xA5;

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(8)), "LD (HL),A should take 8 T-cycles");
        assert_eq!(bus.read8(0xC456), 0xA5, "WRAM should receive A through Bus");
    }

    #[test]
    fn ld_addr_hl_d8_writes_immediate_to_memory_pointed_to_by_hl() {
        let mut bus = bus_with_bytes(&[(0x0100, 0x36), (0x0101, 0x3C)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.set_hl(0xC789);

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(12)),
            "LD (HL),d8 should take 12 T-cycles"
        );
        assert_eq!(
            bus.read8(0xC789),
            0x3C,
            "WRAM should receive the immediate byte"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0102,
            "LD (HL),d8 should consume two bytes"
        );
    }

    #[test]
    fn ld_r_addr_hl_reads_memory_pointed_to_by_hl_into_each_register() {
        let cases = [
            (0x46, Register8::B),
            (0x4E, Register8::C),
            (0x56, Register8::D),
            (0x5E, Register8::E),
            (0x66, Register8::H),
            (0x6E, Register8::L),
            (0x7E, Register8::A),
        ];

        for (opcode, register) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            bus.write8(0xC123, 0x8F);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.set_hl(0xC123);

            let cycles = cpu.step(&mut bus);

            assert_eq!(
                cycles,
                Ok(TCycles(8)),
                "opcode {opcode:02X} should take 8 T-cycles"
            );
            assert_eq!(
                read_register(&cpu, register),
                0x8F,
                "opcode {opcode:02X} should load {register:?} from the old HL address"
            );
        }
    }

    #[test]
    fn ld_addr_hl_r_writes_each_register_to_memory_pointed_to_by_hl() {
        let cases = [
            (0x70, Register8::B, 0xB1),
            (0x71, Register8::C, 0xC2),
            (0x72, Register8::D, 0xD3),
            (0x73, Register8::E, 0xE4),
            (0x74, Register8::H, 0xC4),
            (0x75, Register8::L, 0x56),
            (0x77, Register8::A, 0xA8),
        ];

        for (opcode, register, value) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.set_hl(0xC456);
            cpu.write_register8(register, value);

            let cycles = cpu.step(&mut bus);

            assert_eq!(
                cycles,
                Ok(TCycles(8)),
                "opcode {opcode:02X} should take 8 T-cycles"
            );
            assert_eq!(
                bus.read8(0xC456),
                value,
                "opcode {opcode:02X} should write {register:?} to the old HL address"
            );
        }
    }

    #[test]
    fn ld_a_addr_bc_and_de_read_through_register_pairs() {
        for (opcode, pair_name, address) in [(0x0A, "BC", 0xC010), (0x1A, "DE", 0xC020)] {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            bus.write8(address, 0x86);
            let mut cpu = Cpu::new_dmg_post_boot();
            match pair_name {
                "BC" => cpu.registers.set_bc(address),
                "DE" => cpu.registers.set_de(address),
                _ => unreachable!("test case uses known register pairs"),
            }

            let cycles = cpu.step(&mut bus);

            assert_eq!(
                cycles,
                Ok(TCycles(8)),
                "LD A,({pair_name}) should take 8 T-cycles"
            );
            assert_eq!(cpu.registers().a, 0x86, "A should read through {pair_name}");
        }
    }

    #[test]
    fn ld_addr_bc_and_de_a_write_through_register_pairs() {
        for (opcode, pair_name, address) in [(0x02, "BC", 0xC030), (0x12, "DE", 0xC040)] {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.a = 0x68;
            match pair_name {
                "BC" => cpu.registers.set_bc(address),
                "DE" => cpu.registers.set_de(address),
                _ => unreachable!("test case uses known register pairs"),
            }

            let cycles = cpu.step(&mut bus);

            assert_eq!(
                cycles,
                Ok(TCycles(8)),
                "LD ({pair_name}),A should take 8 T-cycles"
            );
            assert_eq!(
                bus.read8(address),
                0x68,
                "{pair_name} address should receive A"
            );
        }
    }

    #[test]
    fn ldh_a_addr_a8_reads_from_high_memory() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xF0), (0x0101, 0x80)]);
        bus.write8(0xFF80, 0x4D);
        let mut cpu = Cpu::new_dmg_post_boot();

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(12)),
            "LDH A,(a8) should take 12 T-cycles"
        );
        assert_eq!(cpu.registers().a, 0x4D, "A should read from 0xFF00 + a8");
        assert_eq!(
            cpu.registers().pc,
            0x0102,
            "LDH A,(a8) should consume two bytes"
        );
    }

    #[test]
    fn ldh_addr_a8_a_writes_to_high_memory() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xE0), (0x0101, 0x80)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.a = 0xD9;

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(12)),
            "LDH (a8),A should take 12 T-cycles"
        );
        assert_eq!(bus.read8(0xFF80), 0xD9, "0xFF00 + a8 should receive A");
    }

    #[test]
    fn ld_a_addr_a16_reads_from_absolute_memory() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xFA), (0x0101, 0x34), (0x0102, 0xC1)]);
        bus.write8(0xC134, 0x9E);
        let mut cpu = Cpu::new_dmg_post_boot();

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(16)),
            "LD A,(a16) should take 16 T-cycles"
        );
        assert_eq!(
            cpu.registers().a,
            0x9E,
            "A should read from absolute address"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0103,
            "LD A,(a16) should consume three bytes"
        );
    }

    #[test]
    fn ld_addr_a16_a_writes_to_absolute_memory() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xEA), (0x0101, 0x78), (0x0102, 0xC5)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.a = 0x2B;

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(16)),
            "LD (a16),A should take 16 T-cycles"
        );
        assert_eq!(bus.read8(0xC578), 0x2B, "absolute address should receive A");
    }
}
