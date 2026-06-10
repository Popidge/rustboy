//! Joypad hardware for the DMG Game Boy.
//!
//! The joypad register at FF00 selects either action buttons or directions.
//! Button bits are active-low: pressed buttons read as 0.

use crate::interrupt::{Interrupt, InterruptFlags};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    A,
    B,
    Start,
    Select,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joypad {
    select_bits: u8,
    pressed: [bool; 8],
}

impl Joypad {
    #[must_use]
    pub fn new() -> Self {
        Self {
            select_bits: 0x30,
            pressed: [false; 8],
        }
    }

    #[must_use]
    pub fn read(&self) -> u8 {
        let mut value = 0xC0 | self.select_bits | 0x0F;

        if self.select_bits & 0x20 == 0 {
            value &= self.action_bits();
        }

        if self.select_bits & 0x10 == 0 {
            value &= self.direction_bits();
        }

        value
    }

    pub fn write(&mut self, value: u8) {
        self.select_bits = value & 0x30;
    }

    pub fn set_button(&mut self, button: Button, pressed: bool, interrupts: &mut InterruptFlags) {
        let index = button_index(button);
        let was_pressed = self.pressed[index];
        self.pressed[index] = pressed;

        if pressed && !was_pressed {
            interrupts.request(Interrupt::Joypad);
        }
    }

    #[must_use]
    pub fn is_pressed(&self, button: Button) -> bool {
        self.pressed[button_index(button)]
    }

    fn action_bits(&self) -> u8 {
        let mut bits = 0x0F;
        bits = apply_active_low(bits, 0, self.is_pressed(Button::A));
        bits = apply_active_low(bits, 1, self.is_pressed(Button::B));
        bits = apply_active_low(bits, 2, self.is_pressed(Button::Select));
        bits = apply_active_low(bits, 3, self.is_pressed(Button::Start));
        bits
    }

    fn direction_bits(&self) -> u8 {
        let mut bits = 0x0F;
        bits = apply_active_low(bits, 0, self.is_pressed(Button::Right));
        bits = apply_active_low(bits, 1, self.is_pressed(Button::Left));
        bits = apply_active_low(bits, 2, self.is_pressed(Button::Up));
        bits = apply_active_low(bits, 3, self.is_pressed(Button::Down));
        bits
    }
}

impl Default for Joypad {
    fn default() -> Self {
        Self::new()
    }
}

fn apply_active_low(bits: u8, bit: u8, pressed: bool) -> u8 {
    if pressed {
        bits & !(1 << bit)
    } else {
        bits
    }
}

fn button_index(button: Button) -> usize {
    match button {
        Button::A => 0,
        Button::B => 1,
        Button::Start => 2,
        Button::Select => 3,
        Button::Up => 4,
        Button::Down => 5,
        Button::Left => 6,
        Button::Right => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::{Button, Joypad};
    use crate::interrupt::{Interrupt, InterruptFlags};

    #[test]
    fn tracks_pressed_and_released_buttons() {
        let mut joypad = Joypad::new();
        let mut interrupts = InterruptFlags::default();

        joypad.set_button(Button::A, true, &mut interrupts);
        assert!(joypad.is_pressed(Button::A));

        joypad.set_button(Button::A, false, &mut interrupts);
        assert!(!joypad.is_pressed(Button::A));
    }

    #[test]
    fn reads_action_group_with_active_low_buttons() {
        let mut joypad = Joypad::new();
        let mut interrupts = InterruptFlags::default();

        joypad.write(0x10);
        joypad.set_button(Button::A, true, &mut interrupts);
        joypad.set_button(Button::Start, true, &mut interrupts);

        assert_eq!(
            joypad.read() & 0x0F,
            0b0110,
            "selected action buttons should read pressed bits as zero"
        );
    }

    #[test]
    fn reads_direction_group_with_active_low_buttons() {
        let mut joypad = Joypad::new();
        let mut interrupts = InterruptFlags::default();

        joypad.write(0x20);
        joypad.set_button(Button::Right, true, &mut interrupts);
        joypad.set_button(Button::Down, true, &mut interrupts);

        assert_eq!(
            joypad.read() & 0x0F,
            0b0110,
            "selected direction buttons should read pressed bits as zero"
        );
    }

    #[test]
    fn button_press_requests_joypad_interrupt_once_per_press() {
        let mut joypad = Joypad::new();
        let mut interrupts = InterruptFlags::default();

        joypad.set_button(Button::A, true, &mut interrupts);
        assert!(interrupts.contains(Interrupt::Joypad));

        interrupts.clear(Interrupt::Joypad);
        joypad.set_button(Button::A, true, &mut interrupts);
        assert!(!interrupts.contains(Interrupt::Joypad));

        joypad.set_button(Button::A, false, &mut interrupts);
        joypad.set_button(Button::A, true, &mut interrupts);
        assert!(interrupts.contains(Interrupt::Joypad));
    }
}
