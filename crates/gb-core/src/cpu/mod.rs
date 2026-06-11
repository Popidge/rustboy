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
enum CbOperand {
    Register(Register8),
    AddressHl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Condition {
    NotZero,
    Zero,
    NotCarry,
    Carry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CpuRunState {
    Running,
    Halted,
    Stopped,
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
    ime: bool,
    ime_enable_pending: bool,
    halt_bug_pending: bool,
    run_state: CpuRunState,
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
            ime: false,
            ime_enable_pending: false,
            halt_bug_pending: false,
            run_state: CpuRunState::Running,
        }
    }

    /// Returns the CPU register state.
    #[must_use]
    pub fn registers(&self) -> &CpuRegisters {
        &self.registers
    }

    /// Returns whether interrupt master enable is currently set.
    #[must_use]
    pub fn ime(&self) -> bool {
        self.ime
    }

    /// Returns whether the CPU is currently halted.
    #[must_use]
    pub fn halted(&self) -> bool {
        self.run_state == CpuRunState::Halted
    }

    /// Returns whether the CPU has entered the placeholder STOP state.
    #[must_use]
    pub fn stopped(&self) -> bool {
        self.run_state == CpuRunState::Stopped
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

    fn fetch_opcode(&mut self, bus: &mut Bus) -> u8 {
        let address = self.registers.pc;
        let value = bus.cpu_fetch8(address);
        self.registers.pc = self.registers.pc.wrapping_add(1);
        value
    }

    fn fetch_operand8(&mut self, bus: &mut Bus) -> u8 {
        let address = self.registers.pc;
        let value = bus.cpu_read8(address);
        self.registers.pc = self.registers.pc.wrapping_add(1);
        value
    }

    fn fetch_operand16(&mut self, bus: &mut Bus) -> u16 {
        let low = self.fetch_operand8(bus);
        let high = self.fetch_operand8(bus);

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
        if let Some(cycles) = self.service_interrupt_or_halt(bus) {
            return Ok(cycles);
        }

        let enable_ime_after_step = self.ime_enable_pending;
        self.ime_enable_pending = false;
        let pc = self.registers.pc;
        let opcode = if self.halt_bug_pending {
            self.halt_bug_pending = false;
            bus.cpu_fetch8(pc)
        } else {
            self.fetch_opcode(bus)
        };

        let result = match opcode {
            0x00 => Ok(TCycles(4)),
            0x01 => Ok(self.ld_rr_d16(RegisterPair::BC, bus)),
            0x02 => Ok(self.ld_addr_rr_a(RegisterPair::BC, bus)),
            0x03 => Ok(self.inc_rr(RegisterPair::BC)),
            0x04 => Ok(self.inc_r(Register8::B)),
            0x05 => Ok(self.dec_r(Register8::B)),
            0x06 => Ok(self.ld_r_d8(Register8::B, bus)),
            0x07 => Ok(self.rlca()),
            0x08 => Ok(self.ld_addr_a16_sp(bus)),
            0x09 => Ok(self.add_hl_rr(RegisterPair::BC)),
            0x0A => Ok(self.ld_a_addr_rr(RegisterPair::BC, bus)),
            0x0B => Ok(self.dec_rr(RegisterPair::BC)),
            0x0C => Ok(self.inc_r(Register8::C)),
            0x0D => Ok(self.dec_r(Register8::C)),
            0x0E => Ok(self.ld_r_d8(Register8::C, bus)),
            0x0F => Ok(self.rrca()),
            0x10 => Ok(self.stop(bus)),
            0x11 => Ok(self.ld_rr_d16(RegisterPair::DE, bus)),
            0x12 => Ok(self.ld_addr_rr_a(RegisterPair::DE, bus)),
            0x13 => Ok(self.inc_rr(RegisterPair::DE)),
            0x14 => Ok(self.inc_r(Register8::D)),
            0x15 => Ok(self.dec_r(Register8::D)),
            0x16 => Ok(self.ld_r_d8(Register8::D, bus)),
            0x17 => Ok(self.rla()),
            0x19 => Ok(self.add_hl_rr(RegisterPair::DE)),
            0x1A => Ok(self.ld_a_addr_rr(RegisterPair::DE, bus)),
            0x1B => Ok(self.dec_rr(RegisterPair::DE)),
            0x1C => Ok(self.inc_r(Register8::E)),
            0x1D => Ok(self.dec_r(Register8::E)),
            0x1E => Ok(self.ld_r_d8(Register8::E, bus)),
            0x1F => Ok(self.rra()),
            0x18 => Ok(self.jr_e8(bus)),
            0x20 => Ok(self.jr_cc_e8(Condition::NotZero, bus)),
            0x21 => Ok(self.ld_rr_d16(RegisterPair::HL, bus)),
            0x22 => Ok(self.ld_addr_hl_inc_a(bus)),
            0x23 => Ok(self.inc_rr(RegisterPair::HL)),
            0x24 => Ok(self.inc_r(Register8::H)),
            0x25 => Ok(self.dec_r(Register8::H)),
            0x26 => Ok(self.ld_r_d8(Register8::H, bus)),
            0x27 => Ok(self.daa()),
            0x28 => Ok(self.jr_cc_e8(Condition::Zero, bus)),
            0x29 => Ok(self.add_hl_rr(RegisterPair::HL)),
            0x2A => Ok(self.ld_a_addr_hl_inc(bus)),
            0x2B => Ok(self.dec_rr(RegisterPair::HL)),
            0x2C => Ok(self.inc_r(Register8::L)),
            0x2D => Ok(self.dec_r(Register8::L)),
            0x2E => Ok(self.ld_r_d8(Register8::L, bus)),
            0x2F => Ok(self.cpl()),
            0x30 => Ok(self.jr_cc_e8(Condition::NotCarry, bus)),
            0x31 => Ok(self.ld_rr_d16(RegisterPair::SP, bus)),
            0x32 => Ok(self.ld_addr_hl_dec_a(bus)),
            0x33 => Ok(self.inc_rr(RegisterPair::SP)),
            0x34 => Ok(self.inc_addr_hl(bus)),
            0x35 => Ok(self.dec_addr_hl(bus)),
            0x36 => Ok(self.ld_addr_hl_d8(bus)),
            0x37 => Ok(self.scf()),
            0x38 => Ok(self.jr_cc_e8(Condition::Carry, bus)),
            0x39 => Ok(self.add_hl_rr(RegisterPair::SP)),
            0x3A => Ok(self.ld_a_addr_hl_dec(bus)),
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
            0x76 => Ok(self.halt(bus)),
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
            0x86 => Ok(self.add_a_addr_hl(bus)),
            0x87 => Ok(self.add_a_r(Register8::A)),
            0x88 => Ok(self.adc_a_r(Register8::B)),
            0x89 => Ok(self.adc_a_r(Register8::C)),
            0x8A => Ok(self.adc_a_r(Register8::D)),
            0x8B => Ok(self.adc_a_r(Register8::E)),
            0x8C => Ok(self.adc_a_r(Register8::H)),
            0x8D => Ok(self.adc_a_r(Register8::L)),
            0x8E => Ok(self.adc_a_addr_hl(bus)),
            0x8F => Ok(self.adc_a_r(Register8::A)),
            0x90 => Ok(self.sub_a_r(Register8::B)),
            0x91 => Ok(self.sub_a_r(Register8::C)),
            0x92 => Ok(self.sub_a_r(Register8::D)),
            0x93 => Ok(self.sub_a_r(Register8::E)),
            0x94 => Ok(self.sub_a_r(Register8::H)),
            0x95 => Ok(self.sub_a_r(Register8::L)),
            0x96 => Ok(self.sub_a_addr_hl(bus)),
            0x97 => Ok(self.sub_a_r(Register8::A)),
            0x98 => Ok(self.sbc_a_r(Register8::B)),
            0x99 => Ok(self.sbc_a_r(Register8::C)),
            0x9A => Ok(self.sbc_a_r(Register8::D)),
            0x9B => Ok(self.sbc_a_r(Register8::E)),
            0x9C => Ok(self.sbc_a_r(Register8::H)),
            0x9D => Ok(self.sbc_a_r(Register8::L)),
            0x9E => Ok(self.sbc_a_addr_hl(bus)),
            0x9F => Ok(self.sbc_a_r(Register8::A)),
            0xA0 => Ok(self.and_a_r(Register8::B)),
            0xA1 => Ok(self.and_a_r(Register8::C)),
            0xA2 => Ok(self.and_a_r(Register8::D)),
            0xA3 => Ok(self.and_a_r(Register8::E)),
            0xA4 => Ok(self.and_a_r(Register8::H)),
            0xA5 => Ok(self.and_a_r(Register8::L)),
            0xA6 => Ok(self.and_a_addr_hl(bus)),
            0xA7 => Ok(self.and_a_r(Register8::A)),
            0xA8 => Ok(self.xor_a_r(Register8::B)),
            0xA9 => Ok(self.xor_a_r(Register8::C)),
            0xAA => Ok(self.xor_a_r(Register8::D)),
            0xAB => Ok(self.xor_a_r(Register8::E)),
            0xAC => Ok(self.xor_a_r(Register8::H)),
            0xAD => Ok(self.xor_a_r(Register8::L)),
            0xAE => Ok(self.xor_a_addr_hl(bus)),
            0xAF => Ok(self.xor_a_r(Register8::A)),
            0xB0 => Ok(self.or_a_r(Register8::B)),
            0xB1 => Ok(self.or_a_r(Register8::C)),
            0xB2 => Ok(self.or_a_r(Register8::D)),
            0xB3 => Ok(self.or_a_r(Register8::E)),
            0xB4 => Ok(self.or_a_r(Register8::H)),
            0xB5 => Ok(self.or_a_r(Register8::L)),
            0xB6 => Ok(self.or_a_addr_hl(bus)),
            0xB7 => Ok(self.or_a_r(Register8::A)),
            0xB8 => Ok(self.cp_a_r(Register8::B)),
            0xB9 => Ok(self.cp_a_r(Register8::C)),
            0xBA => Ok(self.cp_a_r(Register8::D)),
            0xBB => Ok(self.cp_a_r(Register8::E)),
            0xBC => Ok(self.cp_a_r(Register8::H)),
            0xBD => Ok(self.cp_a_r(Register8::L)),
            0xBE => Ok(self.cp_a_addr_hl(bus)),
            0xBF => Ok(self.cp_a_r(Register8::A)),
            0xCB => Ok(self.step_cb(bus)),
            0xC0 => Ok(self.ret_cc(Condition::NotZero, bus)),
            0xC1 => Ok(self.pop_rr(StackRegisterPair::BC, bus)),
            0xC2 => Ok(self.jp_cc_a16(Condition::NotZero, bus)),
            0xC3 => Ok(self.jp_a16(bus)),
            0xC4 => Ok(self.call_cc_a16(Condition::NotZero, bus)),
            0xC5 => Ok(self.push_rr(StackRegisterPair::BC, bus)),
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
            0xD1 => Ok(self.pop_rr(StackRegisterPair::DE, bus)),
            0xD2 => Ok(self.jp_cc_a16(Condition::NotCarry, bus)),
            0xD4 => Ok(self.call_cc_a16(Condition::NotCarry, bus)),
            0xD5 => Ok(self.push_rr(StackRegisterPair::DE, bus)),
            0xD6 => Ok(self.sub_a_d8(bus)),
            0xD7 => Ok(self.rst(0x10, bus)),
            0xD8 => Ok(self.ret_cc(Condition::Carry, bus)),
            0xD9 => Ok(self.reti(bus)),
            0xDA => Ok(self.jp_cc_a16(Condition::Carry, bus)),
            0xDC => Ok(self.call_cc_a16(Condition::Carry, bus)),
            0xDE => Ok(self.sbc_a_d8(bus)),
            0xDF => Ok(self.rst(0x18, bus)),
            0xE0 => Ok(self.ldh_addr_a8_a(bus)),
            0xE1 => Ok(self.pop_rr(StackRegisterPair::HL, bus)),
            0xE2 => Ok(self.ldh_addr_c_a(bus)),
            0xE5 => Ok(self.push_rr(StackRegisterPair::HL, bus)),
            0xE6 => Ok(self.and_a_d8(bus)),
            0xE7 => Ok(self.rst(0x20, bus)),
            0xE8 => Ok(self.add_sp_e8(bus)),
            0xE9 => Ok(self.jp_hl()),
            0xEA => Ok(self.ld_addr_a16_a(bus)),
            0xEE => Ok(self.xor_a_d8(bus)),
            0xEF => Ok(self.rst(0x28, bus)),
            0xF0 => Ok(self.ldh_a_addr_a8(bus)),
            0xF1 => Ok(self.pop_rr(StackRegisterPair::AF, bus)),
            0xF2 => Ok(self.ldh_a_addr_c(bus)),
            0xF3 => Ok(self.di()),
            0xF5 => Ok(self.push_rr(StackRegisterPair::AF, bus)),
            0xF6 => Ok(self.or_a_d8(bus)),
            0xF7 => Ok(self.rst(0x30, bus)),
            0xF8 => Ok(self.ld_hl_sp_e8(bus)),
            0xF9 => Ok(self.ld_sp_hl(bus)),
            0xFA => Ok(self.ld_a_addr_a16(bus)),
            0xFB => Ok(self.ei()),
            0xFE => Ok(self.cp_a_d8(bus)),
            0xFF => Ok(self.rst(0x38, bus)),
            _ => Err(CpuError::UnimplementedOpcode { pc, opcode }),
        };

        if result.is_ok() && enable_ime_after_step {
            self.ime = true;
        }

        result
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

    fn service_interrupt_or_halt(&mut self, bus: &mut Bus) -> Option<TCycles> {
        let pending = bus.pending_interrupt();

        if self.run_state == CpuRunState::Halted && pending.is_none() {
            bus.cpu_idle_mcycle();
            return Some(TCycles(4));
        }

        if self.run_state == CpuRunState::Halted && pending.is_some() {
            self.run_state = CpuRunState::Running;
        }

        if self.ime {
            if let Some(interrupt) = pending {
                self.ime = false;
                self.ime_enable_pending = false;
                for _ in 0..3 {
                    bus.cpu_idle_mcycle();
                }
                bus.clear_interrupt(interrupt);
                self.push16(bus, self.registers.pc);
                self.registers.pc = interrupt.vector();

                return Some(TCycles(20));
            }
        }

        None
    }

    fn di(&mut self) -> TCycles {
        self.ime = false;
        self.ime_enable_pending = false;

        TCycles(4)
    }

    fn ei(&mut self) -> TCycles {
        self.ime_enable_pending = true;

        TCycles(4)
    }

    fn halt(&mut self, bus: &Bus) -> TCycles {
        if !self.ime && bus.pending_interrupt().is_some() {
            self.halt_bug_pending = true;
        } else {
            self.run_state = CpuRunState::Halted;
        }

        TCycles(4)
    }

    fn stop(&mut self, bus: &mut Bus) -> TCycles {
        let _padding = self.fetch_operand8(bus);
        self.run_state = CpuRunState::Stopped;

        TCycles(4)
    }

    fn cb_operand(opcode: u8) -> CbOperand {
        match opcode & 0x07 {
            0 => CbOperand::Register(Register8::B),
            1 => CbOperand::Register(Register8::C),
            2 => CbOperand::Register(Register8::D),
            3 => CbOperand::Register(Register8::E),
            4 => CbOperand::Register(Register8::H),
            5 => CbOperand::Register(Register8::L),
            6 => CbOperand::AddressHl,
            7 => CbOperand::Register(Register8::A),
            _ => unreachable!("three low bits produce values 0 through 7"),
        }
    }

    fn read_cb_operand(&self, operand: CbOperand, bus: &mut Bus) -> u8 {
        match operand {
            CbOperand::Register(register) => self.read_register8(register),
            CbOperand::AddressHl => bus.cpu_read8(self.registers.hl()),
        }
    }

    fn write_cb_operand(&mut self, operand: CbOperand, bus: &mut Bus, value: u8) {
        match operand {
            CbOperand::Register(register) => self.write_register8(register, value),
            CbOperand::AddressHl => bus.cpu_write8(self.registers.hl(), value),
        }
    }

    fn cb_cycles(operand: CbOperand, memory_cycles: u32, register_cycles: u32) -> TCycles {
        match operand {
            CbOperand::AddressHl => TCycles(memory_cycles),
            CbOperand::Register(_) => TCycles(register_cycles),
        }
    }

    fn step_cb(&mut self, bus: &mut Bus) -> TCycles {
        let opcode = self.fetch_operand8(bus);
        let operand = Self::cb_operand(opcode);

        match opcode {
            0x00..=0x07 => self.cb_update_operand(operand, bus, Self::rotate_left_circular),
            0x08..=0x0F => self.cb_update_operand(operand, bus, Self::rotate_right_circular),
            0x10..=0x17 => self.cb_update_operand(operand, bus, Self::rotate_left_through_carry),
            0x18..=0x1F => self.cb_update_operand(operand, bus, Self::rotate_right_through_carry),
            0x20..=0x27 => self.cb_update_operand(operand, bus, Self::shift_left_arithmetic),
            0x28..=0x2F => self.cb_update_operand(operand, bus, Self::shift_right_arithmetic),
            0x30..=0x37 => self.cb_update_operand(operand, bus, Self::swap_nibbles),
            0x38..=0x3F => self.cb_update_operand(operand, bus, Self::shift_right_logical),
            0x40..=0x7F => self.cb_bit(opcode, operand, bus),
            0x80..=0xBF => self.cb_res(opcode, operand, bus),
            0xC0..=0xFF => self.cb_set(opcode, operand, bus),
        }
    }

    fn rlca(&mut self) -> TCycles {
        let value = self.registers.a;
        let result = value.rotate_left(1);

        self.registers.a = result;
        self.set_rotate_flags(false, value & 0x80 != 0);

        TCycles(4)
    }

    fn rla(&mut self) -> TCycles {
        let value = self.registers.a;
        let carry_in = u8::from(self.registers.f.carry());
        let result = (value << 1) | carry_in;

        self.registers.a = result;
        self.set_rotate_flags(false, value & 0x80 != 0);

        TCycles(4)
    }

    fn rrca(&mut self) -> TCycles {
        let value = self.registers.a;
        let result = value.rotate_right(1);

        self.registers.a = result;
        self.set_rotate_flags(false, value & 0x01 != 0);

        TCycles(4)
    }

    fn rra(&mut self) -> TCycles {
        let value = self.registers.a;
        let carry_in = u8::from(self.registers.f.carry()) << 7;
        let result = (value >> 1) | carry_in;

        self.registers.a = result;
        self.set_rotate_flags(false, value & 0x01 != 0);

        TCycles(4)
    }

    fn cb_update_operand(
        &mut self,
        operand: CbOperand,
        bus: &mut Bus,
        operation: fn(&mut Self, u8) -> (u8, bool),
    ) -> TCycles {
        let value = self.read_cb_operand(operand, bus);
        let (result, carry) = operation(self, value);

        self.write_cb_operand(operand, bus, result);
        self.set_rotate_flags(result == 0, carry);

        Self::cb_cycles(operand, 16, 8)
    }

    #[allow(clippy::unused_self)]
    fn rotate_left_circular(&mut self, value: u8) -> (u8, bool) {
        (value.rotate_left(1), value & 0x80 != 0)
    }

    #[allow(clippy::unused_self)]
    fn rotate_right_circular(&mut self, value: u8) -> (u8, bool) {
        (value.rotate_right(1), value & 0x01 != 0)
    }

    fn rotate_left_through_carry(&mut self, value: u8) -> (u8, bool) {
        let carry_in = u8::from(self.registers.f.carry());

        ((value << 1) | carry_in, value & 0x80 != 0)
    }

    fn rotate_right_through_carry(&mut self, value: u8) -> (u8, bool) {
        let carry_in = u8::from(self.registers.f.carry()) << 7;

        ((value >> 1) | carry_in, value & 0x01 != 0)
    }

    #[allow(clippy::unused_self)]
    fn shift_left_arithmetic(&mut self, value: u8) -> (u8, bool) {
        (value << 1, value & 0x80 != 0)
    }

    #[allow(clippy::unused_self)]
    fn shift_right_arithmetic(&mut self, value: u8) -> (u8, bool) {
        ((value >> 1) | (value & 0x80), value & 0x01 != 0)
    }

    #[allow(clippy::unused_self)]
    fn shift_right_logical(&mut self, value: u8) -> (u8, bool) {
        (value >> 1, value & 0x01 != 0)
    }

    #[allow(clippy::unused_self)]
    fn swap_nibbles(&mut self, value: u8) -> (u8, bool) {
        (value.rotate_left(4), false)
    }

    fn set_rotate_flags(&mut self, zero: bool, carry: bool) {
        self.registers.f.set_zero(zero);
        self.registers.f.set_subtract(false);
        self.registers.f.set_half_carry(false);
        self.registers.f.set_carry(carry);
    }

    fn cb_bit(&mut self, opcode: u8, operand: CbOperand, bus: &mut Bus) -> TCycles {
        let bit = (opcode >> 3) & 0x07;
        let value = self.read_cb_operand(operand, bus);

        self.registers.f.set_zero(value & (1 << bit) == 0);
        self.registers.f.set_subtract(false);
        self.registers.f.set_half_carry(true);

        Self::cb_cycles(operand, 12, 8)
    }

    fn cb_res(&mut self, opcode: u8, operand: CbOperand, bus: &mut Bus) -> TCycles {
        let bit = (opcode >> 3) & 0x07;
        let value = self.read_cb_operand(operand, bus) & !(1 << bit);

        self.write_cb_operand(operand, bus, value);

        Self::cb_cycles(operand, 16, 8)
    }

    fn cb_set(&mut self, opcode: u8, operand: CbOperand, bus: &mut Bus) -> TCycles {
        let bit = (opcode >> 3) & 0x07;
        let value = self.read_cb_operand(operand, bus) | (1 << bit);

        self.write_cb_operand(operand, bus, value);

        Self::cb_cycles(operand, 16, 8)
    }

    fn ld_r_d8(&mut self, register: Register8, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand8(bus);
        self.write_register8(register, value);

        TCycles(8)
    }

    fn ld_r_r(&mut self, destination: Register8, source: Register8) -> TCycles {
        let value = self.read_register8(source);
        self.write_register8(destination, value);

        TCycles(4)
    }

    fn ld_r_addr_hl(&mut self, register: Register8, bus: &mut Bus) -> TCycles {
        let value = bus.cpu_read8(self.registers.hl());
        self.write_register8(register, value);

        TCycles(8)
    }

    fn ld_addr_hl_r(&mut self, register: Register8, bus: &mut Bus) -> TCycles {
        bus.cpu_write8(self.registers.hl(), self.read_register8(register));

        TCycles(8)
    }

    fn ld_a_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        self.registers.a = bus.cpu_read8(self.registers.hl());

        TCycles(8)
    }

    fn ld_addr_hl_a(&mut self, bus: &mut Bus) -> TCycles {
        bus.cpu_write8(self.registers.hl(), self.registers.a);

        TCycles(8)
    }

    fn ld_a_addr_hl_inc(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.registers.hl();
        self.registers.a = bus.cpu_read8(address);
        self.registers.set_hl(address.wrapping_add(1));

        TCycles(8)
    }

    fn ld_a_addr_hl_dec(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.registers.hl();
        self.registers.a = bus.cpu_read8(address);
        self.registers.set_hl(address.wrapping_sub(1));

        TCycles(8)
    }

    fn ld_addr_hl_inc_a(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.registers.hl();
        bus.cpu_write8(address, self.registers.a);
        self.registers.set_hl(address.wrapping_add(1));

        TCycles(8)
    }

    fn ld_addr_hl_dec_a(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.registers.hl();
        bus.cpu_write8(address, self.registers.a);
        self.registers.set_hl(address.wrapping_sub(1));

        TCycles(8)
    }

    fn ld_addr_hl_d8(&mut self, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand8(bus);
        bus.cpu_write8(self.registers.hl(), value);

        TCycles(12)
    }

    fn ld_a_addr_rr(&mut self, pair: RegisterPair, bus: &mut Bus) -> TCycles {
        self.registers.a = bus.cpu_read8(self.read_register_pair(pair));

        TCycles(8)
    }

    fn ld_addr_rr_a(&mut self, pair: RegisterPair, bus: &mut Bus) -> TCycles {
        bus.cpu_write8(self.read_register_pair(pair), self.registers.a);

        TCycles(8)
    }

    fn ldh_a_addr_a8(&mut self, bus: &mut Bus) -> TCycles {
        let offset = self.fetch_operand8(bus);
        self.registers.a = bus.cpu_read8(0xFF00 + u16::from(offset));

        TCycles(12)
    }

    fn ldh_addr_a8_a(&mut self, bus: &mut Bus) -> TCycles {
        let offset = self.fetch_operand8(bus);
        bus.cpu_write8(0xFF00 + u16::from(offset), self.registers.a);

        TCycles(12)
    }

    fn ldh_a_addr_c(&mut self, bus: &mut Bus) -> TCycles {
        self.registers.a = bus.cpu_read8(0xFF00 + u16::from(self.registers.c));

        TCycles(8)
    }

    fn ldh_addr_c_a(&mut self, bus: &mut Bus) -> TCycles {
        bus.cpu_write8(0xFF00 + u16::from(self.registers.c), self.registers.a);

        TCycles(8)
    }

    fn ld_a_addr_a16(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.fetch_operand16(bus);
        self.registers.a = bus.cpu_read8(address);

        TCycles(16)
    }

    fn ld_addr_a16_a(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.fetch_operand16(bus);
        bus.cpu_write8(address, self.registers.a);

        TCycles(16)
    }

    fn ld_addr_a16_sp(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.fetch_operand16(bus);
        let [low, high] = self.registers.sp.to_le_bytes();
        bus.cpu_write8(address, low);
        bus.cpu_write8(address.wrapping_add(1), high);

        TCycles(20)
    }

    fn ld_rr_d16(&mut self, pair: RegisterPair, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand16(bus);
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

    fn inc_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.registers.hl();
        let value = bus.cpu_read8(address);
        let result = value.wrapping_add(1);

        bus.cpu_write8(address, result);
        self.registers.f.set_zero(result == 0);
        self.registers.f.set_subtract(false);
        self.registers
            .f
            .set_half_carry((value & 0x0F).wrapping_add(1) > 0x0F);

        TCycles(12)
    }

    fn dec_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        let address = self.registers.hl();
        let value = bus.cpu_read8(address);
        let result = value.wrapping_sub(1);

        bus.cpu_write8(address, result);
        self.registers.f.set_zero(result == 0);
        self.registers.f.set_subtract(true);
        self.registers.f.set_half_carry(value.trailing_zeros() >= 4);

        TCycles(12)
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

    fn add_a_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        let value = bus.cpu_read8(self.registers.hl());
        self.alu_add(value);
        TCycles(8)
    }

