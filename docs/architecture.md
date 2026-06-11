# Emulator Architecture

This document describes the intended architecture for the DMG Game Boy emulator.

The project began as a learning-first, agent-assisted emulator in Rust. The emulator has now reached the phase where the main architectural pressure is accuracy: making CPU, bus, timer, DMA, interrupts, and PPU behaviour line up in time. It should remain understandable, testable, and pleasant to work on while becoming accurate enough to run real non-colour Game Boy software.

The architecture should model the Game Boy as a collection of owned hardware components connected through a memory bus.

## Design goals

* Keep the emulator core independent from any desktop, audio, UI, or filesystem frontend.
* Model hardware components as owned Rust structs with clear responsibilities.
* Route all CPU memory access through the bus.
* Prefer explicit, readable code over clever generated code during the learning phase.
* Use Rust types to encode hardware invariants where they genuinely prevent confusion.
* Keep each milestone small, testable, and reviewable.
* Prefer hardware-model accuracy over behaviour-specific compatibility hacks.
* Avoid global mutable state.
* Avoid permanent references between hardware components.
* Avoid `unsafe` code unless explicitly approved and documented.

## Crate layout

The project should use a Rust workspace.

```text
gb-rs/
  crates/
    gb-core/
      src/
        lib.rs
        machine.rs
        bus.rs
        cpu/
          mod.rs
          registers.rs
          flags.rs
          instruction.rs
          execute.rs
        cartridge/
          mod.rs
          header.rs
          mbc.rs
        ppu/
          mod.rs
          registers.rs
          tile.rs
          sprite.rs
        timer.rs
        interrupt.rs
        joypad.rs
        serial.rs
        apu.rs
    gb-desktop/
      src/
        main.rs
  docs/
    architecture.md
    style.md
  AGENTS.md
```

`gb-core` contains the emulator logic.

`gb-desktop` contains frontend code: windowing, keyboard input, audio output, command-line arguments, save-file persistence, debug UI, and any platform integration.

The core crate must not depend on windowing, graphics, audio backend, or desktop UI crates.

## High-level model

The emulator models a Game Boy as a CPU connected to a bus. The bus owns or routes access to the rest of the hardware.

```text
+-------------------------+
|        GameBoy          |
|                         |
|  +------+    +--------+ |
|  | CPU  | -> |  Bus   | |
|  +------+    +--------+ |
|                |        |
|                +-- Cartridge
|                +-- PPU
|                +-- Timer
|                +-- Joypad
|                +-- Serial
|                +-- APU
|                +-- WRAM
|                +-- HRAM
|                +-- Interrupt registers
+-------------------------+
```

The CPU does not directly access cartridge memory, VRAM, timers, input, or IO registers. It reads and writes addresses through `Bus`.

## Core ownership structure

The preferred top-level structure is:

```rust
pub struct GameBoy {
    cpu: Cpu,
    bus: Bus,
}
```

`GameBoy` owns the whole emulated machine.

The bus owns hardware components and memory regions:

```rust
pub struct Bus {
    cartridge: Cartridge,
    ppu: Ppu,
    timer: Timer,
    joypad: Joypad,
    serial: Serial,
    apu: Apu,

    wram: [u8; 0x2000],
    hram: [u8; 0x7F],

    interrupt_enable: InterruptFlags,
    interrupt_flags: InterruptFlags,
}
```

The CPU should be stepped by temporarily borrowing the bus:

```rust
impl GameBoy {
    pub fn step(&mut self) -> TCycles {
        let cycles = self.cpu.step(&mut self.bus);
        cycles
    }
}
```

Early milestones ticked the bus after a whole instruction. Accuracy milestones
should instead move CPU execution toward clocked bus-cycle helpers, so timer,
PPU, DMA, and interrupt side effects can observe events inside an instruction.
See `docs/timing-architecture.md` for the timing direction.

Do not make `Cpu` permanently own or borrow the bus.

Avoid designs like:

```rust
pub struct Cpu<'a> {
    bus: &'a mut Bus,
}
```

The CPU should receive temporary access to the bus during execution.

## Hardware memory ownership

Fixed-size hardware memory should use fixed-size arrays.

Examples:

```rust
pub struct Ppu {
    vram: [u8; 0x2000],
    oam: [u8; 0xA0],
    framebuffer: [u32; 160 * 144],
}
```

```rust
pub struct Bus {
    wram: [u8; 0x2000],
    hram: [u8; 0x7F],
}
```

Variable-sized cartridge ROM and external RAM should use `Vec<u8>`.

```rust
pub struct RomOnly {
    rom: Vec<u8>,
}
```

```rust
pub struct Mbc1 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    selected_rom_bank: u8,
    selected_ram_bank: u8,
    ram_enabled: bool,
}
```

The emulator should not manually allocate and free fixed hardware regions. Rust ownership should be used to make the lifetime of each component clear.

## Memory map

All CPU memory access must go through the bus.

The initial DMG memory map is:

```text
0x0000-0x3FFF  Cartridge ROM bank 0
0x4000-0x7FFF  Cartridge switchable ROM bank
0x8000-0x9FFF  Video RAM
0xA000-0xBFFF  External cartridge RAM
0xC000-0xDFFF  Work RAM
0xE000-0xFDFF  Echo RAM
0xFE00-0xFE9F  OAM sprite attribute memory
0xFEA0-0xFEFF  Unusable memory
0xFF00-0xFF7F  IO registers
0xFF80-0xFFFE  High RAM
0xFFFF         Interrupt enable register
```

`Bus::read8` and `Bus::write8` are responsible for address routing.

Example shape:

```rust
impl Bus {
    pub fn read8(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cartridge.read_rom(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr - 0x8000),
            0xA000..=0xBFFF => self.cartridge.read_ram(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.read8(addr - 0x2000),
            0xFE00..=0xFE9F => self.ppu.read_oam(addr - 0xFE00),
            0xFEA0..=0xFEFF => 0xFF,
            0xFF00 => self.joypad.read(),
            0xFF01..=0xFF02 => self.serial.read(addr),
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.interrupt_flags.read_if(),
            0xFF10..=0xFF3F => self.apu.read(addr),
            0xFF40..=0xFF4B => self.ppu.read_register(addr),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.interrupt_enable.read_ie(),
            _ => 0xFF,
        }
    }
}
```

The exact implementation may change, but the architectural rule should not: the CPU talks to the bus, not directly to components.

## CPU

The CPU models the Game Boy LR35902-like processor.

The CPU owns:

* Registers
* Interrupt master enable state
* Halted/stopped state
* Any delayed interrupt enable state
* Internal execution helpers

The CPU does not own memory.

Registers should be represented clearly:

```rust
pub struct Registers {
    pub a: u8,
    pub f: Flags,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}
```

Register pairs should be exposed through helpers:

```rust
impl Registers {
    pub fn af(&self) -> u16;
    pub fn set_af(&mut self, value: u16);

    pub fn bc(&self) -> u16;
    pub fn set_bc(&mut self, value: u16);

    pub fn de(&self) -> u16;
    pub fn set_de(&mut self, value: u16);

    pub fn hl(&self) -> u16;
    pub fn set_hl(&mut self, value: u16);
}
```

The lower nibble of the flags register must always be zero.

## Instruction execution

Instruction execution should begin with an explicit opcode match.

```rust
match opcode {
    0x00 => self.nop(),
    0x3E => self.ld_a_d8(bus),
    0xAF => self.xor_a(),
    _ => return Err(CpuError::UnimplementedOpcode { pc, opcode }),
}
```

During the learning phase:

* Prefer explicit match arms.
* Avoid macro-generated opcode tables unless approved.
* Avoid dense opcode cleverness until the CPU is substantially complete.
* Instruction helpers should be small and testable.
* Shared ALU helpers are encouraged.

Each instruction should return consumed cycles using the chosen cycle type. For
timing-sensitive work, instruction helpers should also preserve the order of
fetches, reads, writes, and internal cycles by using clocked bus helpers.

## Cycle units

The emulator must choose one internal cycle unit and name it explicitly.

Preferred approach:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TCycles(pub u32);
```

If machine cycles are used anywhere, they must be distinct:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MCycles(pub u32);
```

Do not use a naked `u32` named `cycles` in public APIs without documenting whether it represents T-cycles or M-cycles.

## Bus timing

