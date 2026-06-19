//! Address bus and basic memory map routing for the DMG Game Boy.
//!
//! The CPU talks to memory-mapped hardware through this type. At this stage the
//! bus owns the cartridge plus WRAM/HRAM and interrupt registers; later
//! milestones will add PPU, timer, serial, joypad, and APU routing.

use crate::{
    apu::{Apu, StereoSample},
    cartridge::Cartridge,
    cpu::TCycles,
    interrupt::{Interrupt, InterruptFlags},
    joypad::{Button, Joypad},
    ppu::{Ppu, FRAMEBUFFER_PIXELS},
    serial::Serial,
    timer::Timer,
};

const WRAM_START: u16 = 0xC000;
const WRAM_END: u16 = 0xDFFF;
const WRAM_SIZE: usize = 0x2000;
const ECHO_RAM_START: u16 = 0xE000;
const ECHO_RAM_END: u16 = 0xFDFF;
const ECHO_RAM_OFFSET: u16 = 0x2000;

const HRAM_START: u16 = 0xFF80;
const HRAM_END: u16 = 0xFFFE;
const HRAM_SIZE: usize = 0x007F;

const INTERRUPT_ENABLE_ADDR: u16 = 0xFFFF;
const INTERRUPT_FLAGS_ADDR: u16 = 0xFF0F;
const JOYPAD_ADDR: u16 = 0xFF00;
const SERIAL_START: u16 = 0xFF01;
const SERIAL_END: u16 = 0xFF02;
const TIMER_START: u16 = 0xFF04;
const TIMER_END: u16 = 0xFF07;
const VRAM_START: u16 = 0x8000;
const VRAM_END: u16 = 0x9FFF;
const CARTRIDGE_RAM_START: u16 = 0xA000;
const CARTRIDGE_RAM_END: u16 = 0xBFFF;
const OAM_START: u16 = 0xFE00;
const OAM_END: u16 = 0xFE9F;
const OAM_SIZE: u8 = 0xA0;
const UNUSABLE_OAM_START: u16 = 0xFEA0;
const UNUSABLE_OAM_END: u16 = 0xFEFF;
const DMA_ADDR: u16 = 0xFF46;
const PPU_REGISTER_START: u16 = 0xFF40;
const PPU_REGISTER_END: u16 = 0xFF4B;
const CPU_MACHINE_CYCLE: TCycles = TCycles(4);

/// The CPU-visible operation represented by a test-only bus-cycle record.
///
/// The trace deliberately describes bus operations rather than CPU opcodes, so
/// timing investigations can compare a failing run at the hardware boundary.
#[cfg(any(test, feature = "test-trace"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusCycleKind {
    OpcodeFetch,
    Read,
    Write,
    Idle,
}

#[cfg(not(any(test, feature = "test-trace")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BusCycleKind {
    OpcodeFetch,
    Read,
    Write,
    Idle,
}

/// Small state snapshot captured at the beginning of a CPU machine cycle.
#[cfg(any(test, feature = "test-trace"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusTraceState {
    pub ppu_ly: u8,
    pub ppu_mode: crate::ppu::PpuMode,
    pub interrupt_flags: u8,
    pub interrupt_enable: u8,
    pub timer_tima: u8,
    pub timer_tma: u8,
    /// Remaining T-cycles before the pending TIMA reload, if any.
    pub timer_overflow_delay: Option<u8>,
    pub dma_active: bool,
    pub dma_next_byte: u8,
    pub dma_startup_mcycles: u8,
}

/// One deterministic CPU bus-cycle record for timing tests.
///
/// `tcycle` is the monotonic bus time at the beginning of the operation. The
/// snapshot is taken after the read or write transfer, but before that machine
/// cycle advances the four T-cycles of bus-owned hardware.
#[cfg(any(test, feature = "test-trace"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusCycleRecord {
    pub tcycle: u64,
    pub kind: BusCycleKind,
    pub address: Option<u16>,
    pub value: Option<u8>,
    pub state: BusTraceState,
}

