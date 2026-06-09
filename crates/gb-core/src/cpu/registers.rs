use super::CpuFlags;
use std::fmt;

/// CPU registers for the DMG Game Boy.
#[derive(Clone, PartialEq, Eq)]
pub struct CpuRegisters {
    pub a: u8,
    pub f: CpuFlags,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}

impl CpuRegisters {
    /// Creates zeroed CPU registers.
    #[must_use]
    pub fn new() -> Self {
        Self {
            a: 0,
            f: CpuFlags::default(),
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            sp: 0,
            pc: 0,
        }
    }

    /// Creates the standard DMG post-boot register state.
    #[must_use]
    pub fn new_dmg_post_boot() -> Self {
        let mut registers = Self::new();
        registers.set_af(0x01B0);
        registers.set_bc(0x0013);
        registers.set_de(0x00D8);
        registers.set_hl(0x014D);
        registers.sp = 0xFFFE;
        registers.pc = 0x0100;
        registers
    }

    /// Returns the combined AF register pair.
    #[must_use]
    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f.raw()])
    }

    /// Sets the combined AF register pair.
    pub fn set_af(&mut self, value: u16) {
        let [a, f] = value.to_be_bytes();
        self.a = a;
        self.f.set_raw(f);
    }

    /// Returns the combined BC register pair.
    #[must_use]
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }

    /// Sets the combined BC register pair.
    pub fn set_bc(&mut self, value: u16) {
        let [b, c] = value.to_be_bytes();
        self.b = b;
        self.c = c;
    }

    /// Returns the combined DE register pair.
    #[must_use]
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }

    /// Sets the combined DE register pair.
    pub fn set_de(&mut self, value: u16) {
        let [d, e] = value.to_be_bytes();
        self.d = d;
        self.e = e;
    }

    /// Returns the combined HL register pair.
    #[must_use]
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }

    /// Sets the combined HL register pair.
    pub fn set_hl(&mut self, value: u16) {
        let [h, l] = value.to_be_bytes();
        self.h = h;
        self.l = l;
    }
}

impl Default for CpuRegisters {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for CpuRegisters {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "AF={:04X} BC={:04X} DE={:04X} HL={:04X} SP={:04X} PC={:04X}",
            self.af(),
            self.bc(),
            self.de(),
            self.hl(),
            self.sp,
            self.pc
        )
    }
}

#[cfg(test)]
mod tests {
    use super::CpuRegisters;

    #[test]
    fn new_registers_are_zeroed() {
        let registers = CpuRegisters::new();

        assert_eq!(registers.a, 0, "A should start at zero");
        assert_eq!(registers.f.raw(), 0, "F should start at zero");
        assert_eq!(registers.b, 0, "B should start at zero");
        assert_eq!(registers.c, 0, "C should start at zero");
        assert_eq!(registers.d, 0, "D should start at zero");
        assert_eq!(registers.e, 0, "E should start at zero");
        assert_eq!(registers.h, 0, "H should start at zero");
        assert_eq!(registers.l, 0, "L should start at zero");
        assert_eq!(registers.sp, 0, "SP should start at zero");
        assert_eq!(registers.pc, 0, "PC should start at zero");
    }

    #[test]
    fn debug_format_is_useful_for_trace_logs() {
        let mut registers = CpuRegisters::new();
        registers.set_af(0x01B0);
        registers.set_bc(0x0013);
        registers.set_de(0x00D8);
        registers.set_hl(0x014D);
        registers.sp = 0xFFFE;
        registers.pc = 0x0100;

        assert_eq!(
            format!("{registers:?}"),
            "AF=01B0 BC=0013 DE=00D8 HL=014D SP=FFFE PC=0100",
            "Debug output should be compact enough for CPU traces"
        );
    }

    #[test]
    fn set_af_updates_a_and_masks_f_lower_nibble() {
        let mut registers = CpuRegisters::new();

        registers.set_af(0x12FF);

        assert_eq!(registers.a, 0x12, "high AF byte should update A");
        assert_eq!(
            registers.f.raw(),
            0xF0,
            "low AF byte should update F with lower nibble masked"
        );
        assert_eq!(registers.af(), 0x12F0, "AF should expose masked F bits");
    }

    #[test]
    fn set_bc_updates_b_and_c() {
        let mut registers = CpuRegisters::new();

        registers.set_bc(0x1234);

        assert_eq!(registers.b, 0x12, "high BC byte should update B");
        assert_eq!(registers.c, 0x34, "low BC byte should update C");
        assert_eq!(registers.bc(), 0x1234, "BC getter should combine B and C");
    }

    #[test]
    fn set_de_updates_d_and_e() {
        let mut registers = CpuRegisters::new();

        registers.set_de(0xABCD);

        assert_eq!(registers.d, 0xAB, "high DE byte should update D");
        assert_eq!(registers.e, 0xCD, "low DE byte should update E");
        assert_eq!(registers.de(), 0xABCD, "DE getter should combine D and E");
    }

    #[test]
    fn set_hl_updates_h_and_l() {
        let mut registers = CpuRegisters::new();

        registers.set_hl(0xC001);

        assert_eq!(registers.h, 0xC0, "high HL byte should update H");
        assert_eq!(registers.l, 0x01, "low HL byte should update L");
        assert_eq!(registers.hl(), 0xC001, "HL getter should combine H and L");
    }
}