The bus owns hardware timing. Earlier implementation milestones advanced
hardware after each whole instruction:

```rust
impl Bus {
    pub fn tick(&mut self, cycles: TCycles) {
        self.timer.tick(cycles, &mut self.interrupt_flags);
        self.ppu.tick(cycles, &mut self.interrupt_flags);
        self.apu.tick(cycles);
    }
}
```

That model is now a baseline only. Accuracy work should advance hardware during
CPU-visible machine cycles through bus-owned helpers such as opcode fetch,
memory read, memory write, and internal idle. This keeps ordering decisions in
`Bus` while preserving the rule that CPU memory access goes through the bus.

Components should not permanently borrow `InterruptFlags`. They should either:

1. Receive temporary mutable access during `tick`, or
2. Return events that the bus applies.

For the initial implementation, temporary mutable access is preferred for simplicity.

## Interrupts

Interrupts should be represented with typed helpers rather than loose bit manipulation everywhere.

Interrupts:

```text
Bit 0: VBlank
Bit 1: LCD STAT
Bit 2: Timer
Bit 3: Serial
Bit 4: Joypad
```

Recommended enum:

```rust
pub enum Interrupt {
    VBlank,
    LcdStat,
    Timer,
    Serial,
    Joypad,
}
```

Recommended wrapper:

```rust
pub struct InterruptFlags {
    bits: u8,
}
```

`IF` reads should preserve unused-bit behaviour as required by DMG hardware.

The CPU is responsible for servicing interrupts:

* Check enabled and requested interrupts.
* Respect `IME`.
* Clear `IME`.
* Clear the serviced interrupt request bit.
* Push `PC` onto the stack.
* Jump to the interrupt vector.

Interrupt vectors:

```text
VBlank:  0x0040
LCD STAT: 0x0048
Timer:   0x0050
Serial:  0x0058
Joypad:  0x0060
```

## Cartridge

The cartridge is hardware, not just a file.

Start with ROM-only support, then evolve to cartridge controller variants.

Preferred representation after ROM-only:

```rust
pub enum Cartridge {
    RomOnly(RomOnly),
    Mbc1(Mbc1),
    Mbc3(Mbc3),
    Mbc5(Mbc5),
    Unsupported(UnsupportedCartridge),
}
```

An enum is preferred over `Box<dyn Mapper>` at first because:

* The mapper set is finite and known.
* Exhaustiveness checking is useful.
* It keeps control flow explicit while learning.

A trait-based design may be reconsidered later if it simplifies the implementation.

The cartridge should parse:

```text
0x0134-0x0143  Title
0x0147         Cartridge type
0x0148         ROM size
0x0149         RAM size
0x014D         Header checksum
```

ROM loading and parsing should return `Result`.

Runtime cartridge reads and writes should mimic hardware behaviour and return concrete values.

## PPU

The PPU owns:

* VRAM
* OAM
* LCD registers
* Current line
* Current mode
* Dot/cycle progress
* Framebuffer
* Frame-ready flag

The PPU should expose bus-facing methods:

```rust
impl Ppu {
    pub fn read_vram(&self, offset: u16) -> u8;
    pub fn write_vram(&mut self, offset: u16, value: u8);

    pub fn read_oam(&self, offset: u16) -> u8;
    pub fn write_oam(&mut self, offset: u16, value: u8);

    pub fn read_register(&self, addr: u16) -> u8;
    pub fn write_register(&mut self, addr: u16, value: u8);

    pub fn tick(&mut self, cycles: TCycles, interrupts: &mut InterruptFlags);
}
```

PPU modes should use an enum internally:

```rust
pub enum PpuMode {
    HBlank,
    VBlank,
    OamSearch,
    PixelTransfer,
}
```

At the bus boundary, modes are converted to STAT register bits.

The framebuffer should initially be simple and frontend-friendly:

```rust
framebuffer: [u32; 160 * 144]
```

A more abstract colour representation can be introduced later if useful.

## Timer

The timer owns:

* DIV state
* TIMA
* TMA
* TAC
* Internal divider state needed to model timing

Timer registers:

```text
FF04 DIV
FF05 TIMA
FF06 TMA
FF07 TAC
```

The timer advances through `Timer::tick`.