/// One component position in the Bus-owned T-cycle dispatcher.
///
/// A single DMG T-cycle advances these components in this fixed order. Keeping
/// the order here makes timing changes reviewable instead of letting component
/// calls leak into CPU execution paths.
#[cfg(any(test, feature = "test-trace"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusDispatchStage {
    Timer,
    Ppu,
    Apu,
    OamDma,
    Serial,
}

#[cfg(not(any(test, feature = "test-trace")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BusDispatchStage {
    Timer,
    Ppu,
    Apu,
    OamDma,
    Serial,
}

/// A test-only record of one component invocation within a Bus T-cycle.
///
/// The snapshot is captured immediately after `stage` advances. Five records
/// with the same `tcycle` therefore describe the complete dispatcher order.
#[cfg(any(test, feature = "test-trace"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusTcycleRecord {
    pub tcycle: u64,
    pub stage: BusDispatchStage,
    pub state: BusTraceState,
}

/// CPU-facing memory bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bus {
    cartridge: Cartridge,
    ppu: Ppu,
    joypad: Joypad,
    serial: Serial,
    timer: Timer,
    apu: Apu,
    wram: [u8; WRAM_SIZE],
    hram: [u8; HRAM_SIZE],
    interrupt_enable: InterruptFlags,
    interrupt_flags: InterruptFlags,
    stop_wake_requested: bool,
    oam_dma: OamDma,
    clocked_cpu_cycles: TCycles,
    elapsed_tcycles: u64,
    #[cfg(any(test, feature = "test-trace"))]
    cycle_trace: Vec<BusCycleRecord>,
    #[cfg(any(test, feature = "test-trace"))]
    tcycle_trace: Vec<BusTcycleRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OamDma {
    source_high: u8,
    next_byte: u8,
    startup_mcycles: u8,
    active: bool,
    elapsed_tcycles: u32,
}

impl OamDma {
    const fn inactive() -> Self {
        Self {
            source_high: 0,
            next_byte: 0,
            startup_mcycles: 0,
            active: false,
            elapsed_tcycles: 0,
        }
    }

    fn start(&mut self, source_high: u8) {
        self.source_high = source_high;
        self.next_byte = 0;
        self.startup_mcycles = 2;
        self.active = true;
        self.elapsed_tcycles = 0;
    }

    fn is_active(self) -> bool {
        self.active
    }

    fn source_address(self) -> u16 {
        (u16::from(self.source_high) << 8) | u16::from(self.next_byte)
    }

    fn complete_byte(&mut self) {
        self.next_byte = self.next_byte.wrapping_add(1);

        if self.next_byte == OAM_SIZE {
            self.active = false;
            self.elapsed_tcycles = 0;
        }
    }
}