    fn adc_a_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        let value = bus.cpu_read8(self.registers.hl());
        self.alu_adc(value);
        TCycles(8)
    }

    fn sub_a_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        let value = bus.cpu_read8(self.registers.hl());
        self.alu_sub(value);
        TCycles(8)
    }

    fn sbc_a_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        let value = bus.cpu_read8(self.registers.hl());
        self.alu_sbc(value);
        TCycles(8)
    }

    fn and_a_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        let value = bus.cpu_read8(self.registers.hl());
        self.alu_and(value);
        TCycles(8)
    }

    fn or_a_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        let value = bus.cpu_read8(self.registers.hl());
        self.alu_or(value);
        TCycles(8)
    }

    fn xor_a_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        let value = bus.cpu_read8(self.registers.hl());
        self.alu_xor(value);
        TCycles(8)
    }

    fn cp_a_addr_hl(&mut self, bus: &mut Bus) -> TCycles {
        let value = bus.cpu_read8(self.registers.hl());
        self.alu_cp(value);
        TCycles(8)
    }

    fn add_a_d8(&mut self, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand8(bus);
        self.alu_add(value);
        TCycles(8)
    }

    fn adc_a_d8(&mut self, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand8(bus);
        self.alu_adc(value);
        TCycles(8)
    }

    fn sub_a_d8(&mut self, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand8(bus);
        self.alu_sub(value);
        TCycles(8)
    }

    fn sbc_a_d8(&mut self, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand8(bus);
        self.alu_sbc(value);
        TCycles(8)
    }

    fn and_a_d8(&mut self, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand8(bus);
        self.alu_and(value);
        TCycles(8)
    }

    fn or_a_d8(&mut self, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand8(bus);
        self.alu_or(value);
        TCycles(8)
    }

    fn xor_a_d8(&mut self, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand8(bus);
        self.alu_xor(value);
        TCycles(8)
    }

    fn cp_a_d8(&mut self, bus: &mut Bus) -> TCycles {
        let value = self.fetch_operand8(bus);
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

    fn add_sp_e8(&mut self, bus: &mut Bus) -> TCycles {
        let offset = self.fetch_operand8(bus);
        let result = self
            .registers
            .sp
            .wrapping_add_signed(i16::from(offset.cast_signed()));

        self.set_sp_offset_flags(self.registers.sp, offset);
        self.registers.sp = result;
        bus.cpu_idle_mcycle();
        bus.cpu_idle_mcycle();

        TCycles(16)
    }

    fn ld_hl_sp_e8(&mut self, bus: &mut Bus) -> TCycles {
        let offset = self.fetch_operand8(bus);
        let result = self
            .registers
            .sp
            .wrapping_add_signed(i16::from(offset.cast_signed()));

        self.set_sp_offset_flags(self.registers.sp, offset);
        self.registers.set_hl(result);
        bus.cpu_idle_mcycle();

        TCycles(12)
    }

    fn ld_sp_hl(&mut self, bus: &mut Bus) -> TCycles {
        self.registers.sp = self.registers.hl();
        bus.cpu_idle_mcycle();

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

    fn jp_a16(&mut self, bus: &mut Bus) -> TCycles {
        self.registers.pc = self.fetch_operand16(bus);
        bus.cpu_idle_mcycle();

        TCycles(16)
    }

    fn jp_hl(&mut self) -> TCycles {
        self.registers.pc = self.registers.hl();

        TCycles(4)
    }

    fn jp_cc_a16(&mut self, condition: Condition, bus: &mut Bus) -> TCycles {
        let address = self.fetch_operand16(bus);

        if self.condition_is_met(condition) {
            self.registers.pc = address;
            bus.cpu_idle_mcycle();
            TCycles(16)
        } else {
            TCycles(12)
        }
    }

    fn jr_e8(&mut self, bus: &mut Bus) -> TCycles {
        let offset = self.fetch_operand8(bus);
        self.relative_jump(offset);
        bus.cpu_idle_mcycle();

        TCycles(12)
    }

    fn jr_cc_e8(&mut self, condition: Condition, bus: &mut Bus) -> TCycles {
        let offset = self.fetch_operand8(bus);

        if self.condition_is_met(condition) {
            self.relative_jump(offset);
            bus.cpu_idle_mcycle();
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
        let address = self.fetch_operand16(bus);
        bus.cpu_idle_mcycle();
        self.push16(bus, self.registers.pc);
        self.registers.pc = address;

        TCycles(24)
    }

    fn call_cc_a16(&mut self, condition: Condition, bus: &mut Bus) -> TCycles {
        let address = self.fetch_operand16(bus);

        if self.condition_is_met(condition) {
            bus.cpu_idle_mcycle();
            self.push16(bus, self.registers.pc);
            self.registers.pc = address;
            TCycles(24)
        } else {
            TCycles(12)
        }
    }

    fn ret(&mut self, bus: &mut Bus) -> TCycles {
        self.registers.pc = self.pop16(bus);
        bus.cpu_idle_mcycle();

        TCycles(16)
    }

    fn reti(&mut self, bus: &mut Bus) -> TCycles {
        self.registers.pc = self.pop16(bus);
        self.ime = true;
        bus.cpu_idle_mcycle();

        TCycles(16)
    }

    fn ret_cc(&mut self, condition: Condition, bus: &mut Bus) -> TCycles {
        bus.cpu_idle_mcycle();
        if self.condition_is_met(condition) {
            self.registers.pc = self.pop16(bus);
            bus.cpu_idle_mcycle();
            TCycles(20)
        } else {
            TCycles(8)
        }
    }

    fn rst(&mut self, vector: u16, bus: &mut Bus) -> TCycles {
        bus.cpu_idle_mcycle();
        self.push16(bus, self.registers.pc);
        self.registers.pc = vector;

        TCycles(16)
    }

    fn push_rr(&mut self, pair: StackRegisterPair, bus: &mut Bus) -> TCycles {
        bus.cpu_idle_mcycle();
        self.push16(bus, self.read_stack_register_pair(pair));

        TCycles(16)
    }

    fn pop_rr(&mut self, pair: StackRegisterPair, bus: &mut Bus) -> TCycles {
        let value = self.pop16(bus);
        self.write_stack_register_pair(pair, value);

        TCycles(12)
    }

    fn read_stack_register_pair(&self, pair: StackRegisterPair) -> u16 {
        match pair {
            StackRegisterPair::BC => self.registers.bc(),
            StackRegisterPair::DE => self.registers.de(),
            StackRegisterPair::HL => self.registers.hl(),
            StackRegisterPair::AF => self.registers.af(),
        }
    }

    fn write_stack_register_pair(&mut self, pair: StackRegisterPair, value: u16) {
        match pair {
            StackRegisterPair::BC => self.registers.set_bc(value),
            StackRegisterPair::DE => self.registers.set_de(value),
            StackRegisterPair::HL => self.registers.set_hl(value),
            StackRegisterPair::AF => self.registers.set_af(value),
        }
    }

    fn push16(&mut self, bus: &mut Bus, value: u16) {
        let [low, high] = value.to_le_bytes();
        self.registers.sp = self.registers.sp.wrapping_sub(1);
        bus.cpu_write8(self.registers.sp, high);
        self.registers.sp = self.registers.sp.wrapping_sub(1);
        bus.cpu_write8(self.registers.sp, low);
    }

    fn pop16(&mut self, bus: &mut Bus) -> u16 {
        let low = bus.cpu_read8(self.registers.sp);
        self.registers.sp = self.registers.sp.wrapping_add(1);
        let high = bus.cpu_read8(self.registers.sp);
        self.registers.sp = self.registers.sp.wrapping_add(1);

        u16::from_le_bytes([low, high])
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackRegisterPair {
    BC,
    DE,
    HL,
    AF,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new_dmg_post_boot()
    }
}

#[cfg(test)]
mod tests {
    use super::{Cpu, CpuError, Register8, TCycles};
    use crate::{bus::Bus, cartridge::Cartridge, interrupt::Interrupt};

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
    fn hl_auto_update_loads_read_or_write_a_and_update_hl() {
        let cases = [
            (0x22, 0xC100, 0xC101, 0x5A, 0x5A, "LD (HL+),A"),
            (0x2A, 0xC101, 0xC102, 0xA5, 0x3C, "LD A,(HL+)"),
            (0x32, 0xC102, 0xC101, 0x66, 0x66, "LD (HL-),A"),
            (0x3A, 0xC101, 0xC100, 0x77, 0x2D, "LD A,(HL-)"),
        ];

        for (opcode, initial_hl, expected_hl, initial_a, memory_value, name) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            bus.write8(initial_hl, memory_value);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.set_hl(initial_hl);
            cpu.registers.a = initial_a;

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(8)), "{name} should take 8 T-cycles");
            assert_eq!(
                cpu.registers().hl(),
                expected_hl,
                "{name} should update HL after using the original address"
            );
            if opcode == 0x22 || opcode == 0x32 {
                assert_eq!(
                    bus.read8(initial_hl),
                    initial_a,
                    "{name} should write A to the original HL address"
                );
            } else {
                assert_eq!(
                    cpu.registers().a,
                    memory_value,
                    "{name} should load A from the original HL address"
                );
            }
        }
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
    fn ldh_a_addr_c_and_ldh_addr_c_a_use_c_as_high_memory_offset() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xE2), (0x0101, 0xF2)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.a = 0xA7;
        cpu.registers.c = 0x80;

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(8)), "LD (C),A should take 8 T-cycles");
        assert_eq!(bus.read8(0xFF80), 0xA7, "0xFF00 + C should receive A");

        cpu.registers.a = 0x00;
        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(8)), "LD A,(C) should take 8 T-cycles");
        assert_eq!(cpu.registers().a, 0xA7, "A should read from 0xFF00 + C");
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
    fn ld_addr_a16_sp_writes_stack_pointer_little_endian() {
        let mut bus = bus_with_bytes(&[(0x0100, 0x08), (0x0101, 0x34), (0x0102, 0xC1)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.sp = 0xBEEF;

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(20)),
            "LD (a16),SP should take 20 T-cycles"
        );
        assert_eq!(
            bus.read8(0xC134),
            0xEF,
            "low byte of SP should be stored first"
        );
        assert_eq!(
            bus.read8(0xC135),
            0xBE,
            "high byte of SP should be stored second"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0103,
            "LD (a16),SP should consume three bytes"
        );
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
    fn inc_and_dec_addr_hl_update_memory_and_flags_but_preserve_carry() {
        let cases = [
            (0x34, 0x0F, 0x10, false, false, true, "INC (HL) half carry"),
            (0x34, 0xFF, 0x00, true, false, true, "INC (HL) zero"),
            (0x35, 0x10, 0x0F, false, true, true, "DEC (HL) half carry"),
            (0x35, 0x01, 0x00, true, true, false, "DEC (HL) zero"),
        ];

        for (opcode, initial, expected, zero, subtract, half_carry, name) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            bus.write8(0xC300, initial);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.set_hl(0xC300);
            cpu.registers.f.set_carry(true);

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(12)), "{name} should take 12 T-cycles");
            assert_eq!(bus.read8(0xC300), expected, "{name} memory result");
            assert_flags(&cpu, zero, subtract, half_carry, true);
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
    fn alu_operations_can_read_operand_from_hl_memory() {
        assert_alu_addr_hl_case((
            0x86,
            0x0F,
            0x01,
            false,
            0x10,
            false,
            false,
            true,
            false,
            "ADD A,(HL)",
        ));
        assert_alu_addr_hl_case((
            0x8E,
            0xFF,
            0x00,
            true,
            0x00,
            true,
            false,
            true,
            true,
            "ADC A,(HL)",
        ));
        assert_alu_addr_hl_case((
            0x96,
            0x10,
            0x01,
            false,
            0x0F,
            false,
            true,
            true,
            false,
            "SUB A,(HL)",
        ));
        assert_alu_addr_hl_case((
            0x9E,
            0x10,
            0x0F,
            true,
            0x00,
            true,
            true,
            true,
            false,
            "SBC A,(HL)",
        ));
        assert_alu_addr_hl_case((
            0xA6,
            0xF0,
            0x0F,
            false,
            0x00,
            true,
            false,
            true,
            false,
            "AND A,(HL)",
        ));
        assert_alu_addr_hl_case((
            0xAE,
            0xF0,
            0x0F,
            false,
            0xFF,
            false,
            false,
            false,
            false,
            "XOR A,(HL)",
        ));
        assert_alu_addr_hl_case((
            0xB6,
            0x80,
            0x01,
            false,
            0x81,
            false,
            false,
            false,
            false,
            "OR A,(HL)",
        ));
        assert_alu_addr_hl_case((
            0xBE,
            0x10,
            0x01,
            false,
            0x10,
            false,
            true,
            true,
            false,
            "CP A,(HL)",
        ));
    }

    type AluAddrHlCase = (u8, u8, u8, bool, u8, bool, bool, bool, bool, &'static str);

    fn assert_alu_addr_hl_case(case: AluAddrHlCase) {
        let (
            opcode,
            a,
            memory_value,
            carry_in,
            expected_a,
            zero,
            subtract,
            half_carry,
            carry,
            name,
        ) = case;
        let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
        bus.write8(0xC200, memory_value);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.set_hl(0xC200);
        cpu.registers.a = a;
        cpu.registers.f.set_carry(carry_in);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(8)), "{name} should take 8 T-cycles");
        assert_eq!(cpu.registers().a, expected_a, "{name} result");
        assert_flags(&cpu, zero, subtract, half_carry, carry);
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

        let value = cpu.pop16(&mut bus);

        assert_eq!(value, 0x1234, "pop16 should read the pushed value");
        assert_eq!(cpu.registers().sp, 0xC100, "pop16 should restore SP by two");
    }

    #[test]
    fn push_rr_stores_register_pair_on_stack() {
        let cases = [
            (0xC5, 0x1234, "PUSH BC"),
            (0xD5, 0x5678, "PUSH DE"),
            (0xE5, 0x9ABC, "PUSH HL"),
            (0xF5, 0xDEF0, "PUSH AF"),
        ];

        for (opcode, value, name) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.sp = 0xC100;
            cpu.registers.set_bc(0);
            cpu.registers.set_de(0);
            cpu.registers.set_hl(0);
            cpu.registers.set_af(0);
            match opcode {
                0xC5 => cpu.registers.set_bc(value),
                0xD5 => cpu.registers.set_de(value),
                0xE5 => cpu.registers.set_hl(value),
                0xF5 => cpu.registers.set_af(value),
                _ => unreachable!("test only uses PUSH opcodes"),
            }

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(16)), "{name} should take 16 T-cycles");
            assert_eq!(
                cpu.registers().sp,
                0xC0FE,
                "{name} should decrement SP by two"
            );
            assert_eq!(
                bus.read16(0xC0FE),
                value,
                "{name} should store the pair at SP"
            );
        }
    }

    #[test]
    fn pop_rr_loads_register_pair_from_stack() {
        let cases = [
            (0xC1, 0x1234, 0x1234, "POP BC"),
            (0xD1, 0x5678, 0x5678, "POP DE"),
            (0xE1, 0x9ABC, 0x9ABC, "POP HL"),
            (0xF1, 0xDEF7, 0xDEF0, "POP AF"),
        ];

        for (opcode, stack_value, expected_value, name) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            bus.write16(0xC100, stack_value);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.sp = 0xC100;
            cpu.registers.set_bc(0);
            cpu.registers.set_de(0);
            cpu.registers.set_hl(0);
            cpu.registers.set_af(0);

            let cycles = cpu.step(&mut bus);

            let actual_value = match opcode {
                0xC1 => cpu.registers().bc(),
                0xD1 => cpu.registers().de(),
                0xE1 => cpu.registers().hl(),
                0xF1 => cpu.registers().af(),
                _ => unreachable!("test only uses POP opcodes"),
            };

            assert_eq!(cycles, Ok(TCycles(12)), "{name} should take 12 T-cycles");
            assert_eq!(
                actual_value, expected_value,
                "{name} should load the register pair"
            );
            assert_eq!(
                cpu.registers().sp,
                0xC102,
                "{name} should increment SP by two"
            );
        }
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
    fn reti_pops_pc_and_enables_ime() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xD9)]);
        bus.write16(0xC100, 0xC456);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.sp = 0xC100;
        cpu.ime = false;

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(16)), "RETI should take 16 T-cycles");
        assert_eq!(cpu.registers().pc, 0xC456, "RETI should pop PC from stack");
        assert_eq!(cpu.registers().sp, 0xC102, "RETI should advance SP by two");
        assert!(cpu.ime(), "RETI should enable IME immediately");
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

    #[test]
    fn non_cb_rotates_update_a_and_carry_with_zero_always_clear() {
        let cases = [
            (
                0x07,
                0x80,
                false,
                0x01,
                true,
                "RLCA moves bit 7 into bit 0 and carry",
            ),
            (
                0x17,
                0x80,
                true,
                0x01,
                true,
                "RLA rotates through incoming carry",
            ),
            (
                0x0F,
                0x01,
                false,
                0x80,
                true,
                "RRCA moves bit 0 into bit 7 and carry",
            ),
            (
                0x1F,
                0x01,
                true,
                0x80,
                true,
                "RRA rotates through incoming carry",
            ),
            (
                0x07,
                0x00,
                false,
                0x00,
                false,
                "RLCA keeps Z clear even for zero result",
            ),
        ];

        for (opcode, value, carry_in, expected, carry, name) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.a = value;
            cpu.registers.f.set_raw(0xF0);
            cpu.registers.f.set_carry(carry_in);

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(4)), "{name} cycles");
            assert_eq!(cpu.registers().a, expected, "{name} result");
            assert_flags(&cpu, false, false, false, carry);
        }
    }

    #[test]
    fn cb_rotate_shift_and_swap_register_ops_update_values_and_flags() {
        let cases = [
            (0x00, Register8::B, 0x80, false, 0x01, false, true, "RLC B"),
            (0x09, Register8::C, 0x01, false, 0x80, false, true, "RRC C"),
            (0x12, Register8::D, 0x80, true, 0x01, false, true, "RL D"),
            (0x1B, Register8::E, 0x01, true, 0x80, false, true, "RR E"),
            (0x24, Register8::H, 0x81, false, 0x02, false, true, "SLA H"),
            (0x2D, Register8::L, 0x81, false, 0xC0, false, true, "SRA L"),
            (
                0x37,
                Register8::A,
                0xF0,
                false,
                0x0F,
                false,
                false,
                "SWAP A",
            ),
            (0x38, Register8::B, 0x01, false, 0x00, true, true, "SRL B"),
        ];

        for (cb_opcode, register, value, carry_in, expected, zero, carry, name) in cases {
            let mut bus = bus_with_bytes(&[(0x0100, 0xCB), (0x0101, cb_opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.write_register8(register, value);
            cpu.registers.f.set_carry(carry_in);

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(8)), "{name} cycles");
            assert_eq!(cpu.read_register8(register), expected, "{name} result");
            assert_flags(&cpu, zero, false, false, carry);
            assert_eq!(
                cpu.registers().pc,
                0x0102,
                "CB instruction should consume two bytes"
            );
        }
    }

    #[test]
    fn cb_bit_set_and_res_register_ops_cover_all_bit_positions() {
        for bit in 0..=7 {
            let bit_opcode = 0x40 | (bit << 3);
            let res_opcode = 0x80 | (bit << 3);
            let set_opcode = 0xC0 | (bit << 3);

            let mut bus = bus_with_bytes(&[(0x0100, 0xCB), (0x0101, bit_opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.b = 1 << bit;
            cpu.registers.f.set_carry(true);

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(8)), "BIT {bit},B cycles");
            assert_flags(&cpu, false, false, true, true);
            assert_eq!(
                cpu.registers().b,
                1 << bit,
                "BIT should not modify the operand"
            );

            let mut bus = bus_with_bytes(&[(0x0100, 0xCB), (0x0101, res_opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();
            cpu.registers.b = 0xFF;

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(8)), "RES {bit},B cycles");
            assert_eq!(cpu.registers().b, !(1 << bit), "RES should clear bit {bit}");

            let mut bus = bus_with_bytes(&[(0x0100, 0xCB), (0x0101, set_opcode)]);
            let mut cpu = Cpu::new_dmg_post_boot();

            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, Ok(TCycles(8)), "SET {bit},B cycles");
            assert_eq!(cpu.registers().b, 1 << bit, "SET should set bit {bit}");
        }
    }

    #[test]
    fn cb_ops_on_hl_use_memory_operand_and_longer_cycles() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xCB), (0x0101, 0x06)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.set_hl(0xC000);
        bus.write8(0xC000, 0x80);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(16)), "RLC (HL) should take 16 T-cycles");
        assert_eq!(
            bus.read8(0xC000),
            0x01,
            "RLC should write the memory result"
        );
        assert_flags(&cpu, false, false, false, true);

        let mut bus = bus_with_bytes(&[(0x0100, 0xCB), (0x0101, 0x7E)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.set_hl(0xC000);
        bus.write8(0xC000, 0x00);

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(12)),
            "BIT 7,(HL) should take 12 T-cycles"
        );
        assert_flags(&cpu, true, false, true, cpu.registers().f.carry());

        let mut bus = bus_with_bytes(&[(0x0100, 0xCB), (0x0101, 0xC6)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.set_hl(0xC000);

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(16)),
            "SET 0,(HL) should take 16 T-cycles"
        );
        assert_eq!(bus.read8(0xC000), 0x01, "SET should update memory");
    }

    #[test]
    fn di_and_ei_update_ime_with_ei_delayed_until_after_next_instruction() {
        let mut bus = bus_with_bytes(&[(0x0100, 0xFB), (0x0101, 0x00), (0x0102, 0xF3)]);
        let mut cpu = Cpu::new_dmg_post_boot();

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(4)), "EI should take 4 T-cycles");
        assert!(
            !cpu.ime(),
            "EI should not enable IME until one instruction later"
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(4)),
            "NOP after EI should take 4 T-cycles"
        );
        assert!(
            cpu.ime(),
            "IME should enable after the instruction following EI"
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(4)), "DI should take 4 T-cycles");
        assert!(!cpu.ime(), "DI should clear IME immediately");
    }

    #[test]
    fn interrupt_service_pushes_pc_clears_if_and_jumps_to_priority_vector() {
        let mut bus = bus_with_bytes(&[(0x0100, 0x00)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.sp = 0xC100;
        cpu.ime = true;
        bus.write8(0xFFFF, Interrupt::VBlank.mask() | Interrupt::Timer.mask());
        bus.request_interrupt(Interrupt::Timer);
        bus.request_interrupt(Interrupt::VBlank);

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(20)),
            "servicing an interrupt should take 20 T-cycles"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0040,
            "VBlank should be serviced before Timer"
        );
        assert_eq!(
            cpu.registers().sp,
            0xC0FE,
            "interrupt service should push PC"
        );
        assert_eq!(
            bus.read16(cpu.registers().sp),
            0x0100,
            "interrupt service should push the interrupted PC"
        );
        assert!(!cpu.ime(), "interrupt service should clear IME");
        assert_eq!(
            bus.interrupt_flags(),
            Interrupt::Timer.mask(),
            "serviced interrupt should be cleared from IF"
        );
    }

    #[test]
    fn halt_pauses_fetch_until_an_interrupt_is_pending_then_wakes() {
        let mut bus = bus_with_bytes(&[(0x0100, 0x76), (0x0101, 0x00)]);
        let mut cpu = Cpu::new_dmg_post_boot();

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, Ok(TCycles(4)), "HALT should take 4 T-cycles");
        assert!(cpu.halted(), "HALT should set the halted state");
        assert_eq!(cpu.registers().pc, 0x0101, "HALT should consume its opcode");

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(4)),
            "halted CPU should idle for 4 T-cycles"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0101,
            "halted CPU should not fetch while no interrupt is pending"
        );

        bus.write8(0xFFFF, Interrupt::Timer.mask());
        bus.request_interrupt(Interrupt::Timer);
        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(4)),
            "woken CPU should execute the next opcode"
        );
        assert!(!cpu.halted(), "pending interrupt should wake HALT");
        assert_eq!(
            cpu.registers().pc,
            0x0102,
            "woken CPU should fetch normally"
        );
    }

    #[test]
    fn halt_bug_repeats_next_opcode_fetch_when_ime_is_clear_and_interrupt_pending() {
        let mut bus = bus_with_bytes(&[(0x0100, 0x76), (0x0101, 0x3C)]);
        let mut cpu = Cpu::new_dmg_post_boot();
        cpu.registers.a = 0x10;
        bus.write8(0xFFFF, Interrupt::Timer.mask());
        bus.request_interrupt(Interrupt::Timer);

        let halt_cycles = cpu.step(&mut bus);

        assert_eq!(
            halt_cycles,
            Ok(TCycles(4)),
            "HALT should still consume 4 T-cycles"
        );
        assert!(
            !cpu.halted(),
            "HALT bug should prevent entering the halted state"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0101,
            "HALT should consume its own opcode"
        );

        let first_inc_cycles = cpu.step(&mut bus);

        assert_eq!(
            first_inc_cycles,
            Ok(TCycles(4)),
            "first repeated INC A should execute normally"
        );
        assert_eq!(cpu.registers().a, 0x11, "first repeated opcode execution");
        assert_eq!(
            cpu.registers().pc,
            0x0101,
            "HALT bug should suppress the next opcode fetch increment once"
        );

        let second_inc_cycles = cpu.step(&mut bus);

        assert_eq!(
            second_inc_cycles,
            Ok(TCycles(4)),
            "second INC A should execute from the same address"
        );
        assert_eq!(
            cpu.registers().a,
            0x12,
            "same opcode should be executed twice"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0102,
            "fetching should return to normal after the one repeated opcode"
        );
    }

    #[test]
    fn stop_sets_documented_placeholder_state_and_consumes_padding_byte() {
        let mut bus = bus_with_bytes(&[(0x0100, 0x10), (0x0101, 0x00)]);
        let mut cpu = Cpu::new_dmg_post_boot();

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles,
            Ok(TCycles(4)),
            "STOP placeholder should take 4 T-cycles"
        );
        assert!(
            cpu.stopped(),
            "STOP should enter the placeholder stopped state"
        );
        assert_eq!(
            cpu.registers().pc,
            0x0102,
            "STOP should consume its padding byte"
        );
    }

    #[test]
    fn cpu_can_emit_serial_test_output_through_ldh_instructions() {
        let mut bus = bus_with_bytes(&[
            (0x0100, 0x3E),
            (0x0101, b'O'),
            (0x0102, 0xE0),
            (0x0103, 0x01),
            (0x0104, 0x3E),
            (0x0105, 0x81),
            (0x0106, 0xE0),
            (0x0107, 0x02),
            (0x0108, 0x3E),
            (0x0109, b'K'),
            (0x010A, 0xE0),
            (0x010B, 0x01),
            (0x010C, 0x3E),
            (0x010D, 0x81),
            (0x010E, 0xE0),
            (0x010F, 0x02),
        ]);
        let mut cpu = Cpu::new_dmg_post_boot();

        for _ in 0..8 {
            let cycles = cpu
                .step(&mut bus)
                .expect("serial test ROM opcodes should run");
            bus.tick(cycles);
        }

        assert_eq!(
            bus.take_serial_output(),
            b"OK",
            "LDH writes to SB/SC should collect serial text output"
        );
    }
}