When TIMA overflows, the timer requests the Timer interrupt.

The timer should not directly mutate the CPU.

Accuracy work should model TIMA increments from selected DIV-bit falling edges,
including DIV/TAC write effects and delayed overflow reload behaviour.

## Joypad

The joypad owns button state.

Buttons:

```rust
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
```

The joypad exposes the `FF00` hardware register behaviour.

The frontend maps keyboard or controller events into `Button` state changes through `GameBoy` or `Bus` methods.

Button bits are active-low. The implementation should make this explicit in tests.

## Serial

The serial component owns:

* SB register
* SC register
* A debug output buffer or callback-free collection mechanism

Registers:

```text
FF01 SB
FF02 SC
```

Minimal serial support is required for many test ROMs.

When a transfer is triggered, the emulator should expose the transferred byte for headless test output.

`gb-core` should not print directly to stdout. The frontend or runner should collect and print serial output.

## APU

The APU can start as a register skeleton.

The APU owns audio registers and eventually audio channel state.

Audio output backend code belongs in `gb-desktop`, not `gb-core`.

`gb-core` may expose generated samples through a buffer or pull API, but it should not know about the host audio library.

## Frontend boundary

`gb-desktop` may handle:

* Window creation
* Framebuffer scaling
* Keyboard input
* Controller input
* Audio output
* ROM loading from filesystem
* Save RAM files
* Debugger UI
* Command-line arguments
* Headless runner mode

`gb-core` should expose a clean API:

```rust
impl GameBoy {
    pub fn from_rom(rom: Vec<u8>) -> Result<Self, EmulatorError>;

    pub fn step(&mut self) -> TCycles;

    pub fn run_until_frame(&mut self);

    pub fn framebuffer(&self) -> &[u32; 160 * 144];

    pub fn set_button(&mut self, button: Button, pressed: bool);

    pub fn take_serial_output(&mut self) -> Vec<u8>;

    pub fn save_ram(&self) -> Option<&[u8]>;

    pub fn load_save_ram(&mut self, data: &[u8]) -> Result<(), SaveRamError>;
}
```

The exact API can evolve, but the boundary should remain clean.

## Testing strategy

Each hardware component should have isolated unit tests.

Required categories:

* Cartridge header parsing
* Cartridge banking
* Register pairs
* Flag behaviour
* Instruction execution
* Bus address routing
* Timer increments and overflow
* Interrupt requests and servicing
* Serial output
* Tile decoding
* PPU mode transitions
* Joypad register behaviour
* DMA
* Save RAM extraction/restoration

External test ROMs should not be committed unless their license clearly permits it.

The repo may include documentation explaining where to place local test ROMs.

Headless test runner support is encouraged.

## Debugging and observability

The emulator should support trace output without printing directly from the core.

Useful trace format:

```text
PC=0100 OP=00 AF=01B0 BC=0013 DE=00D8 HL=014D SP=FFFE
```

The core may expose trace formatting helpers. The frontend or runner decides whether to print them.

Useful future debug tools:

* CPU register viewer
* Instruction stepping
* Breakpoints
* VRAM tile viewer
* Background map viewer
* OAM/sprite viewer
* Serial output console
* Screenshot capture

## Architectural rules

1. `gb-core` must not depend on desktop/window/audio backend crates.
2. `Cpu` must not permanently own or borrow `Bus`.
3. CPU memory access goes through `Bus`.
4. Fixed hardware memory uses fixed-size arrays.
5. Variable cartridge ROM/RAM uses `Vec<u8>`.
6. Components do not store references to each other.
7. Prefer enums for finite hardware states.
8. Prefer newtypes for cycle units and important bitflag surfaces.
9. Timing-sensitive CPU bus access should advance hardware in the order the CPU performs fetches, reads, writes, and internal cycles.
10. Runtime bus reads return `u8`, not `Result<u8, _>`.
11. ROM loading, parsing, and setup errors should use `Result`.
12. Unknown or unimplemented opcodes must include PC and opcode in the error.
13. No `unsafe` code in `gb-core` unless explicitly approved and documented.
14. No `Rc<RefCell<T>>` in `gb-core` unless explicitly justified.
15. All instruction groups require tests.
16. All milestone work should update milestone records.