impl Bus {
    /// Creates a bus that owns the loaded cartridge and empty internal memory.
    #[must_use]
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            cartridge,
            ppu: Ppu::new(),
            joypad: Joypad::new(),
            serial: Serial::new(),
            timer: Timer::new(),
            apu: Apu::new(),
            wram: [0; WRAM_SIZE],
            hram: [0; HRAM_SIZE],
            interrupt_enable: InterruptFlags::default(),
            interrupt_flags: InterruptFlags::default(),
            stop_wake_requested: false,
            oam_dma: OamDma::inactive(),
            clocked_cpu_cycles: TCycles(0),
            elapsed_tcycles: 0,
            #[cfg(any(test, feature = "test-trace"))]
            cycle_trace: Vec::new(),
            #[cfg(any(test, feature = "test-trace"))]
            tcycle_trace: Vec::new(),
        }
    }

    /// Reads one byte from the CPU address space.
    ///
    /// Unsupported regions return `0xFF` until their hardware components exist.
    #[must_use]
    pub fn read8(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x7FFF => self.cartridge.read_rom(address).unwrap_or(0xFF),
            VRAM_START..=VRAM_END => self.ppu.read_vram(address - VRAM_START),
            CARTRIDGE_RAM_START..=CARTRIDGE_RAM_END => self.cartridge.read_ram(address),
            WRAM_START..=WRAM_END => self.wram[wram_index(address)],
            ECHO_RAM_START..=ECHO_RAM_END => self.wram[wram_index(address - ECHO_RAM_OFFSET)],
            OAM_START..=OAM_END => self.ppu.read_oam(address - OAM_START),
            UNUSABLE_OAM_START..=UNUSABLE_OAM_END => 0xFF,
            JOYPAD_ADDR => self.joypad.read(),
            SERIAL_START..=SERIAL_END => self.serial.read(address),
            TIMER_START..=TIMER_END => self.timer.read(address),
            INTERRUPT_FLAGS_ADDR => self.interrupt_flags.read_if(),
            0xFF10..=0xFF3F => self.apu.read(address),
            DMA_ADDR => self.oam_dma.source_high,
            PPU_REGISTER_START..=PPU_REGISTER_END => self.ppu.read_register(address),
            HRAM_START..=HRAM_END => self.hram[hram_index(address)],
            INTERRUPT_ENABLE_ADDR => self.interrupt_enable.raw(),
            _ => 0xFF,
        }
    }

    /// Writes one byte into the CPU address space.
    ///
    /// Writes to ROM and unsupported regions are ignored for now.
    pub fn write8(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x7FFF => self.cartridge.write_rom(address, value),
            VRAM_START..=VRAM_END => self.ppu.write_vram(address - VRAM_START, value),
            CARTRIDGE_RAM_START..=CARTRIDGE_RAM_END => self.cartridge.write_ram(address, value),
            WRAM_START..=WRAM_END => self.wram[wram_index(address)] = value,
            ECHO_RAM_START..=ECHO_RAM_END => {
                self.wram[wram_index(address - ECHO_RAM_OFFSET)] = value;
            }
            OAM_START..=OAM_END => self.ppu.write_oam(address - OAM_START, value),
            UNUSABLE_OAM_START..=UNUSABLE_OAM_END => {}
            JOYPAD_ADDR => self.joypad.write(value),
            SERIAL_START..=SERIAL_END => {
                self.serial
                    .write_with_clock_phase(address, value, self.timer.serial_clock_phase());
            }
            TIMER_START..=TIMER_END => self.timer.write(address, value),
            INTERRUPT_FLAGS_ADDR => self.interrupt_flags.write_if(value),
            0xFF10..=0xFF3F => self.apu.write(address, value),
            DMA_ADDR => self.oam_dma.start(value),
            PPU_REGISTER_START..=PPU_REGISTER_END => self.ppu.write_register(address, value),
            HRAM_START..=HRAM_END => self.hram[hram_index(address)] = value,
            INTERRUPT_ENABLE_ADDR => self.interrupt_enable.set_raw(value),
            _ => {}
        }
    }

    /// Reads a little-endian 16-bit value from the CPU address space.
    #[must_use]
    pub fn read16(&self, address: u16) -> u16 {
        let low = self.read8(address);
        let high = self.read8(address.wrapping_add(1));

        u16::from_le_bytes([low, high])
    }

    /// Writes a little-endian 16-bit value into the CPU address space.
    pub fn write16(&mut self, address: u16, value: u16) {
        let [low, high] = value.to_le_bytes();

        self.write8(address, low);
        self.write8(address.wrapping_add(1), high);
    }

    /// Advances bus-owned hardware components by the given T-cycles.
    ///
    /// Every iteration is one DMG T-cycle. Hardware advances in timer, PPU,
    /// APU, OAM-DMA, then Serial order; interrupt requests raised by an earlier
    /// stage are visible to the stages that follow. CPU code reaches this
    /// dispatcher only through the clocked CPU bus helpers below.
    pub fn tick(&mut self, cycles: TCycles) {
        for _ in 0..cycles.0 {
            self.tick_tcycle();
        }
    }

    /// Starts accounting for CPU bus cycles during one logical CPU step.
    pub(crate) fn begin_cpu_step(&mut self) {
        self.clocked_cpu_cycles = TCycles(0);
    }

    /// Returns how many T-cycles were already advanced by clocked CPU bus access.
    #[must_use]
    pub(crate) fn clocked_cpu_cycles(&self) -> TCycles {
        self.clocked_cpu_cycles
    }

    /// Fetches an opcode byte for the CPU and advances one machine cycle.
    pub fn cpu_fetch8(&mut self, address: u16) -> u8 {
        let value = self.cpu_read8_during_dma(address);
        self.record_cycle(BusCycleKind::OpcodeFetch, Some(address), Some(value));
        self.advance_cpu_mcycle();
        value
    }

    /// Reads one byte for the CPU and advances one machine cycle.
    pub fn cpu_read8(&mut self, address: u16) -> u8 {
        let value = self.cpu_read8_during_dma(address);
        self.record_cycle(BusCycleKind::Read, Some(address), Some(value));
        self.advance_cpu_mcycle();
        value
    }

    /// Writes one byte for the CPU and advances one machine cycle.
    pub fn cpu_write8(&mut self, address: u16, value: u8) {
        self.cpu_write8_during_dma(address, value);
        self.record_cycle(BusCycleKind::Write, Some(address), Some(value));
        self.advance_cpu_mcycle();
    }

    /// Advances one internal CPU machine cycle without a memory transfer.
    pub fn cpu_idle_mcycle(&mut self) {
        self.record_cycle(BusCycleKind::Idle, None, None);
        self.advance_cpu_mcycle();
    }

    fn advance_cpu_mcycle(&mut self) {
        self.tick(CPU_MACHINE_CYCLE);
        self.clocked_cpu_cycles.0 += CPU_MACHINE_CYCLE.0;
    }

    fn tick_tcycle(&mut self) {
        self.timer.tick(TCycles(1), &mut self.interrupt_flags);
        self.record_tcycle_stage(BusDispatchStage::Timer);

        self.ppu.tick(TCycles(1), &mut self.interrupt_flags);
        self.record_tcycle_stage(BusDispatchStage::Ppu);

        self.apu.tick(TCycles(1));
        self.record_tcycle_stage(BusDispatchStage::Apu);

        self.tick_oam_dma(TCycles(1));
        self.record_tcycle_stage(BusDispatchStage::OamDma);

        self.serial.tick(TCycles(1), &mut self.interrupt_flags);
        self.record_tcycle_stage(BusDispatchStage::Serial);

        self.elapsed_tcycles += 1;
    }

    /// Returns the accumulated test-only CPU bus-cycle trace without draining it.
    #[cfg(any(test, feature = "test-trace"))]
    #[must_use]
    pub fn cycle_trace(&self) -> &[BusCycleRecord] {
        &self.cycle_trace
    }

    /// Drains the accumulated test-only CPU bus-cycle trace.
    #[cfg(any(test, feature = "test-trace"))]
    pub fn take_cycle_trace(&mut self) -> Vec<BusCycleRecord> {
        std::mem::take(&mut self.cycle_trace)
    }

    /// Returns the test-only one-T-cycle dispatcher trace without draining it.
    #[cfg(any(test, feature = "test-trace"))]
    #[must_use]
    pub fn tcycle_trace(&self) -> &[BusTcycleRecord] {
        &self.tcycle_trace
    }

    /// Drains the test-only one-T-cycle dispatcher trace.
    #[cfg(any(test, feature = "test-trace"))]
    pub fn take_tcycle_trace(&mut self) -> Vec<BusTcycleRecord> {
        std::mem::take(&mut self.tcycle_trace)
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32; FRAMEBUFFER_PIXELS] {
        self.ppu.framebuffer()
    }

    #[must_use]
    pub fn frame_ready(&self) -> bool {
        self.ppu.frame_ready()
    }

    pub fn take_frame_ready(&mut self) -> bool {
        self.ppu.take_frame_ready()
    }

    /// Returns collected serial debug output without draining it.
    #[must_use]
    pub fn serial_output(&self) -> &[u8] {
        self.serial.output()
    }

    /// Drains collected serial debug output.
    pub fn take_serial_output(&mut self) -> Vec<u8> {
        self.serial.take_output()
    }

    #[must_use]
    pub fn audio_samples(&self) -> &[StereoSample] {
        self.apu.samples()
    }

    pub fn take_audio_samples(&mut self) -> Vec<StereoSample> {
        self.apu.take_samples()
    }

    /// Updates one joypad button state and requests an interrupt on new presses.
    pub fn set_button(&mut self, button: Button, pressed: bool) {
        let was_pressed = self.joypad.is_pressed(button);
        self.joypad
            .set_button(button, pressed, &mut self.interrupt_flags);

        // On DMG, a new joypad press is a wake source for STOP regardless of
        // whether the Joypad interrupt is enabled.
        if pressed && !was_pressed {
            self.stop_wake_requested = true;
        }
    }

    #[must_use]
    pub fn save_ram(&self) -> Option<&[u8]> {
        self.cartridge.save_ram()
    }

    /// Restores external cartridge RAM from save data.
    ///
    /// # Errors
    ///
    /// Returns a save RAM error when the data cannot be applied to the loaded
    /// cartridge.
    pub fn load_save_ram(&mut self, data: &[u8]) -> Result<(), crate::cartridge::SaveRamError> {
        self.cartridge.load_save_ram(data)
    }

    /// Returns the raw interrupt flags register storage.
    #[must_use]
    pub fn interrupt_flags(&self) -> u8 {
        self.interrupt_flags.raw()
    }

    /// Returns the raw interrupt enable register storage.
    #[must_use]
    pub fn interrupt_enable(&self) -> u8 {
        self.interrupt_enable.raw()
    }

    /// Requests an interrupt in the IF register.
    pub fn request_interrupt(&mut self, interrupt: Interrupt) {
        self.interrupt_flags.request(interrupt);
    }

    /// Clears an interrupt request from the IF register.
    pub fn clear_interrupt(&mut self, interrupt: Interrupt) {
        self.interrupt_flags.clear(interrupt);
    }

    /// Consumes a pending DMG STOP wake caused by a new joypad press.
    pub(crate) fn take_stop_wake_request(&mut self) -> bool {
        std::mem::take(&mut self.stop_wake_requested)
    }

    #[cfg(any(test, feature = "test-trace"))]
    fn record_cycle(&mut self, kind: BusCycleKind, address: Option<u16>, value: Option<u8>) {
        self.cycle_trace.push(BusCycleRecord {
            tcycle: self.elapsed_tcycles,
            kind,
            address,
            value,
            state: self.trace_state(),
        });
    }

    #[cfg(not(any(test, feature = "test-trace")))]
    fn record_cycle(&mut self, _kind: BusCycleKind, _address: Option<u16>, _value: Option<u8>) {}

    #[cfg(any(test, feature = "test-trace"))]
    fn record_tcycle_stage(&mut self, stage: BusDispatchStage) {
        self.tcycle_trace.push(BusTcycleRecord {
            tcycle: self.elapsed_tcycles,
            stage,
            state: self.trace_state(),
        });
    }

    #[cfg(not(any(test, feature = "test-trace")))]
    fn record_tcycle_stage(&mut self, _stage: BusDispatchStage) {}

    #[cfg(any(test, feature = "test-trace"))]
    fn trace_state(&self) -> BusTraceState {
        let (ppu_ly, ppu_mode) = self.ppu.trace_ly_and_mode();
        let timer = self.timer.trace_state();

        BusTraceState {
            ppu_ly,
            ppu_mode,
            interrupt_flags: self.interrupt_flags.raw(),
            interrupt_enable: self.interrupt_enable.raw(),
            timer_tima: timer.tima,
            timer_tma: timer.tma,
            timer_overflow_delay: timer.overflow_delay,
            dma_active: self.oam_dma.is_active(),
            dma_next_byte: self.oam_dma.next_byte,
            dma_startup_mcycles: self.oam_dma.startup_mcycles,
        }
    }

    /// Returns the highest-priority interrupt that is both enabled and requested.
    #[must_use]
    pub fn pending_interrupt(&self) -> Option<Interrupt> {
        InterruptFlags::first_pending(self.interrupt_enable, self.interrupt_flags)
    }

    fn cpu_read8_during_dma(&self, address: u16) -> u8 {
        if self.oam_dma.is_active() && cpu_oam_dma_blocks_address(address) {
            return 0xFF;
        }

        self.read8(address)
    }

    fn cpu_write8_during_dma(&mut self, address: u16, value: u8) {
        if self.oam_dma.is_active() && cpu_oam_dma_blocks_address(address) {
            return;
        }

        self.write8(address, value);
    }

    fn tick_oam_dma(&mut self, cycles: TCycles) {
        if !self.oam_dma.is_active() {
            return;
        }

        self.oam_dma.elapsed_tcycles += cycles.0;

        while self.oam_dma.is_active() && self.oam_dma.elapsed_tcycles >= CPU_MACHINE_CYCLE.0 {
            self.oam_dma.elapsed_tcycles -= CPU_MACHINE_CYCLE.0;

            if self.oam_dma.startup_mcycles > 0 {
                self.oam_dma.startup_mcycles -= 1;
                continue;
            }

            let source_address = self.oam_dma.source_address();
            let destination_offset = u16::from(self.oam_dma.next_byte);
            let byte = self.read_oam_dma_source(source_address);

            self.ppu.write_oam(destination_offset, byte);
            self.oam_dma.complete_byte();
        }
    }

    fn read_oam_dma_source(&self, address: u16) -> u8 {
        let address = if address >= ECHO_RAM_START {
            address - ECHO_RAM_OFFSET
        } else {
            address
        };

        self.read8(address)
    }
}

