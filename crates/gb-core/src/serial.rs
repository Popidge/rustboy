//! Serial transfer registers for DMG test output.
//!
//! This is a minimal debug-oriented serial model. When software writes the
//! transfer-start bit in SC, the current SB byte is collected and a serial
//! interrupt is requested.

use crate::interrupt::{Interrupt, InterruptFlags};

const SB_ADDR: u16 = 0xFF01;
const SC_ADDR: u16 = 0xFF02;
const TRANSFER_START: u8 = 0x80;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Serial {
    sb: u8,
    sc: u8,
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
            SC_ADDR => self.sc,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, address: u16, value: u8, interrupts: &mut InterruptFlags) {
        match address {
            SB_ADDR => self.sb = value,
            SC_ADDR => {
                self.sc = value;

                if value & TRANSFER_START != 0 {
                    self.output.push(self.sb);
                    self.sc &= !TRANSFER_START;
                    interrupts.request(Interrupt::Serial);
                }
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::Serial;
    use crate::interrupt::InterruptFlags;

    #[test]
    fn serial_registers_roundtrip_and_transfer_start_collects_sb() {
        let mut serial = Serial::new();
        let mut iflags = InterruptFlags::default();

        serial.write(0xFF01, b'A', &mut iflags);
        serial.write(0xFF02, 0x81, &mut iflags);

        assert_eq!(
            serial.read(0xFF01),
            b'A',
            "SB should store the transfer byte"
        );
        assert_eq!(
            serial.read(0xFF02),
            0x01,
            "minimal transfer should clear the start bit after capture"
        );
        assert_eq!(serial.output(), b"A", "transfer start should collect SB");
    }

    #[test]
    fn take_output_drains_collected_bytes() {
        let mut serial = Serial::new();
        let mut iflags = InterruptFlags::default();
        serial.write(0xFF01, b'O', &mut iflags);
        serial.write(0xFF02, 0x80, &mut iflags);
        serial.write(0xFF01, b'K', &mut iflags);
        serial.write(0xFF02, 0x80, &mut iflags);

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
