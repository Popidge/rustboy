//! CPU state for the DMG Game Boy.
//!
//! This milestone models the CPU registers and post-boot initial state. It
//! does not execute instructions yet.

mod flags;
mod registers;

pub use flags::CpuFlags;
pub use registers::CpuRegisters;

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
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new_dmg_post_boot()
    }
}

#[cfg(test)]
mod tests {
    use super::Cpu;

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
}
