//! Serial transfer registers for the DMG Game Boy.
//!
//! Internal-clock transfers shift one bit every 512 T-cycles. Completed bytes
//! are retained only as a headless test observer; the transfer itself requests
//! the Serial interrupt through the Bus-owned dispatcher.

use crate::{
    cpu::TCycles,
    interrupt::{Interrupt, InterruptFlags},
};

const SB_ADDR: u16 = 0xFF01;
const SC_ADDR: u16 = 0xFF02;
const TRANSFER_START: u8 = 0x80;
const INTERNAL_CLOCK: u8 = 0x01;
const SC_READ_MASK: u8 = 0x7E;
const TCYCLES_PER_BIT: u16 = 512;
const BITS_PER_TRANSFER: u8 = 8;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Serial {
    sb: u8,
    sc: u8,
    transferred_byte: u8,
    bit_tcycles: u16,
    bits_remaining: u8,
    output: Vec<u8>,
}

impl Serial {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn read(&self, address: u16) -> u8 {
        match address {
            SB_ADDR => self.sb,
            SC_ADDR => self.sc | SC_READ_MASK,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        self.write_with_clock_phase(address, value, 0);
    }

    pub fn write_with_clock_phase(&mut self, address: u16, value: u8, clock_phase: u16) {
        match address {
            SB_ADDR => self.sb = value,
            SC_ADDR => self.write_sc(value, clock_phase),
            _ => {}
        }
    }

    pub fn tick(&mut self, cycles: TCycles, interrupts: &mut InterruptFlags) {
        for _ in 0..cycles.0 {
            if !self.internal_transfer_active() {
                continue;
            }

            self.bit_tcycles += 1;
            if self.bit_tcycles != TCYCLES_PER_BIT {
                continue;
            }

            self.bit_tcycles = 0;
            self.sb = (self.sb << 1) | 1;
            self.bits_remaining -= 1;

            if self.bits_remaining == 0 {
                self.sc &= !TRANSFER_START;
                self.output.push(self.transferred_byte);
                interrupts.request(Interrupt::Serial);
            }
        }
    }

    #[must_use]
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    fn write_sc(&mut self, value: u8, clock_phase: u16) {
        self.sc = value & (TRANSFER_START | INTERNAL_CLOCK);

        if self.sc & TRANSFER_START != 0 {
            self.transferred_byte = self.sb;
            // The DMG internal serial clock is phase-aligned to the divider.
            // A transfer beginning between clock edges therefore completes
            // slightly sooner than a freshly reset 4,096-T-cycle countdown.
            self.bit_tcycles = clock_phase;
            self.bits_remaining = BITS_PER_TRANSFER;
        }
    }

    fn internal_transfer_active(&self) -> bool {
        self.sc & (TRANSFER_START | INTERNAL_CLOCK) == (TRANSFER_START | INTERNAL_CLOCK)
    }
}

#[cfg(test)]
mod tests {
    use super::Serial;
    use crate::{cpu::TCycles, interrupt::InterruptFlags};

    #[test]
    fn internal_transfer_completes_after_eight_512_tcycle_bits() {
        let mut serial = Serial::new();
        let mut interrupts = InterruptFlags::default();
        serial.write(0xFF01, b'A');
        serial.write(0xFF02, 0x81);

        serial.tick(TCycles(4_095), &mut interrupts);
        assert_eq!(serial.read(0xFF02), 0xFF, "transfer should remain active");
        assert!(
            serial.output().is_empty(),
            "output waits for transfer completion"
        );
        assert_eq!(
            interrupts.raw(),
            0,
            "Serial IF waits for transfer completion"
        );

        serial.tick(TCycles(1), &mut interrupts);
        assert_eq!(
            serial.read(0xFF01),
            0xFF,
            "disconnected input shifts in ones"
        );
        assert_eq!(
            serial.read(0xFF02),
            0x7F,
            "completion should clear SC start"
        );
        assert_eq!(
            serial.output(),
            b"A",
            "completion should collect the sent byte"
        );
        assert_eq!(
            interrupts.raw(),
            0x08,
            "completion should request Serial IF"
        );
    }

    #[test]
    fn external_clock_transfer_waits_for_external_clock_edges() {
        let mut serial = Serial::new();
        let mut interrupts = InterruptFlags::default();
        serial.write(0xFF01, b'A');
        serial.write(0xFF02, 0x80);

        serial.tick(TCycles(4_096), &mut interrupts);

        assert_eq!(
            serial.read(0xFF02),
            0xFE,
            "external transfer should stay active"
        );
        assert!(
            serial.output().is_empty(),
            "no internal clock means no completion"
        );
        assert_eq!(interrupts.raw(), 0, "no completion means no Serial IF");
    }

    #[test]
    fn a_new_start_restarts_the_in_progress_internal_transfer() {
        let mut serial = Serial::new();
        let mut interrupts = InterruptFlags::default();
        serial.write(0xFF01, b'A');
        serial.write(0xFF02, 0x81);
        serial.tick(TCycles(512), &mut interrupts);
        serial.write(0xFF01, b'B');
        serial.write(0xFF02, 0x81);
        serial.tick(TCycles(4_096), &mut interrupts);

        assert_eq!(
            serial.output(),
            b"B",
            "restart should replace the in-flight byte"
        );
    }

    #[test]
    fn take_output_drains_completed_bytes() {
        let mut serial = Serial::new();
        let mut interrupts = InterruptFlags::default();
        serial.write(0xFF01, b'O');
        serial.write(0xFF02, 0x81);
        serial.tick(TCycles(4_096), &mut interrupts);
        serial.write(0xFF01, b'K');
        serial.write(0xFF02, 0x81);
        serial.tick(TCycles(4_096), &mut interrupts);

        assert_eq!(
            serial.take_output(),
            b"OK",
            "take_output should return bytes"
        );
        assert!(
            serial.output().is_empty(),
            "take_output should drain the buffer"
        );
    }
}
