//! Timer hardware for the DMG Game Boy.
//!
//! Models DIV, TIMA, TMA, and TAC. The timer advances through `Bus::tick`
//! and requests the Timer interrupt when TIMA overflows.

use crate::{
    cpu::TCycles,
    interrupt::{Interrupt, InterruptFlags},
};

const DIV_ADDR: u16 = 0xFF04;
const TIMA_ADDR: u16 = 0xFF05;
const TMA_ADDR: u16 = 0xFF06;
const TAC_ADDR: u16 = 0xFF07;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timer {
    div: u8,
    div_counter: u32,
    tima: u8,
    tma: u8,
    tac: u8,
    tima_counter: u32,
}

impl Timer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            div: 0,
            div_counter: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            tima_counter: 0,
        }
    }

    #[must_use]
    pub fn read(&self, address: u16) -> u8 {
        match address {
            DIV_ADDR => self.div,
            TIMA_ADDR => self.tima,
            TMA_ADDR => self.tma,
            TAC_ADDR => self.tac | 0xF8,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        match address {
            DIV_ADDR => {
                self.div = 0;
                self.div_counter = 0;
            }
            TIMA_ADDR => self.tima = value,
            TMA_ADDR => self.tma = value,
            TAC_ADDR => {
                self.tac = value & 0x07;
                self.tima_counter = 0;
            }
            _ => {}
        }
    }

    pub fn tick(&mut self, cycles: TCycles, interrupts: &mut InterruptFlags) {
        self.tick_div(cycles);

        if !self.timer_enabled() {
            return;
        }

        self.tima_counter += cycles.0;
        let period = self.tima_period();

        while self.tima_counter >= period {
            self.tima_counter -= period;
            let (value, overflowed) = self.tima.overflowing_add(1);

            if overflowed {
                self.tima = self.tma;
                interrupts.request(Interrupt::Timer);
            } else {
                self.tima = value;
            }
        }
    }

    fn tick_div(&mut self, cycles: TCycles) {
        self.div_counter += cycles.0;

        while self.div_counter >= 256 {
            self.div_counter -= 256;
            self.div = self.div.wrapping_add(1);
        }
    }

    fn timer_enabled(&self) -> bool {
        self.tac & 0x04 != 0
    }

    fn tima_period(&self) -> u32 {
        match self.tac & 0x03 {
            0b00 => 1024,
            0b01 => 16,
            0b10 => 64,
            0b11 => 256,
            _ => unreachable!("two bits produce values 0 through 3"),
        }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Timer;
    use crate::{cpu::TCycles, interrupt::InterruptFlags};

    #[test]
    fn div_increments_every_256_t_cycles_and_write_resets_it() {
        let mut timer = Timer::new();
        let mut interrupts = InterruptFlags::default();

        timer.tick(TCycles(255), &mut interrupts);
        assert_eq!(timer.read(0xFF04), 0x00, "DIV should not increment early");

        timer.tick(TCycles(1), &mut interrupts);
        assert_eq!(
            timer.read(0xFF04),
            0x01,
            "DIV should increment after 256 cycles"
        );

        timer.write(0xFF04, 0xFF);
        assert_eq!(timer.read(0xFF04), 0x00, "writing DIV should reset it");
    }

    #[test]
    fn tima_increments_at_each_selected_frequency_when_enabled() {
        let cases = [(0b00, 1024), (0b01, 16), (0b10, 64), (0b11, 256)];

        for (select, period) in cases {
            let mut timer = Timer::new();
            let mut interrupts = InterruptFlags::default();
            timer.write(0xFF07, 0x04 | select);

            timer.tick(TCycles(period - 1), &mut interrupts);
            assert_eq!(timer.read(0xFF05), 0x00, "TIMA should wait for full period");

            timer.tick(TCycles(1), &mut interrupts);
            assert_eq!(
                timer.read(0xFF05),
                0x01,
                "TIMA should increment for TAC select {select:02b}"
            );
        }
    }

    #[test]
    fn tima_overflow_reloads_tma_and_requests_timer_interrupt() {
        let mut timer = Timer::new();
        let mut interrupts = InterruptFlags::default();
        timer.write(0xFF05, 0xFF);
        timer.write(0xFF06, 0x42);
        timer.write(0xFF07, 0x05);

        timer.tick(TCycles(16), &mut interrupts);

        assert_eq!(
            timer.read(0xFF05),
            0x42,
            "overflow should reload TIMA from TMA"
        );
        assert_eq!(
            interrupts.raw(),
            0x04,
            "overflow should request the Timer interrupt"
        );
    }
}
