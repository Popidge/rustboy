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
            return self.wave.peek_wave_ram(address - WAVE_RAM_START);
        }

        match address {
            NR10 => self.ch1.nr0 | 0x80,
            NR11 => self.ch1.nr1 | 0x3F,
            NR12 => self.ch1.nr2,
            NR14 => self.ch1.nr4 | 0xBF,
            NR21 => self.ch2.nr1 | 0x3F,
            NR22 => self.ch2.nr2,
            NR24 => self.ch2.nr4 | 0xBF,
            NR30 => (self.wave.nr0 & 0x80) | 0x7F,
            NR32 => self.wave.nr2 | 0x9F,
            NR34 => self.wave.nr4 | 0xBF,
            NR42 => self.noise.nr2,
            NR43 => self.noise.nr3,
            NR44 => self.noise.nr4 | 0xBF,
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

    pub fn read_cpu(&mut self, address: u16) -> u8 {
        if matches!(address, WAVE_RAM_START..=WAVE_RAM_END) {
            return self.wave.read_wave_ram(address - WAVE_RAM_START);
        }

        self.read(address)
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
            match address {
                NR11 => self.ch1.write_length_while_powered_off(value),
                NR21 => self.ch2.write_length_while_powered_off(value),
                NR31 => self.wave.write_length(value),
                NR41 => self.noise.write_length(value),
                _ => {}
            }
            return;
        }

        match address {
            NR10 => self.ch1.write_sweep(value),
            NR11 => self.ch1.write_duty_length(value),
            NR12 => self.ch1.write_envelope(value),
            NR13 => self.ch1.write_period_low(value),
            NR14 => self
                .ch1
                .write_control(value, should_clock_length_on_enable(self.frame_step)),
            NR21 => self.ch2.write_duty_length(value),
            NR22 => self.ch2.write_envelope(value),
            NR23 => self.ch2.write_period_low(value),
            NR24 => self
                .ch2
                .write_control(value, should_clock_length_on_enable(self.frame_step)),
            NR30 => self.wave.write_dac(value),
            NR31 => self.wave.write_length(value),
            NR32 => self.wave.write_output_level(value),
            NR33 => self.wave.write_period_low(value),
            NR34 => self
                .wave
                .write_control(value, should_clock_length_on_enable(self.frame_step)),
            NR41 => self.noise.write_length(value),
            NR42 => self.noise.write_envelope(value),
            NR43 => self.noise.write_polynomial(value),
            NR44 => self
                .noise
                .write_control(value, should_clock_length_on_enable(self.frame_step)),
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

fn should_clock_length_on_enable(frame_step: u8) -> bool {
    frame_step % 2 == 1
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
    sweep_negate_calculated: bool,
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
            sweep_negate_calculated: false,
        }
    }

    fn power_off(&mut self) {
        let has_sweep = self.has_sweep;
        *self = Self::new(has_sweep);
    }

    fn write_sweep(&mut self, value: u8) {
        if self.has_sweep {
            if self.enabled
                && self.sweep_negate_calculated
                && self.nr0 & 0x08 != 0
                && value & 0x08 == 0
            {
                self.enabled = false;
            }
            self.nr0 = value & 0x7F;
        }
    }

    fn write_duty_length(&mut self, value: u8) {
        self.nr1 = value;
        self.length_counter = 64 - u16::from(value & 0x3F);
    }

    fn write_length_while_powered_off(&mut self, value: u8) {
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

    fn write_period_low(&mut self, value: u8) {
        self.nr3 = value;
    }

    fn write_control(&mut self, value: u8, clock_length_on_enable: bool) {
        let length_was_disabled = self.nr4 & 0x40 == 0;
        self.nr4 = value & 0xC7;
        if length_was_disabled
            && value & 0x40 != 0
            && clock_length_on_enable
            && self.length_counter > 0
        {
            self.tick_length();
        }
        let length_was_zero = self.length_counter == 0;
        if value & 0x80 != 0 {
            self.trigger();
            if value & 0x40 != 0 && length_was_zero && clock_length_on_enable {
                self.tick_length();
            }
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

        self.sweep_timer = self.sweep_timer_period();
        if self.sweep_period() == 0 {
            return;
        }
        let shift = self.nr0 & 0x07;

        if self.nr0 & 0x08 != 0 {
            self.sweep_negate_calculated = true;
        }
        if let Some(new_period) = self.calculate_sweep_period() {
            if shift == 0 {
                return;
            }
            self.set_period(new_period);
            self.shadow_period = new_period;
            if self.calculate_sweep_period().is_none() {
                self.enabled = false;
            }
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
            self.sweep_timer = self.sweep_timer_period();
            self.sweep_enabled = self.sweep_period() != 0 || self.sweep_shift() != 0;
            self.sweep_negate_calculated = false;
            if self.nr0 & 0x08 != 0 && self.sweep_shift() != 0 {
                self.sweep_negate_calculated = true;
            }
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

    fn sweep_timer_period(&self) -> u8 {
        match self.sweep_period() {
            0 => 8,
            period => period,
        }
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
    playback_byte: u8,
    wave_ram_just_read: bool,
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
            playback_byte: 0,
            wave_ram_just_read: false,
            wave_ram: [0; WAVE_RAM_SIZE],
        }
    }

    fn power_off(&mut self) {
        let wave_ram = self.wave_ram;
        *self = Self::new();
        self.wave_ram = wave_ram;
    }

    fn read_wave_ram(&mut self, offset: u16) -> u8 {
        if self.enabled {
            if self.wave_ram_just_read {
                self.wave_ram_just_read = false;
                self.wave_ram[self.sample_index / 2]
            } else {
                0xFF
            }
        } else {
            self.wave_ram[usize::from(offset)]
        }
    }

    fn peek_wave_ram(&self, offset: u16) -> u8 {
        if self.enabled {
            if self.wave_ram_just_read {
                self.wave_ram[self.sample_index / 2]
            } else {
                0xFF
            }
        } else {
            self.wave_ram[usize::from(offset)]
        }
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

    fn write_control(&mut self, value: u8, clock_length_on_enable: bool) {
        let length_was_disabled = self.nr4 & 0x40 == 0;
        self.nr4 = value & 0xC7;
        if length_was_disabled
            && value & 0x40 != 0
            && clock_length_on_enable
            && self.length_counter > 0
        {
            self.tick_length();
        }
        let length_was_zero = self.length_counter == 0;
        if value & 0x80 != 0 {
            self.trigger();
            if value & 0x40 != 0 && length_was_zero && clock_length_on_enable {
                self.tick_length();
            }
        }
    }

    fn tick(&mut self, cycles: u32) {
        if !self.enabled {
            return;
        }

        self.wave_ram_just_read = false;
        let mut remaining = cycles;
        while remaining >= self.timer {
            remaining -= self.timer;
            self.sample_index = (self.sample_index + 1) & 0x1F;
            self.playback_byte = self.wave_ram[self.sample_index / 2];
            self.wave_ram_just_read = remaining == 0;
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

        let byte = self.playback_byte;
        let sample = if self.sample_index & 1 == 1 {
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
        self.playback_byte = self.wave_ram[0];
        self.wave_ram_just_read = false;
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

    fn write_control(&mut self, value: u8, clock_length_on_enable: bool) {
        let length_was_disabled = self.nr4 & 0x40 == 0;
        self.nr4 = value & 0xC0;
        if length_was_disabled
            && value & 0x40 != 0
            && clock_length_on_enable
            && self.length_counter > 0
        {
            self.tick_length();
        }
        let length_was_zero = self.length_counter == 0;
        if value & 0x80 != 0 {
            self.trigger();
            if value & 0x40 != 0 && length_was_zero && clock_length_on_enable {
                self.tick_length();
            }
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
        self.timer = (self.timer_reload() / 2).max(1);
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
            "NR14 should preserve length and period-high bits while masking trigger high"
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
    fn dmg_length_registers_remain_writable_while_powered_off() {
        let mut apu = Apu::new();

        apu.write(0xFF26, 0x00);
        apu.write(0xFF11, 0xC1);
        apu.write(0xFF16, 0x80);
        apu.write(0xFF1B, 0x02);
        apu.write(0xFF20, 0x03);
        apu.write(0xFF26, 0x80);

        assert_eq!(
            apu.ch1.nr1, 0x01,
            "powered-off NR11 writes should keep only the length bits"
        );
        assert_eq!(
            apu.ch2.nr1, 0x00,
            "powered-off NR21 writes should not preserve square duty bits"
        );
        assert_eq!(
            apu.wave.length_counter, 254,
            "powered-off NR31 writes should update wave length"
        );
        assert_eq!(
            apu.noise.length_counter, 61,
            "powered-off NR41 writes should update noise length"
        );
    }

    #[test]
    fn wave_ram_survives_apu_power_cycle() {
        let mut apu = Apu::new();

        apu.write(0xFF30, 0x37);
        apu.write(0xFF26, 0x00);
        apu.write(0xFF26, 0x80);

        assert_eq!(
            apu.read(0xFF30),
            0x37,
            "APU power cycling should not clear wave RAM"
        );
    }

    #[test]
    fn sweep_period_zero_reloads_timer_as_eight() {
        let mut apu = Apu::new();

        apu.write(0xFF10, 0x01);
        apu.write(0xFF12, 0x08);
        apu.write(0xFF14, 0x80);

        assert_eq!(
            apu.ch1.sweep_timer, 8,
            "sweep period 0 should reload the sweep timer as 8"
        );
    }

    #[test]
    fn sweep_second_overflow_check_disables_channel_after_update() {
        let mut apu = Apu::new();

        apu.write(0xFF10, 0x11);
        apu.write(0xFF12, 0x08);
        apu.write(0xFF13, 0x00);
        apu.write(0xFF14, 0x85);
        apu.ch1.tick_sweep();

        assert_eq!(
            apu.read(0xFF26) & 0x01,
            0,
            "second sweep overflow check should disable channel 1"
        );
    }

    #[test]
    fn sweep_exiting_negate_after_calculation_disables_channel() {
        let mut apu = Apu::new();

        apu.write(0xFF10, 0x09);
        apu.write(0xFF12, 0x08);
        apu.write(0xFF14, 0x80);
        assert_eq!(apu.read(0xFF26) & 0x01, 0x01);

        apu.write(0xFF10, 0x10);

        assert_eq!(
            apu.read(0xFF26) & 0x01,
            0,
            "leaving negate mode after a sweep subtraction calculation should disable channel 1"
        );
    }

    #[test]
    fn register_reads_match_blargg_dmg_sound_masks() {
        const MASKS: [u8; 0x30] = [
            0x80, 0x3F, 0x00, 0xFF, 0xBF, 0xFF, 0x3F, 0x00, 0xFF, 0xBF, 0x7F, 0xFF, 0x9F, 0xFF,
            0xBF, 0xFF, 0xFF, 0x00, 0x00, 0xBF, 0x00, 0x00, 0x70, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let mut apu = Apu::new();

        for value in 0..=u8::MAX {
            for (offset, mask) in MASKS.iter().copied().enumerate() {
                let address = 0xFF10 + u16::try_from(offset).expect("offset fits");
                if address == 0xFF26 {
                    continue;
                }

                apu.write(address, value);
                assert_eq!(
                    apu.read(address),
                    value | mask,
                    "APU read mask mismatch at 0x{address:04X} after writing 0x{value:02X}"
                );

                apu.write(0xFF25, 0);
                apu.write(0xFF1A, 0);
            }
        }
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
        assert_eq!(apu.read(0xFF30), 0xF0, "inactive wave RAM should roundtrip");
        apu.write(0xFF1A, 0x80);
        apu.write(0xFF1C, 0x20);
        apu.write(0xFF1E, 0x80);
        apu.tick(TCycles(4_194_304 / 120));

        assert!(
            apu.samples()
                .iter()
                .any(|sample| sample.left != 0 || sample.right != 0),
            "triggered wave channel should contribute audio samples"
        );
    }

    #[test]
    fn active_wave_ram_reads_only_after_internal_wave_read() {
        let mut apu = Apu::new();
        apu.write(0xFF30, 0x12);
        apu.write(0xFF31, 0x34);
        apu.write(0xFF1A, 0x80);
        apu.write(0xFF1C, 0x20);
        apu.write(0xFF1D, 0xFF);
        apu.write(0xFF1E, 0x87);

        assert_eq!(
            apu.read(0xFF31),
            0xFF,
            "active DMG wave RAM reads should be locked until an internal fetch"
        );

        apu.tick(TCycles(2));
        assert_eq!(
            apu.read(0xFF31),
            0x12,
            "active DMG wave RAM reads should expose the current byte immediately after an internal wave read"
        );

        apu.tick(TCycles(2));

        assert_eq!(
            apu.read(0xFF30),
            0x34,
            "active wave RAM reads should follow the current byte after another internal wave read"
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