fn cpu_oam_dma_blocks_address(address: u16) -> bool {
    // During active DMG OAM DMA, the CPU bus is owned by the transfer engine.
    // HRAM remains reachable because it has its own internal memory path; FF46
    // is retained as an explicit exception so software can restart DMA.
    !matches!(address, HRAM_START..=HRAM_END | DMA_ADDR)
}

fn wram_index(address: u16) -> usize {
    usize::from(address - WRAM_START)
}

fn hram_index(address: u16) -> usize {
    usize::from(address - HRAM_START)
}

#[cfg(test)]
mod tests {
    use super::{Bus, BusCycleKind, BusDispatchStage};
    use crate::{cartridge::Cartridge, cpu::TCycles, ppu::PpuMode};

    fn test_bus() -> Bus {
        let mut rom = vec![0; 0x8000];
        rom[0x0147] = 0x00;
        rom[0x0148] = 0x00;
        rom[0x0149] = 0x00;
        rom[0x014D] = header_checksum(&rom);

        Bus::new(Cartridge::from_bytes(rom).expect("minimal ROM should load"))
    }

    fn header_checksum(rom: &[u8]) -> u8 {
        rom[0x0134..=0x014C].iter().fold(0_u8, |checksum, byte| {
            checksum.wrapping_sub(*byte).wrapping_sub(1)
        })
    }

