//! DMG audio processing unit.
//!
//! This is a practical first-pass APU: it models the four DMG sound channels,
//! the frame sequencer, audio register routing, and produces signed stereo PCM
//! samples for a frontend-owned audio backend.

use crate::cpu::TCycles;

pub const AUDIO_SAMPLE_RATE: u32 = 44_100;

const CPU_CLOCK_HZ: u32 = 4_194_304;
const FRAME_SEQUENCER_PERIOD: u32 = CPU_CLOCK_HZ / 512;
const SAMPLE_SCALE: f32 = 0.20;

const NR10: u16 = 0xFF10;
const NR11: u16 = 0xFF11;
const NR12: u16 = 0xFF12;
const NR13: u16 = 0xFF13;
const NR14: u16 = 0xFF14;
const NR21: u16 = 0xFF16;
const NR22: u16 = 0xFF17;
const NR23: u16 = 0xFF18;
const NR24: u16 = 0xFF19;
const NR30: u16 = 0xFF1A;
const NR31: u16 = 0xFF1B;
const NR32: u16 = 0xFF1C;
const NR33: u16 = 0xFF1D;
const NR34: u16 = 0xFF1E;
const NR41: u16 = 0xFF20;
const NR42: u16 = 0xFF21;
const NR43: u16 = 0xFF22;
const NR44: u16 = 0xFF23;
const NR50: u16 = 0xFF24;
const NR51: u16 = 0xFF25;
const NR52: u16 = 0xFF26;
const WAVE_RAM_START: u16 = 0xFF30;
const WAVE_RAM_END: u16 = 0xFF3F;
const WAVE_RAM_SIZE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StereoSample {
    pub left: i16,
    pub right: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Apu {
    powered: bool,
    frame_step: u8,
    frame_counter: u32,
    sample_counter: u32,
    nr50: u8,
    nr51: u8,
    ch1: SquareChannel,
    ch2: SquareChannel,
    wave: WaveChannel,
    noise: NoiseChannel,
    samples: Vec<StereoSample>,
}

impl Apu {
    #[must_use]
    pub fn new() -> Self {
        Self {
            powered: true,
            frame_step: 0,
            frame_counter: 0,
            sample_counter: 0,
            nr50: 0x77,
            nr51: 0xF3,
            ch1: SquareChannel::new(true),
            ch2: SquareChannel::new(false),
            wave: WaveChannel::new(),
            noise: NoiseChannel::new(),
            samples: Vec::new(),
        }
    }

    #[must_use]
    pub fn read(&self, address: u16) -> u8 {
        if matches!(address, WAVE_RAM_START..=WAVE_RAM_END) {
            return self.wave.wave_ram[usize::from(address - WAVE_RAM_START)];
        }

        match address {
            NR10 => self.ch1.nr0 | 0x80,
            NR11 => self.ch1.nr1 | 0x3F,
            NR12 => self.ch1.nr2,
            NR14 => self.ch1.nr4 | 0xB8,
            NR21 => self.ch2.nr1 | 0x3F,
            NR22 => self.ch2.nr2,
            NR24 => self.ch2.nr4 | 0xB8,
            NR30 => (self.wave.nr0 & 0x80) | 0x7F,
            NR32 => self.wave.nr2 | 0x9F,
            NR34 => self.wave.nr4 | 0xB8,
            NR42 => self.noise.nr2,
            NR43 => self.noise.nr3,
            NR44 => self.noise.nr4 | 0xB8,
            NR50 => self.nr50,
            NR51 => self.nr51,
            NR52 => {
                0x70 | if self.powered { 0x80 } else { 0 }
                    | u8::from(self.ch1.enabled)
                    | (u8::from(self.ch2.enabled) << 1)
                    | (u8::from(self.wave.enabled) << 2)
                    | (u8::from(self.noise.enabled) << 3)
            }
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        if matches!(address, WAVE_RAM_START..=WAVE_RAM_END) {
            self.wave.wave_ram[usize::from(address - WAVE_RAM_START)] = value;
            return;
        }

        if address == NR52 {
            self.write_nr52(value);
            return;
        }

        if !self.powered {
            return;
        }

        match address {
            NR10 => self.ch1.write_sweep(value),
            NR11 => self.ch1.write_duty_length(value),
            NR12 => self.ch1.write_envelope(value),
            NR13 => self.ch1.write_period_low(value),
            NR14 => self.ch1.write_control(value),
            NR21 => self.ch2.write_duty_length(value),
            NR22 => self.ch2.write_envelope(value),
            NR23 => self.ch2.write_period_low(value),
            NR24 => self.ch2.write_control(value),
            NR30 => self.wave.write_dac(value),
            NR31 => self.wave.write_length(value),
            NR32 => self.wave.write_output_level(value),
            NR33 => self.wave.write_period_low(value),
            NR34 => self.wave.write_control(value),
            NR41 => self.noise.write_length(value),
            NR42 => self.noise.write_envelope(value),
            NR43 => self.noise.write_polynomial(value),
            NR44 => self.noise.write_control(value),
            NR50 => self.nr50 = value,
            NR51 => self.nr51 = value,
            _ => {}
        }
    }

    pub fn tick(&mut self, cycles: TCycles) {
        if !self.powered {
            return;
        }

        self.tick_frame_sequencer(cycles.0);
        self.ch1.tick(cycles.0);
        self.ch2.tick(cycles.0);
        self.wave.tick(cycles.0);
        self.noise.tick(cycles.0);
        self.tick_samples(cycles.0);
    }

    #[must_use]
    pub fn samples(&self) -> &[StereoSample] {
        &self.samples
    }

    pub fn take_samples(&mut self) -> Vec<StereoSample> {
        std::mem::take(&mut self.samples)
    }

    fn write_nr52(&mut self, value: u8) {
        let should_power = value & 0x80 != 0;

        if self.powered && !should_power {
            self.powered = false;
            self.frame_step = 0;
            self.frame_counter = 0;
            self.sample_counter = 0;
            self.nr50 = 0;
            self.nr51 = 0;
            self.ch1.power_off();
            self.ch2.power_off();
            self.wave.power_off();
            self.noise.power_off();
            self.samples.clear();
        } else if !self.powered && should_power {
            self.powered = true;
            self.frame_step = 0;
            self.frame_counter = 0;
            self.sample_counter = 0;
        }
    }

    fn tick_frame_sequencer(&mut self, cycles: u32) {
        self.frame_counter += cycles;

        while self.frame_counter >= FRAME_SEQUENCER_PERIOD {
            self.frame_counter -= FRAME_SEQUENCER_PERIOD;

            if matches!(self.frame_step, 0 | 2 | 4 | 6) {
                self.ch1.tick_length();
                self.ch2.tick_length();
                self.wave.tick_length();
                self.noise.tick_length();
            }
            if matches!(self.frame_step, 2 | 6) {
                self.ch1.tick_sweep();
            }
            if self.frame_step == 7 {
                self.ch1.tick_envelope();
                self.ch2.tick_envelope();
                self.noise.tick_envelope();
            }

            self.frame_step = (self.frame_step + 1) & 0x07;
        }
    }

    fn tick_samples(&mut self, cycles: u32) {
        self.sample_counter += cycles * AUDIO_SAMPLE_RATE;

        while self.sample_counter >= CPU_CLOCK_HZ {
            self.sample_counter -= CPU_CLOCK_HZ;
            self.samples.push(self.mix_sample());
        }
    }

    fn mix_sample(&self) -> StereoSample {
        let outputs = [
            self.ch1.output(),
            self.ch2.output(),
            self.wave.output(),
            self.noise.output(),
        ];
        let mut left = 0.0_f32;
        let mut right = 0.0_f32;

        for (channel, output) in outputs.iter().enumerate() {
            let Some(output) = output else {
                continue;
            };
            let centered = (f32::from(*output) / 15.0) * 2.0 - 1.0;

            if self.nr51 & (1 << channel) != 0 {
                right += centered;
            }
            if self.nr51 & (1 << (channel + 4)) != 0 {
                left += centered;
            }
        }

        let left_volume = f32::from((self.nr50 >> 4) & 0x07) + 1.0;
        let right_volume = f32::from(self.nr50 & 0x07) + 1.0;
        StereoSample {
            left: float_to_i16(left * left_volume / 8.0 * SAMPLE_SCALE),
            right: float_to_i16(right * right_volume / 8.0 * SAMPLE_SCALE),
        }
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::cast_possible_truncation)]
fn float_to_i16(value: f32) -> i16 {
    let scaled = value.clamp(-1.0, 1.0) * f32::from(i16::MAX);
    scaled.round() as i16
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct SquareChannel {
    has_sweep: bool,
    enabled: bool,
    dac_enabled: bool,
    nr0: u8,
    nr1: u8,
    nr2: u8,
    nr3: u8,
    nr4: u8,
    length_counter: u16,
    timer: u32,
    duty_step: usize,
    volume: u8,
    envelope_timer: u8,
    shadow_period: u16,
    sweep_timer: u8,
    sweep_enabled: bool,
}

impl SquareChannel {
    fn new(has_sweep: bool) -> Self {
        Self {
            has_sweep,
            enabled: false,
            dac_enabled: false,
            nr0: 0,
            nr1: 0,
            nr2: 0,
            nr3: 0,
            nr4: 0,
            length_counter: 0,
            timer: 0,
            duty_step: 0,
            volume: 0,
            envelope_timer: 0,
            shadow_period: 0,
            sweep_timer: 0,
            sweep_enabled: false,
        }
    }

    fn power_off(&mut self) {
        let has_sweep = self.has_sweep;
        *self = Self::new(has_sweep);
    }

    fn write_sweep(&mut self, value: u8) {
        if self.has_sweep {
            self.nr0 = value & 0x7F;
        }
    }

    fn write_duty_length(&mut self, value: u8) {
        self.nr1 = value;
        self.length_counter = 64 - u16::from(value & 0x3F);
    }

    fn write_envelope(&mut self, value: u8) {
        self.nr2 = value;
        self.dac_enabled = value & 0xF8 != 0;
        if !self.dac_enabled {
            self.enabled = false;
        }
    }

    fn write_period_low(&mut self, value: u8) {
        self.nr3 = value;
    }

    fn write_control(&mut self, value: u8) {
        self.nr4 = value & 0xC7;
        if value & 0x80 != 0 {
            self.trigger();
        }
    }

    fn tick(&mut self, cycles: u32) {
        if !self.enabled {
            return;
        }

        let mut remaining = cycles;
        while remaining >= self.timer {
            remaining -= self.timer;
            self.duty_step = (self.duty_step + 1) & 0x07;
            self.timer = self.timer_reload();
        }
        self.timer -= remaining;
    }

    fn tick_length(&mut self) {
        if self.nr4 & 0x40 == 0 || self.length_counter == 0 {
            return;
        }

        self.length_counter -= 1;
        if self.length_counter == 0 {
            self.enabled = false;
        }
    }

    fn tick_envelope(&mut self) {
        let period = self.nr2 & 0x07;
        if period == 0 || self.envelope_timer == 0 {
            return;
        }

        self.envelope_timer -= 1;
        if self.envelope_timer != 0 {
            return;
        }

        self.envelope_timer = period;
        if self.nr2 & 0x08 != 0 {
            self.volume = (self.volume + 1).min(15);
        } else {
            self.volume = self.volume.saturating_sub(1);
        }
    }

    fn tick_sweep(&mut self) {
        if !self.has_sweep || !self.sweep_enabled {
            return;
        }

        self.sweep_timer = self.sweep_timer.saturating_sub(1);
        if self.sweep_timer != 0 {
            return;
        }

        self.sweep_timer = self.sweep_period().max(1);
        let shift = self.nr0 & 0x07;
        if shift == 0 {
            return;
        }

        if let Some(new_period) = self.calculate_sweep_period() {
            self.set_period(new_period);
            self.shadow_period = new_period;
            let _ = self.calculate_sweep_period();
        } else {
            self.enabled = false;
        }
    }

    fn output(&self) -> Option<u8> {
        if !self.enabled || !self.dac_enabled {
            return None;
        }

        let duty = usize::from(self.nr1 >> 6);
        Some(if DUTY_PATTERNS[duty][self.duty_step] {
            self.volume
        } else {
            0
        })
    }

    fn trigger(&mut self) {
        self.enabled = self.dac_enabled;
        if self.length_counter == 0 {
            self.length_counter = 64;
        }

        self.timer = self.timer_reload();
        self.duty_step = 0;
        self.volume = self.nr2 >> 4;
        self.envelope_timer = (self.nr2 & 0x07).max(1);
        self.shadow_period = self.period();

        if self.has_sweep {
            self.sweep_timer = self.sweep_period().max(1);
            self.sweep_enabled = self.sweep_period() != 0 || self.sweep_shift() != 0;
            if self.sweep_shift() != 0 && self.calculate_sweep_period().is_none() {
                self.enabled = false;
            }
        }
    }

    fn period(&self) -> u16 {
        u16::from(self.nr3) | (u16::from(self.nr4 & 0x07) << 8)
    }

    fn set_period(&mut self, period: u16) {
        self.nr3 = period.to_le_bytes()[0];
        self.nr4 = (self.nr4 & !0x07) | (((period >> 8) as u8) & 0x07);
    }

    fn timer_reload(&self) -> u32 {
        (2048 - u32::from(self.period())).max(1) * 4
    }

    fn sweep_period(&self) -> u8 {
        (self.nr0 >> 4) & 0x07
    }

    fn sweep_shift(&self) -> u8 {
        self.nr0 & 0x07
    }

    fn calculate_sweep_period(&self) -> Option<u16> {
        let delta = self.shadow_period >> self.sweep_shift();
        let period = if self.nr0 & 0x08 != 0 {
            self.shadow_period.wrapping_sub(delta)
        } else {
            self.shadow_period.wrapping_add(delta)
        };

        (period <= 2047).then_some(period)
    }
}

const DUTY_PATTERNS: [[bool; 8]; 4] = [
    [false, false, false, false, false, false, false, true],
    [true, false, false, false, false, false, false, true],
    [true, false, false, false, false, true, true, true],
    [false, true, true, true, true, true, true, false],
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct WaveChannel {
    enabled: bool,
    dac_enabled: bool,
    nr0: u8,
    nr1: u8,
    nr2: u8,
    nr3: u8,
    nr4: u8,
    length_counter: u16,
    timer: u32,
    sample_index: usize,
    wave_ram: [u8; WAVE_RAM_SIZE],
}

impl WaveChannel {
    fn new() -> Self {
        Self {
            enabled: false,
            dac_enabled: false,
            nr0: 0,
            nr1: 0,
            nr2: 0,
            nr3: 0,
            nr4: 0,
            length_counter: 0,
            timer: 0,
            sample_index: 0,
            wave_ram: [0; WAVE_RAM_SIZE],
        }
    }

    fn power_off(&mut self) {
        *self = Self::new();
    }

    fn write_dac(&mut self, value: u8) {
        self.nr0 = value & 0x80;
        self.dac_enabled = value & 0x80 != 0;
        if !self.dac_enabled {
            self.enabled = false;
        }
    }

    fn write_length(&mut self, value: u8) {
        self.nr1 = value;
        self.length_counter = 256 - u16::from(value);
    }

    fn write_output_level(&mut self, value: u8) {
        self.nr2 = value & 0x60;
    }

    fn write_period_low(&mut self, value: u8) {
        self.nr3 = value;
    }

    fn write_control(&mut self, value: u8) {
        self.nr4 = value & 0xC7;
        if value & 0x80 != 0 {
            self.trigger();
        }
    }

    fn tick(&mut self, cycles: u32) {
        if !self.enabled {
            return;
        }

        let mut remaining = cycles;
        while remaining >= self.timer {
            remaining -= self.timer;
            self.sample_index = (self.sample_index + 1) & 0x1F;
            self.timer = self.timer_reload();
        }
        self.timer -= remaining;
    }

    fn tick_length(&mut self) {
        if self.nr4 & 0x40 == 0 || self.length_counter == 0 {
            return;
        }

        self.length_counter -= 1;
        if self.length_counter == 0 {
            self.enabled = false;
        }
    }

    fn output(&self) -> Option<u8> {
        if !self.enabled || !self.dac_enabled {
            return None;
        }

        let byte = self.wave_ram[self.sample_index / 2];
        let sample = if self.sample_index & 1 == 0 {
            byte >> 4
        } else {
            byte & 0x0F
        };

        Some(match (self.nr2 >> 5) & 0x03 {
            0 => 0,
            1 => sample,
            2 => sample >> 1,
            3 => sample >> 2,
            _ => unreachable!("two bits produce values 0 through 3"),
        })
    }

    fn trigger(&mut self) {
        self.enabled = self.dac_enabled;
        if self.length_counter == 0 {
            self.length_counter = 256;
        }
        self.timer = self.timer_reload();
        self.sample_index = 0;
    }

    fn period(&self) -> u16 {
        u16::from(self.nr3) | (u16::from(self.nr4 & 0x07) << 8)
    }

    fn timer_reload(&self) -> u32 {
        (2048 - u32::from(self.period())).max(1) * 2
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NoiseChannel {
    enabled: bool,
    dac_enabled: bool,
    nr1: u8,
    nr2: u8,
    nr3: u8,
    nr4: u8,
    length_counter: u16,
    timer: u32,
    volume: u8,
    envelope_timer: u8,
    lfsr: u16,
}

impl NoiseChannel {
    fn new() -> Self {
        Self {
            enabled: false,
            dac_enabled: false,
            nr1: 0,
            nr2: 0,
            nr3: 0,
            nr4: 0,
            length_counter: 0,
            timer: 0,
            volume: 0,
            envelope_timer: 0,
            lfsr: 0x7FFF,
        }
    }

    fn power_off(&mut self) {
        *self = Self::new();
    }

    fn write_length(&mut self, value: u8) {
        self.nr1 = value & 0x3F;
        self.length_counter = 64 - u16::from(value & 0x3F);
    }

    fn write_envelope(&mut self, value: u8) {
        self.nr2 = value;
        self.dac_enabled = value & 0xF8 != 0;
        if !self.dac_enabled {
            self.enabled = false;
        }
    }

    fn write_polynomial(&mut self, value: u8) {
        self.nr3 = value;
    }

    fn write_control(&mut self, value: u8) {
        self.nr4 = value & 0xC0;
        if value & 0x80 != 0 {
            self.trigger();
        }
    }

    fn tick(&mut self, cycles: u32) {
        if !self.enabled {
            return;
        }

        let mut remaining = cycles;
        while remaining >= self.timer {
            remaining -= self.timer;
            let feedback = (self.lfsr & 1) ^ ((self.lfsr >> 1) & 1);
            self.lfsr = (self.lfsr >> 1) | (feedback << 14);
            if self.nr3 & 0x08 != 0 {
                self.lfsr = (self.lfsr & !(1 << 6)) | (feedback << 6);
            }
            self.timer = self.timer_reload();
        }
        self.timer -= remaining;
    }

    fn tick_length(&mut self) {
        if self.nr4 & 0x40 == 0 || self.length_counter == 0 {
            return;
        }

        self.length_counter -= 1;
        if self.length_counter == 0 {
            self.enabled = false;
        }
    }

    fn tick_envelope(&mut self) {
        let period = self.nr2 & 0x07;
        if period == 0 || self.envelope_timer == 0 {
            return;
        }

        self.envelope_timer -= 1;
        if self.envelope_timer != 0 {
            return;
        }

        self.envelope_timer = period;
        if self.nr2 & 0x08 != 0 {
            self.volume = (self.volume + 1).min(15);
        } else {
            self.volume = self.volume.saturating_sub(1);
        }
    }

    fn output(&self) -> Option<u8> {
        if !self.enabled || !self.dac_enabled {
            return None;
        }

        Some(if self.lfsr & 1 == 0 { self.volume } else { 0 })
    }

    fn trigger(&mut self) {
        self.enabled = self.dac_enabled;
        if self.length_counter == 0 {
            self.length_counter = 64;
        }
        self.timer = self.timer_reload();
        self.volume = self.nr2 >> 4;
        self.envelope_timer = (self.nr2 & 0x07).max(1);
        self.lfsr = 0x7FFF;
    }

    fn timer_reload(&self) -> u32 {
        let divisor = match self.nr3 & 0x07 {
            0 => 8,
            value => u32::from(value) * 16,
        };
        divisor << (self.nr3 >> 4)
    }
}

#[cfg(test)]
mod tests {
    use super::{Apu, AUDIO_SAMPLE_RATE, FRAME_SEQUENCER_PERIOD};
    use crate::cpu::TCycles;

    #[test]
    fn audio_registers_roundtrip_with_hardware_read_masks() {
        let mut apu = Apu::new();

        apu.write(0xFF10, 0x7F);
        apu.write(0xFF11, 0x80);
        apu.write(0xFF12, 0xF3);
        apu.write(0xFF13, 0xAA);
        apu.write(0xFF14, 0x47);
        apu.write(0xFF24, 0x76);
        apu.write(0xFF25, 0xF3);

        assert_eq!(
            apu.read(0xFF10),
            0xFF,
            "NR10 should keep sweep bits and read unused bit 7 high"
        );
        assert_eq!(
            apu.read(0xFF11),
            0xBF,
            "NR11 reads duty bits and masks length bits high"
        );
        assert_eq!(apu.read(0xFF12), 0xF3, "NR12 should roundtrip");
        assert_eq!(apu.read(0xFF13), 0xFF, "NR13 is write-only");
        assert_eq!(
            apu.read(0xFF14),
            0xFF,
            "NR14 should read trigger and period bits as high"
        );
        assert_eq!(apu.read(0xFF24), 0x76, "NR50 should roundtrip");
        assert_eq!(apu.read(0xFF25), 0xF3, "NR51 should roundtrip");
    }

    #[test]
    fn nr52_power_control_disables_channels_and_ignores_register_writes() {
        let mut apu = Apu::new();

        apu.write(0xFF12, 0xF0);
        apu.write(0xFF14, 0x80);
        assert_eq!(
            apu.read(0xFF26) & 0x81,
            0x81,
            "NR52 should report power and active channel 1"
        );

        apu.write(0xFF26, 0x00);
        apu.write(0xFF12, 0xF0);

        assert_eq!(apu.read(0xFF26), 0x70, "NR52 bit 7 should clear");
        assert_eq!(
            apu.read(0xFF12),
            0x00,
            "powered-off APU should ignore ordinary register writes"
        );
    }

    #[test]
    fn frame_sequencer_clocks_length_counters() {
        let mut apu = Apu::new();
        apu.write(0xFF11, 0x3F);
        apu.write(0xFF12, 0xF0);
        apu.write(0xFF14, 0xC0);

        apu.tick(TCycles(FRAME_SEQUENCER_PERIOD));

        assert_eq!(
            apu.read(0xFF26) & 0x01,
            0x00,
            "length-enabled channel with one remaining tick should be disabled at sequencer step 0"
        );
    }

    #[test]
    fn square_channel_generates_samples() {
        let mut apu = Apu::new();
        apu.write(0xFF11, 0x80);
        apu.write(0xFF12, 0xF0);
        apu.write(0xFF13, 0x00);
        apu.write(0xFF14, 0x80);

        apu.tick(TCycles(4_194_304 / 60));

        assert!(
            !apu.samples().is_empty(),
            "APU should produce frontend-drainable samples"
        );
        assert!(
            apu.samples()
                .iter()
                .any(|sample| sample.left != 0 || sample.right != 0),
            "triggered square channel should produce non-silent PCM"
        );
    }

    #[test]
    fn wave_ram_roundtrips_and_wave_channel_outputs_samples() {
        let mut apu = Apu::new();
        apu.write(0xFF30, 0xF0);
        apu.write(0xFF1A, 0x80);
        apu.write(0xFF1C, 0x20);
        apu.write(0xFF1E, 0x80);
        apu.tick(TCycles(4_194_304 / 120));

        assert_eq!(apu.read(0xFF30), 0xF0, "wave RAM should roundtrip");
        assert!(
            apu.samples()
                .iter()
                .any(|sample| sample.left != 0 || sample.right != 0),
            "triggered wave channel should contribute audio samples"
        );
    }

    #[test]
    fn noise_lfsr_output_is_deterministic() {
        let mut left = Apu::new();
        let mut right = Apu::new();

        for apu in [&mut left, &mut right] {
            apu.write(0xFF21, 0xF0);
            apu.write(0xFF22, 0x00);
            apu.write(0xFF23, 0x80);
            apu.tick(TCycles(4_194_304 / 120));
        }

        assert_eq!(
            left.samples(),
            right.samples(),
            "same noise register writes and cycle count should produce deterministic PCM"
        );
    }

    #[test]
    fn take_samples_drains_generated_audio() {
        let mut apu = Apu::new();
        apu.write(0xFF12, 0xF0);
        apu.write(0xFF14, 0x80);
        apu.tick(TCycles(4_194_304 / AUDIO_SAMPLE_RATE + 1));

        assert!(
            !apu.take_samples().is_empty(),
            "first take should return PCM"
        );
        assert!(
            apu.take_samples().is_empty(),
            "second take should be drained"
        );
    }
}
