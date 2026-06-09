use std::fmt;

const ZERO_FLAG: u8 = 0b1000_0000;
const SUBTRACT_FLAG: u8 = 0b0100_0000;
const HALF_CARRY_FLAG: u8 = 0b0010_0000;
const CARRY_FLAG: u8 = 0b0001_0000;
const FLAG_MASK: u8 = 0xF0;

/// CPU flag register for the DMG Game Boy.
///
/// The lower four bits of `F` are always zero.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuFlags {
    bits: u8,
}

impl CpuFlags {
    /// Creates flags from a raw `F` register value.
    #[must_use]
    pub fn from_raw(value: u8) -> Self {
        Self {
            bits: value & FLAG_MASK,
        }
    }

    /// Returns the raw `F` register value.
    #[must_use]
    pub fn raw(self) -> u8 {
        self.bits
    }

    /// Replaces the raw `F` register value.
    pub fn set_raw(&mut self, value: u8) {
        self.bits = value & FLAG_MASK;
    }

    /// Returns the zero flag.
    #[must_use]
    pub fn zero(self) -> bool {
        self.flag(ZERO_FLAG)
    }

    /// Sets or clears the zero flag.
    pub fn set_zero(&mut self, value: bool) {
        self.set_flag(ZERO_FLAG, value);
    }

    /// Returns the subtract flag.
    #[must_use]
    pub fn subtract(self) -> bool {
        self.flag(SUBTRACT_FLAG)
    }

    /// Sets or clears the subtract flag.
    pub fn set_subtract(&mut self, value: bool) {
        self.set_flag(SUBTRACT_FLAG, value);
    }

    /// Returns the half-carry flag.
    #[must_use]
    pub fn half_carry(self) -> bool {
        self.flag(HALF_CARRY_FLAG)
    }

    /// Sets or clears the half-carry flag.
    pub fn set_half_carry(&mut self, value: bool) {
        self.set_flag(HALF_CARRY_FLAG, value);
    }

    /// Returns the carry flag.
    #[must_use]
    pub fn carry(self) -> bool {
        self.flag(CARRY_FLAG)
    }

    /// Sets or clears the carry flag.
    pub fn set_carry(&mut self, value: bool) {
        self.set_flag(CARRY_FLAG, value);
    }

    fn flag(self, flag: u8) -> bool {
        self.bits & flag != 0
    }

    fn set_flag(&mut self, flag: u8, value: bool) {
        if value {
            self.bits |= flag;
        } else {
            self.bits &= !flag;
        }

        self.bits &= FLAG_MASK;
    }
}

impl fmt::Debug for CpuFlags {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "F={:02X}", self.raw())
    }
}

#[cfg(test)]
mod tests {
    use super::CpuFlags;

    #[test]
    fn from_raw_masks_lower_nibble() {
        let flags = CpuFlags::from_raw(0xBF);

        assert_eq!(
            flags.raw(),
            0xB0,
            "lower four bits of F should always be zero"
        );
    }

    #[test]
    fn set_raw_masks_lower_nibble() {
        let mut flags = CpuFlags::default();

        flags.set_raw(0x7F);

        assert_eq!(
            flags.raw(),
            0x70,
            "set_raw should preserve only the upper flag bits"
        );
    }

    #[test]
    fn zero_flag_can_be_set_cleared_and_read() {
        let mut flags = CpuFlags::default();

        flags.set_zero(true);
        assert!(flags.zero(), "Z flag should read as set");
        assert_eq!(flags.raw(), 0x80, "Z flag should use bit 7");

        flags.set_zero(false);
        assert!(!flags.zero(), "Z flag should read as cleared");
        assert_eq!(flags.raw(), 0x00, "clearing Z should clear bit 7");
    }

    #[test]
    fn subtract_flag_can_be_set_cleared_and_read() {
        let mut flags = CpuFlags::default();

        flags.set_subtract(true);
        assert!(flags.subtract(), "N flag should read as set");
        assert_eq!(flags.raw(), 0x40, "N flag should use bit 6");

        flags.set_subtract(false);
        assert!(!flags.subtract(), "N flag should read as cleared");
        assert_eq!(flags.raw(), 0x00, "clearing N should clear bit 6");
    }

    #[test]
    fn half_carry_flag_can_be_set_cleared_and_read() {
        let mut flags = CpuFlags::default();

        flags.set_half_carry(true);
        assert!(flags.half_carry(), "H flag should read as set");
        assert_eq!(flags.raw(), 0x20, "H flag should use bit 5");

        flags.set_half_carry(false);
        assert!(!flags.half_carry(), "H flag should read as cleared");
        assert_eq!(flags.raw(), 0x00, "clearing H should clear bit 5");
    }

    #[test]
    fn carry_flag_can_be_set_cleared_and_read() {
        let mut flags = CpuFlags::default();

        flags.set_carry(true);
        assert!(flags.carry(), "C flag should read as set");
        assert_eq!(flags.raw(), 0x10, "C flag should use bit 4");

        flags.set_carry(false);
        assert!(!flags.carry(), "C flag should read as cleared");
        assert_eq!(flags.raw(), 0x00, "clearing C should clear bit 4");
    }

    #[test]
    fn flags_are_independent() {
        let mut flags = CpuFlags::default();

        flags.set_zero(true);
        flags.set_half_carry(true);

        assert!(flags.zero(), "Z flag should remain set");
        assert!(!flags.subtract(), "N flag should remain clear");
        assert!(flags.half_carry(), "H flag should remain set");
        assert!(!flags.carry(), "C flag should remain clear");
        assert_eq!(flags.raw(), 0xA0, "Z and H should be the only set flags");
    }
}