    #[test]
    fn cycle_trace_records_ordered_cpu_cycles_and_hardware_snapshots() {
        let mut bus = test_bus();
        bus.write8(0xFF05, 0xFF);
        bus.write8(0xFF06, 0x42);
        bus.write8(0xFF07, 0x05);

        for _ in 0..4 {
            bus.cpu_idle_mcycle();
        }
        bus.cpu_write8(0xFF46, 0xC0);
        let _ = bus.cpu_fetch8(0x0100);

        let trace = bus.take_cycle_trace();
        assert_eq!(
            trace.len(),
            6,
            "each CPU machine cycle should have one record"
        );
        assert_eq!(
            trace.iter().map(|record| record.tcycle).collect::<Vec<_>>(),
            vec![0, 4, 8, 12, 16, 20],
            "trace times should identify the first differing machine cycle"
        );
        assert_eq!(trace[0].kind, BusCycleKind::Idle);
        assert_eq!(trace[4].kind, BusCycleKind::Write);
        assert_eq!(trace[4].address, Some(0xFF46));
        assert_eq!(trace[4].value, Some(0xC0));
        assert!(
            trace[4].state.dma_active,
            "FF46 write should be visible in its trace record"
        );
        assert_eq!(trace[4].state.dma_startup_mcycles, 2);
        assert_eq!(trace[5].kind, BusCycleKind::OpcodeFetch);
        assert_eq!(trace[4].state.timer_tima, 0x00);
        assert_eq!(trace[4].state.timer_tma, 0x42);
        assert_eq!(trace[4].state.timer_overflow_delay, Some(4));
        assert_eq!(trace[4].state.interrupt_flags, 0x00);
        assert_eq!(trace[5].state.timer_tima, 0x42);
        assert_eq!(trace[5].state.timer_overflow_delay, None);
        assert_eq!(trace[5].state.interrupt_flags, 0x04);
        assert_eq!(trace[5].state.interrupt_enable, 0x00);
        assert_eq!(trace[5].state.ppu_ly, 0);
        assert_eq!(trace[5].state.ppu_mode, PpuMode::OamSearch);
    }

