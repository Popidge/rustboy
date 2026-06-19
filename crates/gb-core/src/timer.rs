//! Timer hardware for the DMG Game Boy.
//!
//! Models DIV, TIMA, TMA, and TAC. TIMA increments from falling edges of the
//! TAC-selected internal DIV bit, and overflow reloads are delayed to match
//! the hardware-visible timer pipeline.

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
    div_counter: u16,
    tima: u8,
    tma: u8,
    tac: u8,
    overflow_delay: Option<u8>,
    reload_tma_latch: Option<u8>,
}

/// Timer state captured alongside a test-only bus-cycle trace record.
#[cfg(any(test, feature = "test-trace"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimerTraceState {
    pub tima: u8,
    pub tma: u8,
    pub overflow_delay: Option<u8>,
}

impl Timer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            div_counter: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            overflow_delay: None,
            reload_tma_latch: None,
        }
    }

    #[must_use]
    pub fn read(&self, address: u16) -> u8 {
        match address {
            DIV_ADDR => self.div(),
            TIMA_ADDR => self.tima,
            TMA_ADDR => self.tma,
            TAC_ADDR => self.tac | 0xF8,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        match address {
            DIV_ADDR => {
                let old_signal = self.timer_signal();
                self.div_counter = 0;
                self.increment_on_falling_edge(old_signal, self.timer_signal());
            }
            TIMA_ADDR => {
                if self.overflow_delay != Some(1) {
                    self.tima = value;
                    self.overflow_delay = None;
                }
            }
            TMA_ADDR => self.tma = value,
            TAC_ADDR => {
                let old_signal = self.timer_signal();
                self.tac = value & 0x07;
                self.increment_on_falling_edge(old_signal, self.timer_signal());
            }
            _ => {}
        }
    }

    pub fn tick(&mut self, cycles: TCycles, interrupts: &mut InterruptFlags) {
        for _ in 0..cycles.0 {
            self.tick_overflow_delay(interrupts);

            let old_signal = self.timer_signal();
            self.div_counter = self.div_counter.wrapping_add(1);
            self.increment_on_falling_edge(old_signal, self.timer_signal());
        }
    }

    /// Captures TMA at the start of a CPU machine cycle.
    ///
    /// A TMA write in the M-cycle where the reload transfer occurs updates
    /// TMA, but the transfer itself still uses this latched old value.
    pub(crate) fn begin_cpu_mcycle(&mut self) {
        self.reload_tma_latch = Some(self.tma);
    }

    /// Ends the current CPU machine cycle's TMA reload latch.
    pub(crate) fn end_cpu_mcycle(&mut self) {
        self.reload_tma_latch = None;
    }

    /// Returns the divider phase used to align the DMG internal serial clock.
    #[must_use]
    pub(crate) fn serial_clock_phase(&self) -> u16 {
        self.div_counter & 0x00FF
    }

    #[cfg(any(test, feature = "test-trace"))]
    #[must_use]
    pub(crate) fn trace_state(&self) -> TimerTraceState {
        TimerTraceState {
            tima: self.tima,
            tma: self.tma,
            overflow_delay: self.overflow_delay,
        }
    }

    fn tick_overflow_delay(&mut self, interrupts: &mut InterruptFlags) {
        if let Some(delay) = self.overflow_delay {
            if delay == 1 {
                self.tima = self.reload_tma_latch.unwrap_or(self.tma);
                self.overflow_delay = None;
                interrupts.request(Interrupt::Timer);
            } else {
                self.overflow_delay = Some(delay - 1);
            }
        }
    }

    fn increment_on_falling_edge(&mut self, old_signal: bool, new_signal: bool) {
        if old_signal && !new_signal {
            self.increment_tima();
        }
    }

    fn increment_tima(&mut self) {
        if self.overflow_delay.is_some() {
            return;
        }

        let (value, overflowed) = self.tima.overflowing_add(1);
        self.tima = value;

        if overflowed {
            self.overflow_delay = Some(4);
        }
    }

    fn div(&self) -> u8 {
        (self.div_counter >> 8) as u8
    }

    fn timer_signal(&self) -> bool {
        self.timer_enabled() && (self.div_counter & self.selected_div_bit_mask()) != 0
    }

    fn timer_enabled(&self) -> bool {
        self.tac & 0x04 != 0
    }

    fn selected_div_bit_mask(&self) -> u16 {
        1 << self.selected_div_bit()
    }

    fn selected_div_bit(&self) -> u8 {
        match self.tac & 0x03 {
            0b00 => 9,
            0b01 => 3,
            0b10 => 5,
            0b11 => 7,
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
            0x00,
            "overflow should leave TIMA at 0 during the reload delay"
        );
        assert_eq!(
            interrupts.raw(),
            0x00,
            "overflow should not request the Timer interrupt before reload"
        );

        timer.tick(TCycles(4), &mut interrupts);

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

    #[test]
    fn div_write_can_trigger_selected_bit_falling_edge() {
        let mut timer = Timer::new();
        let mut interrupts = InterruptFlags::default();
        timer.write(0xFF07, 0x05);

        timer.tick(TCycles(8), &mut interrupts);
        assert_eq!(
            timer.read(0xFF05),
            0x00,
            "selected bit should be high but not yet falling"
        );

        timer.write(0xFF04, 0x00);

        assert_eq!(
            timer.read(0xFF05),
            0x01,
            "resetting DIV while the selected bit is high should increment TIMA"
        );
    }

    #[test]
    fn tac_write_can_trigger_selected_bit_falling_edge() {
        let mut timer = Timer::new();
        let mut interrupts = InterruptFlags::default();
        timer.write(0xFF07, 0x05);
        timer.tick(TCycles(8), &mut interrupts);

        timer.write(0xFF07, 0x00);

        assert_eq!(
            timer.read(0xFF05),
            0x01,
            "disabling TAC while the selected timer signal is high should increment TIMA"
        );
    }

    #[test]
    fn tima_write_during_overflow_delay_cancels_reload() {
        let mut timer = Timer::new();
        let mut interrupts = InterruptFlags::default();
        timer.write(0xFF05, 0xFF);
        timer.write(0xFF06, 0x42);
        timer.write(0xFF07, 0x05);

        timer.tick(TCycles(16), &mut interrupts);
        timer.write(0xFF05, 0x99);
        timer.tick(TCycles(4), &mut interrupts);

        assert_eq!(
            timer.read(0xFF05),
            0x99,
            "writing TIMA during overflow delay should cancel the pending reload"
        );
        assert_eq!(
            interrupts.raw(),
            0x00,
            "cancelled overflow should not request the Timer interrupt"
        );
    }

    #[test]
    fn tima_write_on_reload_cycle_is_ignored() {
        let mut timer = Timer::new();
        let mut interrupts = InterruptFlags::default();
        timer.write(0xFF05, 0xFF);
        timer.write(0xFF06, 0x42);
        timer.write(0xFF07, 0x05);

        timer.tick(TCycles(16), &mut interrupts);
        timer.tick(TCycles(3), &mut interrupts);
        timer.write(0xFF05, 0x99);
        timer.tick(TCycles(1), &mut interrupts);

        assert_eq!(
            timer.read(0xFF05),
            0x42,
            "writing TIMA on the reload cycle should not override the TMA reload"
        );
        assert_eq!(
            interrupts.raw(),
            0x04,
            "reload cycle should still request the Timer interrupt"
        );
    }

    #[test]
    fn tma_write_during_overflow_delay_updates_reload_value() {
        let mut timer = Timer::new();
        let mut interrupts = InterruptFlags::default();
        timer.write(0xFF05, 0xFF);
        timer.write(0xFF06, 0x42);
        timer.write(0xFF07, 0x05);

        timer.tick(TCycles(16), &mut interrupts);
        timer.write(0xFF06, 0x77);
        timer.tick(TCycles(4), &mut interrupts);

        assert_eq!(
            timer.read(0xFF05),
            0x77,
            "reload should use the current TMA value when the delay expires"
        );
        assert_eq!(
            interrupts.raw(),
            0x04,
            "reload should request the Timer interrupt"
        );
    }

    #[test]
    fn tma_write_on_reload_cycle_updates_the_reloaded_tima_value() {
        let mut timer = Timer::new();
        let mut interrupts = InterruptFlags::default();
        timer.write(0xFF05, 0xFF);
        timer.write(0xFF06, 0x42);
        timer.write(0xFF07, 0x05);

        timer.tick(TCycles(16), &mut interrupts);
        timer.tick(TCycles(3), &mut interrupts);
        timer.write(0xFF06, 0x77);
        timer.tick(TCycles(1), &mut interrupts);

        assert_eq!(
            timer.read(0xFF05),
            0x77,
            "a TMA write on the reload cycle should supply the TIMA reload value"
        );
        assert_eq!(
            interrupts.raw(),
            0x04,
            "the reload-cycle TMA write must not suppress Timer IF"
        );
    }

    #[test]
    fn cpu_mcycle_tma_write_on_reload_uses_the_prewrite_tma_latch() {
        let mut timer = Timer::new();
        let mut interrupts = InterruptFlags::default();
        timer.write(0xFF05, 0xFF);
        timer.write(0xFF06, 0x42);
        timer.write(0xFF07, 0x05);

        timer.tick(TCycles(16), &mut interrupts);
        timer.tick(TCycles(3), &mut interrupts);
        timer.begin_cpu_mcycle();
        timer.write(0xFF06, 0x77);
        timer.tick(TCycles(1), &mut interrupts);
        timer.end_cpu_mcycle();

        assert_eq!(
            timer.read(0xFF05),
            0x42,
            "a same-M-cycle TMA write must not replace the value already latched for TIMA reload"
        );
        assert_eq!(
            timer.read(0xFF06),
            0x77,
            "the TMA register should still update"
        );
    }
}
