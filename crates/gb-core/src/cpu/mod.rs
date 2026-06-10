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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Condition {
    NotZero,
    Zero,
    NotCarry,
    Carry,
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
    #[allow(clippy::too_many_lines)]
    pub fn step(&mut self, bus: &mut Bus) -> Result<TCycles, CpuError> {
        let pc = self.registers.pc;
        let opcode = self.fetch8(bus);

        match opcode {
            0x00 => Ok(TCycles(4)),
            0x01 => Ok(self.ld_rr_d16(RegisterPair::BC, bus)),
            0x02 => Ok(self.ld_addr_rr_a(RegisterPair::BC, bus)),
            0x03 => Ok(self.inc_rr(RegisterPair::BC)),
            0x04 => Ok(self.inc_r(Register8::B)),
            0x05 => Ok(self.dec_r(Register8::B)),
            0x06 => Ok(self.ld_r_d8(Register8::B, bus)),
            0x09 => Ok(self.add_hl_rr(RegisterPair::BC)),
            0x0A => Ok(self.ld_a_addr_rr(RegisterPair::BC, bus)),
            0x0B => Ok(self.dec_rr(RegisterPair::BC)),
            0x0C => Ok(self.inc_r(Register8::C)),
            0x0D => Ok(self.dec_r(Register8::C)),
            0x0E => Ok(self.ld_r_d8(Register8::C, bus)),
            0x11 => Ok(self.ld_rr_d16(RegisterPair::DE, bus)),
            0x12 => Ok(self.ld_addr_rr_a(RegisterPair::DE, bus)),
            0x13 => Ok(self.inc_rr(RegisterPair::DE)),
            0x14 => Ok(self.inc_r(Register8::D)),
            0x15 => Ok(self.dec_r(Register8::D)),
            0x16 => Ok(self.ld_r_d8(Register8::D, bus)),
            0x19 => Ok(self.add_hl_rr(RegisterPair::DE)),
            0x1A => Ok(self.ld_a_addr_rr(RegisterPair::DE, bus)),
            0x1B => Ok(self.dec_rr(RegisterPair::DE)),
            0x1C => Ok(self.inc_r(Register8::E)),
            0x1D => Ok(self.dec_r(Register8::E)),
            0x1E => Ok(self.ld_r_d8(Register8::E, bus)),
            0x18 => Ok(self.jr_e8(bus)),
            0x20 => Ok(self.jr_cc_e8(Condition::NotZero, bus)),
            0x21 => Ok(self.ld_rr_d16(RegisterPair::HL, bus)),
            0x23 => Ok(self.inc_rr(RegisterPair::HL)),
            0x24 => Ok(self.inc_r(Register8::H)),
            0x25 => Ok(self.dec_r(Register8::H)),
            0x26 => Ok(self.ld_r_d8(Register8::H, bus)),
            0x27 => Ok(self.daa()),
            0x28 => Ok(self.jr_cc_e8(Condition::Zero, bus)),
            0x29 => Ok(self.add_hl_rr(RegisterPair::HL)),
            0x2B => Ok(self.dec_rr(RegisterPair::HL)),
            0x2C => Ok(self.inc_r(Register8::L)),
            0x2D => Ok(self.dec_r(Register8::L)),
            0x2E => Ok(self.ld_r_d8(Register8::L, bus)),
            0x2F => Ok(self.cpl()),
            0x30 => Ok(self.jr_cc_e8(Condition::NotCarry, bus)),
            0x31 => Ok(self.ld_rr_d16(RegisterPair::SP, bus)),
            0x33 => Ok(self.inc_rr(RegisterPair::SP)),
            0x36 => Ok(self.ld_addr_hl_d8(bus)),
            0x37 => Ok(self.scf()),
            0x38 => Ok(self.jr_cc_e8(Condition::Carry, bus)),
            0x39 => Ok(self.add_hl_rr(RegisterPair::SP)),
            0x3B => Ok(self.dec_rr(RegisterPair::SP)),
            0x3C => Ok(self.inc_r(Register8::A)),
            0x3D => Ok(self.dec_r(Register8::A)),
            0x3E => Ok(self.ld_r_d8(Register8::A, bus)),
            0x3F => Ok(self.ccf()),
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
            0x80 => Ok(self.add_a_r(Register8::B)),
            0x81 => Ok(self.add_a_r(Register8::C)),
            0x82 => Ok(self.add_a_r(Register8::D)),
            0x83 => Ok(self.add_a_r(Register8::E)),
            0x84 => Ok(self.add_a_r(Register8::H)),
            0x85 => Ok(self.add_a_r(Register8::L)),
            0x87 => Ok(self.add_a_r(Register8::A)),
            0x88 => Ok(self.adc_a_r(Register8::B)),
            0x89 => Ok(self.adc_a_r(Register8::C)),
            0x8A => Ok(self.adc_a_r(Register8::D)),
            0x8B => Ok(self.adc_a_r(Register8::E)),
            0x8C => Ok(self.adc_a_r(Register8::H)),
            0x8D => Ok(self.adc_a_r(Register8::L)),
            0x8F => Ok(self.adc_a_r(Register8::A)),
            0x90 => Ok(self.sub_a_r(Register8::B)),
            0x91 => Ok(self.sub_a_r(Register8::C)),
            0x92 => Ok(self.sub_a_r(Register8::D)),
            0x93 => Ok(self.sub_a_r(Register8::E)),
            0x94 => Ok(self.sub_a_r(Register8::H)),
            0x95 => Ok(self.sub_a_r(Register8::L)),
            0x97 => Ok(self.sub_a_r(Register8::A)),
            0x98 => Ok(self.sbc_a_r(Register8::B)),
            0x99 => Ok(self.sbc_a_r(Register8::C)),
            0x9A => Ok(self.sbc_a_r(Register8::D)),
            0x9B => Ok(self.sbc_a_r(Register8::E)),
            0x9C => Ok(self.sbc_a_r(Register8::H)),
            0x9D => Ok(self.sbc_a_r(Register8::L)),
            0x9F => Ok(self.sbc_a_r(Register8::A)),
            0xA0 => Ok(self.and_a_r(Register8::B)),
            0xA1 => Ok(self.and_a_r(Register8::C)),
            0xA2 => Ok(self.and_a_r(Register8::D)),
            0xA3 => Ok(self.and_a_r(Register8::E)),
            0xA4 => Ok(self.and_a_r(Register8::H)),
            0xA5 => Ok(self.and_a_r(Register8::L)),
            0xA7 => Ok(self.and_a_r(Register8::A)),
            0xA8 => Ok(self.xor_a_r(Register8::B)),
            0xA9 => Ok(self.xor_a_r(Register8::C)),
            0xAA => Ok(self.xor_a_r(Register8::D)),
            0xAB => Ok(self.xor_a_r(Register8::E)),
            0xAC => Ok(self.xor_a_r(Register8::H)),
            0xAD => Ok(self.xor_a_r(Register8::L)),
            0xAF => Ok(self.xor_a_r(Register8::A)),
            0xB0 => Ok(self.or_a_r(Register8::B)),
            0xB1 => Ok(self.or_a_r(Register8::C)),
            0xB2 => Ok(self.or_a_r(Register8::D)),
            0xB3 => Ok(self.or_a_r(Register8::E)),
            0xB4 => Ok(self.or_a_r(Register8::H)),
            0xB5 => Ok(self.or_a_r(Register8::L)),
            0xB7 => Ok(self.or_a_r(Register8::A)),
            0xB8 => Ok(self.cp_a_r(Register8::B)),
            0xB9 => Ok(self.cp_a_r(Register8::C)),
            0xBA => Ok(self.cp_a_r(Register8::D)),
            0xBB => Ok(self.cp_a_r(Register8::E)),
            0xBC => Ok(self.cp_a_r(Register8::H)),
            0xBD => Ok(self.cp_a_r(Register8::L)),
            0xBF => Ok(self.cp_a_r(Register8::A)),
            0xC0 => Ok(self.ret_cc(Condition::NotZero, bus)),
            0xC2 => Ok(self.jp_cc_a16(Condition::NotZero, bus)),
            0xC3 => Ok(self.jp_a16(bus)),
            0xC4 => Ok(self.call_cc_a16(Condition::NotZero, bus)),
            0xC6 => Ok(self.add_a_d8(bus)),
            0xC7 => Ok(self.rst(0x00, bus)),
            0xC8 => Ok(self.ret_cc(Condition::Zero, bus)),
            0xC9 => Ok(self.ret(bus)),
            0xCA => Ok(self.jp_cc_a16(Condition::Zero, bus)),
            0xCC => Ok(self.call_cc_a16(Condition::Zero, bus)),
            0xCD => Ok(self.call_a16(bus)),
            0xCE => Ok(self.adc_a_d8(bus)),
            0xCF => Ok(self.rst(0x08, bus)),
            0xD0 => Ok(self.ret_cc(Condition::NotCarry, bus)),
            0xD2 => Ok(self.jp_cc_a16(Condition::NotCarry, bus)),
            0xD4 => Ok(self.call_cc_a16(Condition::NotCarry, bus)),
            0xD6 => Ok(self.sub_a_d8(bus)),
            0xD7 => Ok(self.rst(0x10, bus)),
            0xD8 => Ok(self.ret_cc(Condition::Carry, bus)),
            0xDA => Ok(self.jp_cc_a16(Condition::Carry, bus)),
            0xDC => Ok(self.call_cc_a16(Condition::Carry, bus)),
            0xDE => Ok(self.sbc_a_d8(bus)),
            0xDF => Ok(self.rst(0x18, bus)),
            0xE0 => Ok(self.ldh_addr_a8_a(bus)),
            0xE6 => Ok(self.and_a_d8(bus)),
            0xE7 => Ok(self.rst(0x20, bus)),
            0xE8 => Ok(self.add_sp_e8(bus)),
            0xE9 => Ok(self.jp_hl()),
            0xEA => Ok(self.ld_addr_a16_a(bus)),
            0xEE => Ok(self.xor_a_d8(bus)),
            0xEF => Ok(self.rst(0x28, bus)),
            0xF0 => Ok(self.ldh_a_addr_a8(bus)),
            0xF6 => Ok(self.or_a_d8(bus)),
            0xF7 => Ok(self.rst(0x30, bus)),
            0xF8 => Ok(self.ld_hl_sp_e8(bus)),
            0xF9 => Ok(self.ld_sp_hl()),
            0xFA => Ok(self.ld_a_addr_a16(bus)),
            0xFE => Ok(self.cp_a_d8(bus)),
            0xFF => Ok(self.rst(0x38, bus)),
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

    fn inc_r(&mut self, register: Register8) -> TCycles {
        let value = self.read_register8(register);
        let result = value.wrapping_add(1);

        self.write_register8(register, result);
        self.registers.f.set_zero(result == 0);
        self.registers.f.set_subtract(false);
        self.registers
            .f
            .set_half_carry((value & 0x0F).wrapping_add(1) > 0x0F);

        TCycles(4)
    }

    fn dec_r(&mut self, register: Register8) -> TCycles {
        let value = self.read_register8(register);
        let result = value.wrapping_sub(1);

        self.write_register8(register, result);
        self.registers.f.set_zero(result == 0);
        self.registers.f.set_subtract(true);
        self.registers.f.set_half_carry(value.trailing_zeros() >= 4);

        TCycles(4)
    }

    fn add_a_r(&mut self, register: Register8) -> TCycles {
        self.alu_add(self.read_register8(register));
        TCycles(4)
    }

    fn adc_a_r(&mut self, register: Register8) -> TCycles {
        self.alu_adc(self.read_register8(register));
        TCycles(4)
    }

    fn sub_a_r(&mut self, register: Register8) -> TCycles {
        self.alu_sub(self.read_register8(register));
        TCycles(4)
    }

    fn sbc_a_r(&mut self, register: Register8) -> TCycles {
        self.alu_sbc(self.read_register8(register));
        TCycles(4)
    }

    fn and_a_r(&mut self, register: Register8) -> TCycles {
        self.alu_and(self.read_register8(register));
        TCycles(4)
    }

    fn or_a_r(&mut self, register: Register8) -> TCycles {
        self.alu_or(self.read_register8(register));
        TCycles(4)
    }

    fn xor_a_r(&mut self, register: Register8) -> TCycles {
        self.alu_xor(self.read_register8(register));
        TCycles(4)
    }

    fn cp_a_r(&mut self, register: Register8) -> TCycles {
        self.alu_cp(self.read_register8(register));
        TCycles(4)
    }

    fn add_a_d8(&mut self, bus: &Bus) -> TCycles {
        let value = self.fetch8(bus);
        self.alu_add(value);
        TCycles(8)
    }

    fn adc_a_d8(&mut self, bus: &Bus) -> TCycles {
        let value = self.fetch8(bus);
        self.alu_adc(value);
        TCycles(8)
    }

    fn sub_a_d8(&mut self, bus: &Bus) -> TCycles {
        let value = self.fetch8(bus);
        self.alu_sub(value);
        TCycles(8)
    }

    fn sbc_a_d8(&mut self, bus: &Bus) -> TCycles {
        let value = self.fetch8(bus);
        self.alu_sbc(value);
        TCycles(8)
    }

    fn and_a_d8(&mut self, bus: &Bus) -> TCycles {
        let value = self.fetch8(bus);
        self.alu_and(value);
        TCycles(8)
    }

    fn or_a_d8(&mut self, bus: &Bus) -> TCycles {
        let value = self.fetch8(bus);
        self.alu_or(value);
        TCycles(8)
    }

    fn xor_a_d8(&mut self, bus: &Bus) -> TCycles {
        let value = self.fetch8(bus);
        self.alu_xor(value);
        TCycles(8)
    }

    fn cp_a_d8(&mut self, bus: &Bus) -> TCycles {
        let value = self.fetch8(bus);
        self.alu_cp(value);
        TCycles(8)
    }

    fn alu_add(&mut self, value: u8) {
        let a = self.registers.a;
        let (result, carry) = a.overflowing_add(value);

        self.registers.a = result;
        self.registers.f.set_zero(result == 0);
        self.registers.f.set_subtract(false);
        self.registers
            .f
            .set_half_carry((a & 0x0F) + (value & 0x0F) > 0x0F);
        self.registers.f.set_carry(carry);
    }

    fn alu_adc(&mut self, value: u8) {
        let carry_in = u8::from(self.registers.f.carry());
        let a = self.registers.a;
        let result = a.wrapping_add(value).wrapping_add(carry_in);

        self.registers.a = result;
        self.registers.f.set_zero(result == 0);
        self.registers.f.set_subtract(false);
        self.registers
            .f
            .set_half_carry((a & 0x0F) + (value & 0x0F) + carry_in > 0x0F);
        self.registers
            .f
            .set_carry(u16::from(a) + u16::from(value) + u16::from(carry_in) > 0xFF);
    }

    fn alu_sub(&mut self, value: u8) {
        let a = self.registers.a;
        let result = a.wrapping_sub(value);

        self.registers.a = result;
        self.registers.f.set_zero(result == 0);
        self.registers.f.set_subtract(true);
        self.registers.f.set_half_carry((a & 0x0F) < (value & 0x0F));
        self.registers.f.set_carry(a < value);
    }

    fn alu_sbc(&mut self, value: u8) {
        let carry_in = u8::from(self.registers.f.carry());
        let a = self.registers.a;
        let result = a.wrapping_sub(value).wrapping_sub(carry_in);

        self.registers.a = result;
        self.registers.f.set_zero(result == 0);
        self.registers.f.set_subtract(true);
        self.registers
            .f
            .set_half_carry((a & 0x0F) < ((value & 0x0F) + carry_in));
        self.registers
            .f
            .set_carry(u16::from(a) < u16::from(value) + u16::from(carry_in));
    }

    fn alu_and(&mut self, value: u8) {
        self.registers.a &= value;
        self.registers.f.set_zero(self.registers.a == 0);
        self.registers.f.set_subtract(false);
        self.registers.f.set_half_carry(true);
        self.registers.f.set_carry(false);
    }

    fn alu_or(&mut self, value: u8) {
        self.registers.a |= value;
        self.registers.f.set_zero(self.registers.a == 0);
        self.registers.f.set_subtract(false);
        self.registers.f.set_half_carry(false);
        self.registers.f.set_carry(false);
    }

    fn alu_xor(&mut self, value: u8) {
        self.registers.a ^= value;
        self.registers.f.set_zero(self.registers.a == 0);
        self.registers.f.set_subtract(false);
        self.registers.f.set_half_carry(false);
        self.registers.f.set_carry(false);
    }

    fn alu_cp(&mut self, value: u8) {
        let a = self.registers.a;
        let result = a.wrapping_sub(value);

        self.registers.f.set_zero(result == 0);
        self.registers.f.set_subtract(true);
        self.registers.f.set_half_carry((a & 0x0F) < (value & 0x0F));
        self.registers.f.set_carry(a < value);
    }

    fn daa(&mut self) -> TCycles {
        let mut adjustment = 0;
        let mut carry = self.registers.f.carry();

        // DAA adjusts A after a previous BCD add/subtract. The N flag tells us
        // whether to apply the correction by adding or subtracting it.
        if self.registers.f.subtract() {
            if carry {
                adjustment |= 0x60;
            }
            if self.registers.f.half_carry() {
                adjustment |= 0x06;
            }

            self.registers.a = self.registers.a.wrapping_sub(adjustment);
        } else {
            if carry || self.registers.a > 0x99 {
                adjustment |= 0x60;
                carry = true;
            }
            if self.registers.f.half_carry() || self.registers.a & 0x0F > 0x09 {
                adjustment |= 0x06;
            }

            self.registers.a = self.registers.a.wrapping_add(adjustment);
        }

        self.registers.f.set_zero(self.registers.a == 0);
        self.registers.f.set_half_carry(false);
        self.registers.f.set_carry(carry);

        TCycles(4)
    }

    fn cpl(&mut self) -> TCycles {
        self.registers.a = !self.registers.a;
        self.registers.f.set_subtract(true);
        self.registers.f.set_half_carry(true);

        TCycles(4)
    }

    fn scf(&mut self) -> TCycles {
        self.registers.f.set_subtract(false);
        self.registers.f.set_half_carry(false);
        self.registers.f.set_carry(true);

        TCycles(4)
    }

    fn ccf(&mut self) -> TCycles {
        let carry = self.registers.f.carry();

        self.registers.f.set_subtract(false);
        self.registers.f.set_half_carry(false);
        self.registers.f.set_carry(!carry);

        TCycles(4)
    }

    fn inc_rr(&mut self, pair: RegisterPair) -> TCycles {
        let value = self.read_register_pair(pair).wrapping_add(1);
        self.write_register_pair(pair, value);

        TCycles(8)
    }

    fn dec_rr(&mut self, pair: RegisterPair) -> TCycles {
        let value = self.read_register_pair(pair).wrapping_sub(1);
        self.write_register_pair(pair, value);

        TCycles(8)
    }

    fn add_hl_rr(&mut self, pair: RegisterPair) -> TCycles {
        let hl = self.registers.hl();
        let value = self.read_register_pair(pair);
        let result = hl.wrapping_add(value);

        self.registers.set_hl(result);
        self.registers.f.set_subtract(false);
        self.registers
            .f
            .set_half_carry((hl & 0x0FFF) + (value & 0x0FFF) > 0x0FFF);
        self.registers
            .f
            .set_carry(u32::from(hl) + u32::from(value) > 0xFFFF);

        TCycles(8)
    }

    fn add_sp_e8(&mut self, bus: &Bus) -> TCycles {
        let offset = self.fetch8(bus);
        let result = self
            .registers
            .sp
            .wrapping_add_signed(i16::from(offset.cast_signed()));

        self.set_sp_offset_flags(self.registers.sp, offset);
        self.registers.sp = result;

        TCycles(16)
    }

    fn ld_hl_sp_e8(&mut self, bus: &Bus) -> TCycles {
        let offset = self.fetch8(bus);
        let result = self
            .registers
            .sp
            .wrapping_add_signed(i16::from(offset.cast_signed()));

        self.set_sp_offset_flags(self.registers.sp, offset);
        self.registers.set_hl(result);

        TCycles(12)
    }

    fn ld_sp_hl(&mut self) -> TCycles {
        self.registers.sp = self.registers.hl();

        TCycles(8)
    }

    fn set_sp_offset_flags(&mut self, sp: u16, offset: u8) {
        self.registers.f.set_zero(false);
        self.registers.f.set_subtract(false);
        self.registers
            .f
            .set_half_carry((sp & 0x000F) + u16::from(offset & 0x0F) > 0x000F);
        self.registers
            .f
            .set_carry((sp & 0x00FF) + u16::from(offset) > 0x00FF);
    }

    fn jp_a16(&mut self, bus: &Bus) -> TCycles {
        self.registers.pc = self.fetch16(bus);

        TCycles(16)
    }

    fn jp_hl(&mut self) -> TCycles {
        self.registers.pc = self.registers.hl();

        TCycles(4)
    }

    fn jp_cc_a16(&mut self, condition: Condition, bus: &Bus) -> TCycles {
        let address = self.fetch16(bus);

        if self.condition_is_met(condition) {
            self.registers.pc = address;
            TCycles(16)
        } else {
            TCycles(12)
        }
    }

    fn jr_e8(&mut self, bus: &Bus) -> TCycles {
        let offset = self.fetch8(bus);
        self.relative_jump(offset);

        TCycles(12)
    }

    fn jr_cc_e8(&mut self, condition: Condition, bus: &Bus) -> TCycles {
        let offset = self.fetch8(bus);

        if self.condition_is_met(condition) {
            self.relative_jump(offset);
            TCycles(12)
        } else {
            TCycles(8)
        }
    }

    fn relative_jump(&mut self, offset: u8) {
        self.registers.pc = self
            .registers
            .pc
            .wrapping_add_signed(i16::from(offset.cast_signed()));
    }

    fn call_a16(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.fetch16(bus);
        self.push16(bus, self.registers.pc);
        self.registers.pc = address;

        TCycles(24)
    }

    fn call_cc_a16(&mut self, condition: Condition, bus: &mut Bus) -> TCycles {
        let address = self.fetch16(bus);

        if self.condition_is_met(condition) {
            self.push16(bus, self.registers.pc);
            self.registers.pc = address;
            TCycles(24)
        } else {
            TCycles(12)
        }
    }

    fn ret(&mut self, bus: &Bus) -> TCycles {
        self.registers.pc = self.pop16(bus);

        TCycles(16)
    }

    fn ret_cc(&mut self, condition: Condition, bus: &Bus) -> TCycles {
        if self.condition_is_met(condition) {
            self.registers.pc = self.pop16(bus);
            TCycles(20)
        } else {
            TCycles(8)
        }
    }

    fn rst(&mut self, vector: u16, bus: &mut Bus) -> TCycles {
        self.push16(bus, self.registers.pc);
        self.registers.pc = vector;

        TCycles(16)
    }

    fn push16(&mut self, bus: &mut Bus, value: u16) {
        self.registers.sp = self.registers.sp.wrapping_sub(2);
        bus.write16(self.registers.sp, value);
    }

    fn pop16(&mut self, bus: &Bus) -> u16 {
        let value = bus.read16(self.registers.sp);
        self.registers.sp = self.registers.sp.wrapping_add(2);

        value
    }

    fn condition_is_met(&self, condition: Condition) -> bool {
        match condition {
            Condition::NotZero => !self.registers.f.zero(),
            Condition::Zero => self.registers.f.zero(),
            Condition::NotCarry => !self.registers.f.carry(),
            Condition::Carry => self.registers.f.carry(),
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

    #[allow(clippy::fn_params_excessive_bools)]
    fn assert_flags(cpu: &Cpu, zero: bool, subtract: bool, half_carry: bool, carry: bool) {
        assert_eq!(cpu.registers().f.zero(), zero, "Z flag mismatch");
        assert_eq!(cpu.registers().f.subtract(), subtract, "N flag mismatch");
        assert_eq!(
            cpu.registers().f.half_carry(),
            half_carry,
            "H flag mismatch"
        );
        assert_eq!(cpu.registers().f.carry(), carry, "C flag mismatch");
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

    #[test]
    fn inc_r_updates_zero_subtract_and_half_carry_flags_but_preserves_carry() {
        let cases = [
            (0x04, Register8::B, 0x00, 0x01, false, false),
            (0x0C, Register8::C, 0x0F, 0x10, false, true),
            (0x14, Register8::D, 0xFF, 0x00, true, true),
            (0x1C, Register8::E, 0x10, 0x11, false, false),
            (0x24, Register8::H, 0xFF, 0x00, true, true),
            (0x2C, Register8::L, 0x0F, 0x10, false, true),
            (0x3C, Register8::A, 0x00, 0x01, false, false),
        ];

        for (opcode, register, initial, expected, zero, half_carry) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.write_register8(register, initial);
            cpu.registers.f.set_carry(true);

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(4)), "INC r should take 4 T-cycles");
            assert_eq!(
                read_register(&cpu, register),
                expected,
                "opcode {opcode:02X} should increment {register:?}"
            );
            assert_flags(&cpu, zero, false, half_carry, true);
        }
    }

    #[test]
    fn dec_r_updates_zero_subtract_and_half_carry_flags_but_preserves_carry() {
        let cases = [
            (0x05, Register8::B, 0x00, 0xFF, false, true),
            (0x0D, Register8::C, 0x10, 0x0F, false, true),
            (0x15, Register8::D, 0x01, 0x00, true, false),
            (0x1D, Register8::E, 0xFF, 0xFE, false, false),
            (0x25, Register8::H, 0x10, 0x0F, false, true),
            (0x2D, Register8::L, 0x01, 0x00, true, false),
            (0x3D, Register8::A, 0x00, 0xFF, false, true),
        ];

        for (opcode, register, initial, expected, zero, half_carry) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.write_register8(register, initial);
            cpu.registers.f.set_carry(true);

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(4)), "DEC r should take 4 T-cycles");
            assert_eq!(
                read_register(&cpu, register),
                expected,
                "opcode {opcode:02X} should decrement {register:?}"
            );
            assert_flags(&cpu, zero, true, half_carry, true);
        }
    }

    #[test]
    fn add_a_r_sets_result_and_arithmetic_flags() {
        let cases = [
            (0x80, Register8::B, 0x12, 0x23, 0x35, false, false, false),
            (0x81, Register8::C, 0x0F, 0x01, 0x10, false, true, false),
            (0x82, Register8::D, 0xFF, 0x01, 0x00, true, true, true),
            (0x87, Register8::A, 0x80, 0x80, 0x00, true, false, true),
        ];

        for (opcode, register, a, value, expected, zero, half_carry, carry) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.a = a;
            if register != Register8::A {
                cpu.write_register8(register, value);
            }

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(4)), "ADD A,r should take 4 T-cycles");
            assert_eq!(cpu.registers().a, expected, "opcode {opcode:02X} result");
            assert_flags(&cpu, zero, false, half_carry, carry);
        }
    }

    #[test]
    fn adc_sub_and_sbc_use_carry_as_input_and_set_borrow_flags() {
        let cases = [
            (
                0x88,
                Register8::B,
                0x0F,
                0x00,
                true,
                0x10,
                false,
                false,
                true,
                false,
            ),
            (
                0x89,
                Register8::C,
                0xFF,
                0x00,
                true,
                0x00,
                true,
                false,
                true,
                true,
            ),
            (
                0x90,
                Register8::B,
                0x10,
                0x01,
                false,
                0x0F,
                false,
                true,
                true,
                false,
            ),
            (
                0x91,
                Register8::C,
                0x00,
                0x01,
                false,
                0xFF,
                false,
                true,
                true,
                true,
            ),
            (
                0x98,
                Register8::B,
                0x10,
                0x0F,
                true,
                0x00,
                true,
                true,
                true,
                false,
            ),
            (
                0x99,
                Register8::C,
                0x00,
                0x00,
                true,
                0xFF,
                false,
                true,
                true,
                true,
            ),
        ];

        for (opcode, register, a, value, carry_in, expected, zero, subtract, half_carry, carry) in
            cases
        {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.a = a;
            cpu.write_register8(register, value);
            cpu.registers.f.set_carry(carry_in);

            let cycles = cpu.step(&mut bus);

            assert_eq!(
                cycles,
                Ok(TCycles(4)),
                "opcode {opcode:02X} should take 4 T-cycles"
            );
            assert_eq!(cpu.registers().a, expected, "opcode {opcode:02X} result");
            assert_flags(&cpu, zero, subtract, half_carry, carry);
        }
    }

    #[test]
    fn logical_operations_set_expected_flags_and_cp_preserves_a() {
        let cases = [
            (
                0xA0,
                Register8::B,
                0xF0,
                0x0F,
                0x00,
                true,
                false,
                true,
                false,
            ),
            (
                0xA1,
                Register8::C,
                0xF0,
                0x0F,
                0x00,
                true,
                false,
                true,
                false,
            ),
            (
                0xA8,
                Register8::B,
                0xF0,
                0x0F,
                0xFF,
                false,
                false,
                false,
                false,
            ),
            (
                0xB0,
                Register8::B,
                0x00,
                0x00,
                0x00,
                true,
                false,
                false,
                false,
            ),
            (
                0xB1,
                Register8::C,
                0x80,
                0x01,
                0x81,
                false,
                false,
                false,
                false,
            ),
        ];

        for (opcode, register, a, value, expected, zero, subtract, half_carry, carry) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.a = a;
            cpu.write_register8(register, value);

            let cycles = cpu.step(&mut bus);

            assert_eq!(
                cycles,
                Ok(TCycles(4)),
                "opcode {opcode:02X} should take 4 T-cycles"
            );
            assert_eq!(cpu.registers().a, expected, "opcode {opcode:02X} result");
            assert_flags(&cpu, zero, subtract, half_carry, carry);
        }

        let mut bus = bus_with_bytes(&[(0x0100, 0xB8)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.a = 0x10;
        cpu.registers.b = 0x11;

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(4)), "CP A,B should take 4 T-cycles");
        assert_eq!(cpu.registers().a, 0x10, "CP should not modify A");
        assert_flags(&cpu, false, true, true, true);
    }

    #[test]
    fn immediate_alu_variants_reuse_the_same_flag_behaviour() {
        let cases = [
            (0xC6, 0x0F, 0x01, false, 0x10, false, false, true, false),
            (0xCE, 0xFF, 0x00, true, 0x00, true, false, true, true),
            (0xD6, 0x10, 0x01, false, 0x0F, false, true, true, false),
            (0xDE, 0x00, 0x00, true, 0xFF, false, true, true, true),
            (0xE6, 0xF0, 0x0F, false, 0x00, true, false, true, false),
            (0xEE, 0xF0, 0x0F, false, 0xFF, false, false, false, false),
            (0xF6, 0x00, 0x00, false, 0x00, true, false, false, false),
        ];

        for (opcode, a, value, carry_in, expected, zero, subtract, half_carry, carry) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode), (0x0101, value)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.a = a;
            cpu.registers.f.set_carry(carry_in);

            let cycles = cpu.step(&mut bus);

            assert_eq!(
                cycles,
                Ok(TCycles(8)),
                "opcode {opcode:02X} should take 8 T-cycles"
            );
            assert_eq!(cpu.registers().a, expected, "opcode {opcode:02X} result");
            assert_flags(&cpu, zero, subtract, half_carry, carry);
            assert_eq!(
                cpu.registers().pc,
                0x0102,
                "immediate ALU instructions should consume two bytes"
            );
        }

        let mut bus = bus_with_bytes(&[(0x0100, 0xFE), (0x0101, 0x42)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.a = 0x42;

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(8)), "CP d8 should take 8 T-cycles");
        assert_eq!(cpu.registers().a, 0x42, "CP d8 should not modify A");
        assert_flags(&cpu, true, true, false, false);
    }

    #[test]
    fn daa_adjusts_after_bcd_add_and_subtract_cases() {
        let cases = [
            (0x09, false, false, false, 0x09, false, false),
            (0x0A, false, false, false, 0x10, false, false),
            (0x32, false, true, false, 0x38, false, false),
            (0x9A, false, false, false, 0x00, true, true),
            (0x15, true, true, false, 0x0F, false, false),
            (0x7D, true, false, true, 0x1D, false, true),
        ];

        for (a, subtract, half_carry, carry, expected, zero, expected_carry) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, 0x27)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.a = a;
            cpu.registers.f.set_subtract(subtract);
            cpu.registers.f.set_half_carry(half_carry);
            cpu.registers.f.set_carry(carry);

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(4)), "DAA should take 4 T-cycles");
            assert_eq!(cpu.registers().a, expected, "DAA adjusted value");
            assert_flags(&cpu, zero, subtract, false, expected_carry);
        }
    }

    #[test]
    fn cpl_scf_and_ccf_update_only_their_documented_flags() {
        let mut bus = bus_with_bytes(&[(0x0100, 0x2F)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.a = 0x35;
        cpu.registers.f.set_zero(true);
        cpu.registers.f.set_carry(true);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(4)), "CPL should take 4 T-cycles");
        assert_eq!(cpu.registers().a, 0xCA, "CPL should complement A");
        assert_flags(&cpu, true, true, true, true);

        let mut bus = bus_with_bytes(&[(0x0100, 0x37)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.f.set_zero(true);
        cpu.registers.f.set_subtract(true);
        cpu.registers.f.set_half_carry(true);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(4)), "SCF should take 4 T-cycles");
        assert_flags(&cpu, true, false, false, true);

        let mut bus = bus_with_bytes(&[(0x0100, 0x3F)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.f.set_zero(true);
        cpu.registers.f.set_carry(true);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(4)), "CCF should take 4 T-cycles");
        assert_flags(&cpu, true, false, false, false);
    }

    #[test]
    fn inc_and_dec_16_bit_registers_wrap_without_changing_flags() {
        let cases = [
            (0x03, "BC", 0xFFFF, 0x0000),
            (0x13, "DE", 0x00FF, 0x0100),
            (0x23, "HL", 0x1234, 0x1235),
            (0x33, "SP", 0xFFFF, 0x0000),
            (0x0B, "BC", 0x0000, 0xFFFF),
            (0x1B, "DE", 0x0100, 0x00FF),
            (0x2B, "HL", 0x1234, 0x1233),
            (0x3B, "SP", 0x0000, 0xFFFF),
        ];

        for (opcode, pair_name, initial, expected) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.f.set_raw(0xF0);
            match pair_name {
                "BC" => cpu.registers.set_bc(initial),
                "DE" => cpu.registers.set_de(initial),
                "HL" => cpu.registers.set_hl(initial),
                "SP" => cpu.registers.sp = initial,
                _ => unreachable!("test case uses known register pairs"),
            }

            let cycles = cpu.step(&mut bus);
            let actual = match pair_name {
                "BC" => cpu.registers().bc(),
                "DE" => cpu.registers().de(),
                "HL" => cpu.registers().hl(),
                "SP" => cpu.registers().sp,
                _ => unreachable!("test case uses known register pairs"),
            };

            assert_eq!(
                cycles,
                Ok(TCycles(8)),
                "opcode {opcode:02X} should take 8 T-cycles"
            );
            assert_eq!(
                actual, expected,
                "opcode {opcode:02X} should update {pair_name}"
            );
            assert_eq!(
                cpu.registers().f.raw(),
                0xF0,
                "16-bit INC/DEC should not change flags"
            );
        }
    }

    #[test]
    fn add_hl_rr_sets_16_bit_half_carry_and_carry_but_preserves_zero() {
        let cases = [
            (0x09, "BC", 0x1234, 0x0001, 0x1235, false, false),
            (0x19, "DE", 0x0FFF, 0x0001, 0x1000, true, false),
            (0x29, "HL", 0x8000, 0x8000, 0x0000, false, true),
            (0x39, "SP", 0xFFFF, 0x0001, 0x0000, true, true),
        ];

        for (opcode, pair_name, hl, value, expected, half_carry, carry) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.set_hl(hl);
            cpu.registers.f.set_zero(true);
            cpu.registers.f.set_subtract(true);
            match pair_name {
                "BC" => cpu.registers.set_bc(value),
                "DE" => cpu.registers.set_de(value),
                "HL" => {}
                "SP" => cpu.registers.sp = value,
                _ => unreachable!("test case uses known register pairs"),
            }

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(8)), "ADD HL,rr should take 8 T-cycles");
            assert_eq!(
                cpu.registers().hl(),
                expected,
                "opcode {opcode:02X} HL result"
            );
            assert_flags(&cpu, true, false, half_carry, carry);
        }
    }

    #[test]
    fn sp_signed_offset_arithmetic_handles_positive_and_negative_offsets() {
        let cases = [
            (0xE8, 0xFFF8, 0x08, 0x0000, "SP", true, true, TCycles(16)),
            (0xE8, 0x0008, 0xF8, 0x0000, "SP", true, true, TCycles(16)),
            (0xF8, 0x1234, 0x05, 0x1239, "HL", false, false, TCycles(12)),
            (0xF8, 0x0100, 0xF0, 0x00F0, "HL", false, false, TCycles(12)),
        ];

        for (opcode, sp, offset, expected, destination, half_carry, carry, expected_cycles) in cases
        {
            let mut bus = bus_with_bytes(&[(0x0100, opcode), (0x0101, offset)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.sp = sp;
            cpu.registers.f.set_raw(0xF0);

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(expected_cycles), "opcode {opcode:02X} cycles");
            match destination {
                "SP" => assert_eq!(cpu.registers().sp, expected, "ADD SP,e8 result"),
                "HL" => assert_eq!(cpu.registers().hl(), expected, "LD HL,SP+e8 result"),
                _ => unreachable!("test case uses known destinations"),
            }
            assert!(
                !cpu.registers().f.zero(),
                "opcode {opcode:02X} offset {offset:02X} should clear Z"
            );
            assert!(
                !cpu.registers().f.subtract(),
                "opcode {opcode:02X} offset {offset:02X} should clear N"
            );
            assert_eq!(
                cpu.registers().f.half_carry(),
                half_carry,
                "opcode {opcode:02X} offset {offset:02X} H flag"
            );
            assert_eq!(
                cpu.registers().f.carry(),
                carry,
                "opcode {opcode:02X} offset {offset:02X} C flag"
            );
        }
    }

    #[test]
    fn ld_sp_hl_copies_hl_to_sp_without_changing_flags() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xF9)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.set_hl(0xC123);
        cpu.registers.f.set_raw(0xF0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(8)), "LD SP,HL should take 8 T-cycles");
        assert_eq!(cpu.registers().sp, 0xC123, "SP should receive HL");
        assert_eq!(
            cpu.registers().f.raw(),
            0xF0,
            "LD SP,HL should not change flags"
        );
    }

    #[test]
    fn jp_a16_and_jp_hl_set_pc_to_absolute_targets() {
        let (cpu, _bus, cycles) = run_instruction(&[0xC3, 0x34, 0xC1]);

        assert_eq!(cycles, TCycles(16), "JP a16 should take 16 T-cycles");
        assert_eq!(cpu.registers().pc, 0xC134, "JP a16 should load PC from d16");

        let mut bus = bus_with_bytes(&[(0x0100, 0xE9)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.set_hl(0xC456);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(4)), "JP HL should take 4 T-cycles");
        assert_eq!(cpu.registers().pc, 0xC456, "JP HL should copy HL to PC");
    }

    #[test]
    fn jr_e8_jumps_relative_to_pc_after_operand_fetch() {
        let (cpu, _bus, cycles) = run_instruction(&[0x18, 0x05]);

        assert_eq!(cycles, TCycles(12), "JR e8 should take 12 T-cycles");
        assert_eq!(
            cpu.registers().pc,
            0x0107,
            "positive JR offset should be relative to PC after operand"
        );

        let (cpu, _bus, cycles) = run_instruction(&[0x18, 0xFE]);

        assert_eq!(cycles, TCycles(12), "JR e8 should take 12 T-cycles");
        assert_eq!(
            cpu.registers().pc,
            0x0100,
            "negative JR offset should wrap from PC after operand"
        );
    }

    #[test]
    fn conditional_jp_uses_condition_and_taken_cycle_count() {
        let cases = [
            (
                0xC2,
                false,
                false,
                true,
                TCycles(16),
                0xC000,
                "JP NZ,a16 taken",
            ),
            (
                0xC2,
                true,
                false,
                false,
                TCycles(12),
                0x0103,
                "JP NZ,a16 not taken",
            ),
            (
                0xCA,
                true,
                false,
                true,
                TCycles(16),
                0xC000,
                "JP Z,a16 taken",
            ),
            (
                0xD2,
                false,
                false,
                true,
                TCycles(16),
                0xC000,
                "JP NC,a16 taken",
            ),
            (
                0xDA,
                false,
                true,
                true,
                TCycles(16),
                0xC000,
                "JP C,a16 taken",
            ),
            (
                0xDA,
                false,
                false,
                false,
                TCycles(12),
                0x0103,
                "JP C,a16 not taken",
            ),
        ];

        for (opcode, zero, carry, taken, expected_cycles, expected_pc, name) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode), (0x0101, 0x00), (0x0102, 0xC0)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.f.set_zero(zero);
            cpu.registers.f.set_carry(carry);

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(expected_cycles), "{name} cycles");
            assert_eq!(cpu.registers().pc, expected_pc, "{name} PC");
            assert_eq!(
                taken,
                expected_pc == 0xC000,
                "{name} test case should describe branch direction"
            );
        }
    }

    #[test]
    fn conditional_jr_uses_condition_and_taken_cycle_count() {
        let cases = [
            (0x20, false, false, TCycles(12), 0x0104, "JR NZ,e8 taken"),
            (0x20, true, false, TCycles(8), 0x0102, "JR NZ,e8 not taken"),
            (0x28, true, false, TCycles(12), 0x0104, "JR Z,e8 taken"),
            (0x30, false, false, TCycles(12), 0x0104, "JR NC,e8 taken"),
            (0x38, false, true, TCycles(12), 0x0104, "JR C,e8 taken"),
            (0x38, false, false, TCycles(8), 0x0102, "JR C,e8 not taken"),
        ];

        for (opcode, zero, carry, expected_cycles, expected_pc, name) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode), (0x0101, 0x02)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.f.set_zero(zero);
            cpu.registers.f.set_carry(carry);

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(expected_cycles), "{name} cycles");
            assert_eq!(cpu.registers().pc, expected_pc, "{name} PC");
        }
    }

    #[test]
    fn push16_and_pop16_use_little_endian_stack_memory_and_restore_sp() {
        let mut bus = bus_with_bytes(&[]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.sp = 0xC100;

        cpu.push16(&mut bus, 0x1234);

        assert_eq!(
            cpu.registers().sp,
            0xC0FE,
            "push16 should decrement SP by two"
        );
        assert_eq!(
            bus.read8(0xC0FE),
            0x34,
            "push16 should store low byte first"
        );
        assert_eq!(
            bus.read8(0xC0FF),
            0x12,
            "push16 should store high byte second"
        );

        let value = cpu.pop16(&bus);

        assert_eq!(value, 0x1234, "pop16 should read the pushed value");
        assert_eq!(cpu.registers().sp, 0xC100, "pop16 should restore SP by two");
    }

    #[test]
    fn call_pushes_return_address_and_ret_restores_it() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xCD), (0x0101, 0x00), (0x0102, 0xC2)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.sp = 0xC100;

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(24)), "CALL a16 should take 24 T-cycles");
        assert_eq!(cpu.registers().pc, 0xC200, "CALL should jump to a16");
        assert_eq!(
            cpu.registers().sp,
            0xC0FE,
            "CALL should push onto the stack"
        );
        assert_eq!(
            bus.read16(cpu.registers().sp),
            0x0103,
            "CALL should push the address after its operand"
        );

        bus.write8(0xC200, 0xC9);
        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(16)), "RET should take 16 T-cycles");
        assert_eq!(cpu.registers().pc, 0x0103, "RET should restore pushed PC");
        assert_eq!(cpu.registers().sp, 0xC100, "RET should pop the stack");
    }

    #[test]
    fn conditional_call_and_ret_use_taken_and_not_taken_cycles() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xC4), (0x0101, 0x00), (0x0102, 0xC3)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.sp = 0xC100;
        cpu.registers.f.set_zero(false);

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(24)),
            "taken CALL cc,a16 should take 24 T-cycles"
        );
        assert_eq!(cpu.registers().pc, 0xC300, "taken CALL should jump");
        assert_eq!(
            bus.read16(cpu.registers().sp),
            0x0103,
            "taken CALL return address"
        );

        let mut bus = bus_with_bytes(&[(0x0100, 0xC4), (0x0101, 0x00), (0x0102, 0xC3)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.sp = 0xC100;
        cpu.registers.f.set_zero(true);

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(12)),
            "not-taken CALL cc,a16 should take 12 T-cycles"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0103,
            "not-taken CALL should only consume operand"
        );
        assert_eq!(cpu.registers().sp, 0xC100, "not-taken CALL should not push");

        let mut bus = bus_with_bytes(&[(0x0100, 0xC0)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.sp = 0xC100;
        bus.write16(0xC100, 0xC400);
        cpu.registers.f.set_zero(false);

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(20)),
            "taken RET cc should take 20 T-cycles"
        );
        assert_eq!(cpu.registers().pc, 0xC400, "taken RET should pop PC");
        assert_eq!(cpu.registers().sp, 0xC102, "taken RET should advance SP");

        let mut bus = bus_with_bytes(&[(0x0100, 0xC0)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.sp = 0xC100;
        bus.write16(0xC100, 0xC400);
        cpu.registers.f.set_zero(true);

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(8)),
            "not-taken RET cc should take 8 T-cycles"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0101,
            "not-taken RET should only consume opcode"
        );
        assert_eq!(cpu.registers().sp, 0xC100, "not-taken RET should not pop");
    }

    #[test]
    fn rst_pushes_return_address_and_jumps_to_each_vector() {
        let cases = [
            (0xC7, 0x00),
            (0xCF, 0x08),
            (0xD7, 0x10),
            (0xDF, 0x18),
            (0xE7, 0x20),
            (0xEF, 0x28),
            (0xF7, 0x30),
            (0xFF, 0x38),
        ];

        for (opcode, vector) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.sp = 0xC100;

            let cycles = cpu.step(&mut bus);

            assert_eq!(
                cycles,
                Ok(TCycles(16)),
                "RST {vector:02X} should take 16 T-cycles"
            );
            assert_eq!(
                cpu.registers().pc,
                vector,
                "opcode {opcode:02X} should jump to its reset vector"
            );
            assert_eq!(
                bus.read16(cpu.registers().sp),
                0x0101,
                "RST should push the address after the opcode"
            );
        }
    }
}
