//! Interrupt flag helpers for the DMG Game Boy.
//!
//! The `IE` and `IF` registers share the same five interrupt bits. `IF` reads
//! report the upper three bits as set on DMG hardware.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    VBlank,
    LcdStat,
    Timer,
    Serial,
    Joypad,
}

impl Interrupt {
    #[must_use]
    pub fn bit(self) -> u8 {
        match self {
            Self::VBlank => 0,
            Self::LcdStat => 1,
            Self::Timer => 2,
            Self::Serial => 3,
            Self::Joypad => 4,
        }
    }

    #[must_use]
    pub fn vector(self) -> u16 {
        match self {
            Self::VBlank => 0x0040,
            Self::LcdStat => 0x0048,
            Self::Timer => 0x0050,
            Self::Serial => 0x0058,
            Self::Joypad => 0x0060,
        }
    }

    #[must_use]
    pub fn mask(self) -> u8 {
        1 << self.bit()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InterruptFlags {
    bits: u8,
}

impl InterruptFlags {
    const MASK: u8 = 0x1F;

    #[must_use]
    pub fn raw(self) -> u8 {
        self.bits & Self::MASK
    }

    pub fn set_raw(&mut self, value: u8) {
        self.bits = value & Self::MASK;
    }

    #[must_use]
    pub fn read_if(self) -> u8 {
        self.raw() | 0xE0
    }

    pub fn write_if(&mut self, value: u8) {
        self.set_raw(value);
    }

    #[must_use]
    pub fn contains(self, interrupt: Interrupt) -> bool {
        self.raw() & interrupt.mask() != 0
    }

    pub fn request(&mut self, interrupt: Interrupt) {
        self.bits |= interrupt.mask();
        self.bits &= Self::MASK;
    }

    pub fn clear(&mut self, interrupt: Interrupt) {
        self.bits &= !interrupt.mask();
        self.bits &= Self::MASK;
    }

    #[must_use]
    pub fn first_pending(enabled: Self, requested: Self) -> Option<Interrupt> {
        let pending = enabled.raw() & requested.raw();

        [
            Interrupt::VBlank,
            Interrupt::LcdStat,
            Interrupt::Timer,
            Interrupt::Serial,
            Interrupt::Joypad,
        ]
        .into_iter()
        .find(|interrupt| pending & interrupt.mask() != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::{Interrupt, InterruptFlags};

    #[test]
    fn if_reads_force_upper_bits_and_writes_mask_to_interrupt_bits() {
        let mut flags = InterruptFlags::default();

        flags.write_if(0xFF);

        assert_eq!(flags.raw(), 0x1F, "only five interrupt bits are stored");
        assert_eq!(flags.read_if(), 0xFF, "IF reads expose upper bits as set");
    }

    #[test]
    fn request_clear_and_priority_helpers_use_typed_interrupts() {
        let mut enabled = InterruptFlags::default();
        let mut requested = InterruptFlags::default();

        enabled.request(Interrupt::Joypad);
        enabled.request(Interrupt::Timer);
        requested.request(Interrupt::Joypad);
        requested.request(Interrupt::Timer);

        assert!(requested.contains(Interrupt::Timer));
        assert_eq!(
            InterruptFlags::first_pending(enabled, requested),
            Some(Interrupt::Timer),
            "Timer has priority over Joypad"
        );

        requested.clear(Interrupt::Timer);

        assert!(!requested.contains(Interrupt::Timer));
        assert_eq!(
            InterruptFlags::first_pending(enabled, requested),
            Some(Interrupt::Joypad),
            "Joypad is next once Timer is cleared"
        );
    }
}