    #[test]
    fn cycle_trace_time_includes_untraced_bus_ticks() {
        let mut bus = test_bus();
        bus.tick(TCycles(12));
        bus.cpu_read8(0xC000);

        let trace = bus.take_cycle_trace();
        assert_eq!(trace.len(), 1);
        assert_eq!(
            trace[0].tcycle, 12,
            "direct bus ticking must retain monotonic trace time"
        );
        assert_eq!(trace[0].kind, BusCycleKind::Read);
        assert_eq!(trace[0].address, Some(0xC000));
        assert_eq!(trace[0].value, Some(0));
    }

    #[test]
    fn cpu_machine_cycle_dispatches_each_tcycle_in_bus_order() {
        let mut bus = test_bus();

        bus.cpu_idle_mcycle();

        let trace = bus.take_tcycle_trace();
        assert_eq!(
            trace.len(),
            20,
            "one CPU machine cycle should expose four T-cycles and five Bus stages each"
        );
        assert_eq!(
            trace.iter().map(|record| record.tcycle).collect::<Vec<_>>(),
            vec![0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3],
            "the dispatcher should retain the monotonic T-cycle boundary for every stage"
        );
        assert_eq!(
            trace.iter().map(|record| record.stage).collect::<Vec<_>>(),
            vec![
                BusDispatchStage::Timer,
                BusDispatchStage::Ppu,
                BusDispatchStage::Apu,
                BusDispatchStage::OamDma,
                BusDispatchStage::Serial,
                BusDispatchStage::Timer,
                BusDispatchStage::Ppu,
                BusDispatchStage::Apu,
                BusDispatchStage::OamDma,
                BusDispatchStage::Serial,
                BusDispatchStage::Timer,
                BusDispatchStage::Ppu,
                BusDispatchStage::Apu,
                BusDispatchStage::OamDma,
                BusDispatchStage::Serial,
                BusDispatchStage::Timer,
                BusDispatchStage::Ppu,
                BusDispatchStage::Apu,
                BusDispatchStage::OamDma,
                BusDispatchStage::Serial,
            ],
            "each T-cycle should dispatch timer, PPU, APU, OAM DMA, then Serial"
        );
    }
}
