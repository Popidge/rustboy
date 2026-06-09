# Rust Style Guide

This document describes the Rust style and implementation rules for the DMG Game Boy emulator.

The goal is to write Rust that is clear, testable, strongly typed where useful, and faithful to the hardware model. We are not trying to write a C emulator with crab stickers on it.

## Core principles

* Prefer explicit code over clever code.
* Prefer ownership over shared mutable state.
* Prefer small structs with clear responsibilities.
* Prefer strongly typed hardware concepts where they prevent real bugs.
* Prefer fixed-size arrays for fixed-size hardware memory.
* Prefer `Vec<u8>` for variable-sized cartridge data.
* Prefer tests at every behavioural boundary.
* Keep hot runtime paths simple.
* Keep frontend code out of the emulator core.

## No unsafe by default

`unsafe` is not allowed in `gb-core` unless explicitly approved.

If `unsafe` is introduced, it must include:

* A comment explaining why it is needed.
* A comment explaining the safety invariant.
* Tests or benchmarks showing why the safe version was insufficient.
* A note in the relevant milestone record.

For a DMG emulator, `unsafe` should almost certainly not be needed.

## Avoid shared mutable state

Do not use these in `gb-core` unless explicitly justified:

```rust
Rc<RefCell<T>>
Arc<Mutex<T>>
static mut
lazy global mutable state
```

A single-threaded emulator core should use normal ownership and temporary mutable borrows.

Preferred:

```rust
self.cpu.step(&mut self.bus);
```

Avoid:

```rust
cpu.bus.borrow_mut().read8(addr);
```

If the core starts to fill with `Rc<RefCell<T>>`, stop and revisit the architecture.

## Component ownership

Hardware components should be owned by their parent struct.

Preferred:

```rust
pub struct Bus {
    cartridge: Cartridge,
    ppu: Ppu,
    timer: Timer,
    joypad: Joypad,
}
```

Avoid permanent references between components.

Avoid:

```rust
pub struct Timer<'a> {
    interrupt_flags: &'a mut InterruptFlags,
}
```

Preferred:

```rust
impl Timer {
    pub fn tick(&mut self, cycles: TCycles, interrupts: &mut InterruptFlags) {
        // ...
    }
}
```

Temporary access is easier to reason about and keeps the ownership graph simple.

## Fixed hardware memory

Use arrays for hardware memory regions with fixed sizes.

```rust
wram: [u8; 0x2000]
hram: [u8; 0x7F]
vram: [u8; 0x2000]
oam: [u8; 0xA0]
framebuffer: [u32; 160 * 144]
```

Do not allocate these with `Vec` unless there is a clear reason.

The Game Boy’s memory regions are known sizes. Let Rust encode that.

## Cartridge memory

Use `Vec<u8>` for cartridge ROM and cartridge RAM because their sizes vary by cartridge.

```rust
pub struct Mbc1 {
    rom: Vec<u8>,
    ram: Vec<u8>,
}
```

ROM loading should use owned bytes:

```rust
let rom = std::fs::read(path)?;
```

The core should accept ROM bytes from the frontend:

```rust
GameBoy::from_rom(rom_bytes)
```

`gb-core` should not need to know where the bytes came from.

## Public API design

Keep public APIs small and meaningful.

Good:

```rust
pub fn read8(&self, addr: u16) -> u8;
pub fn write8(&mut self, addr: u16, value: u8);
pub fn step(&mut self) -> TCycles;
pub fn framebuffer(&self) -> &[u32; 160 * 144];
```

Avoid exposing internal fields unless there is a specific reason.

For tests, prefer helper constructors or test-only accessors over making everything public.

## Error handling

Use `Result` for setup and validation:

* ROM loading
* Cartridge header parsing
* Unsupported cartridge types
* Save RAM loading
* CLI/frontend errors

Example:

```rust
pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, CartridgeError>;
```

Do not use `Result` for every hardware read/write in the hot emulation path.

Preferred:

```rust
pub fn read8(&self, addr: u16) -> u8;
```

Real hardware returns values from odd places. It does not return `Result`.

For unimplemented CPU opcodes, return a structured CPU error or panic in a controlled way during early development. The error must include PC and opcode.

## Types and newtypes

Use newtypes where they prevent real confusion.

Recommended:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TCycles(pub u32);
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptFlags {
    bits: u8,
}
```

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lcdc {
    bits: u8,
}
```

Useful enums:

```rust
pub enum PpuMode {
    HBlank,
    VBlank,
    OamSearch,
    PixelTransfer,
}
```

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

```rust
pub enum Interrupt {
    VBlank,
    LcdStat,
    Timer,
    Serial,
    Joypad,
}
```

Avoid over-typing every value too early.

Not every `u16` needs to become a custom address type. Use stronger types where they reduce actual ambiguity, especially for cycles, flags, modes, interrupts, buttons, and cartridge kinds.

## Flags

The CPU flags register should not be a loose public `u8`.

The lower nibble of `F` must always be zero.

Preferred:

```rust
pub struct Flags {
    bits: u8,
}
```

Provide helpers:

```rust
pub fn zero(&self) -> bool;
pub fn set_zero(&mut self, value: bool);

pub fn subtract(&self) -> bool;
pub fn set_subtract(&mut self, value: bool);

pub fn half_carry(&self) -> bool;
pub fn set_half_carry(&mut self, value: bool);

pub fn carry(&self) -> bool;
pub fn set_carry(&mut self, value: bool);

pub fn raw(&self) -> u8;
pub fn set_raw(&mut self, value: u8);
```

`set_raw` must mask out the lower nibble.

```rust
self.bits = value & 0xF0;
```

## Bit registers

For hardware registers with unused bits or special read/write behaviour, prefer wrapper types or focused methods.

Example:

```rust
impl InterruptFlags {
    pub fn read_if(&self) -> u8 {
        self.bits | 0xE0
    }

    pub fn write_if(&mut self, value: u8) {
        self.bits = value & 0x1F;
    }
}
```

Do not scatter magic masks throughout unrelated code.

If a mask appears in more than one place, consider naming it.

## Address routing

All address routing belongs in `Bus`.

Good:

```rust
match addr {
    0x8000..=0x9FFF => self.ppu.read_vram(addr - 0x8000),
    0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
    _ => 0xFF,
}
```

Avoid letting the CPU directly access component internals.

Bad:

```rust
cpu.ppu.vram[index]
```

The CPU does not know what VRAM is. The CPU knows addresses.

## Indexing style

For address ranges already checked by `match`, direct indexing is acceptable.

```rust
0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
```

For public component methods that receive offsets, choose one of:

```rust
self.vram[offset as usize]
```

or:

```rust
self.vram.get(offset as usize).copied().unwrap_or(0xFF)
```

Use direct indexing when the caller has already guaranteed the range. Use checked indexing when the method itself is the boundary.

Document the assumption.

## Instruction implementation

During the learning phase, use explicit opcode matches.

Preferred:

```rust
match opcode {
    0x00 => self.nop(),
    0x3E => self.ld_r_d8(Register8::A, bus),
    0x06 => self.ld_r_d8(Register8::B, bus),
    _ => return Err(CpuError::UnimplementedOpcode { pc, opcode }),
}
```

Avoid macro-heavy generated tables until the CPU is substantially complete.

Instruction helpers should be small.

Shared ALU helpers are encouraged:

```rust
fn alu_add(&mut self, value: u8);
fn alu_adc(&mut self, value: u8);
fn alu_sub(&mut self, value: u8);
fn alu_cp(&mut self, value: u8);
```

The opcode layer should handle decoding. The helper should handle behaviour.

## Cycle accounting

Cycle units must be explicit.

Preferred:

```rust
pub fn step(&mut self, bus: &mut Bus) -> TCycles;
```

Avoid returning naked integers without context.

Bad:

```rust
pub fn step(&mut self, bus: &mut Bus) -> u32;
```

Acceptable only if the type alias and documentation are clear:

```rust
pub type TCycles = u32;
```

A newtype is preferred.

## Logging and tracing

`gb-core` should not print directly to stdout during normal operation.

Instead, expose trace strings or events.

Good:

```rust
pub fn trace_line(&self, opcode: u8, pc: u16) -> String;
```

or:

```rust
pub struct CpuTrace {
    pub pc: u16,
    pub opcode: u8,
    pub registers: RegistersSnapshot,
}
```

The runner or frontend decides whether to print.

Serial output should also be collected, not printed directly by the core.

## Testing requirements

Every milestone should include tests.

Expected test types:

* Unit tests for small logic
* Table tests for opcode families
* Integration tests for bus routing
* Headless ROM-runner tests when available
* Screenshot or framebuffer tests for PPU milestones

Instruction tests should cover edge cases, especially:

* Wrapping arithmetic
* Zero flag
* Carry flag
* Half-carry flag
* Subtract flag
* Signed offsets
* Stack push/pop ordering
* Conditional branch taken and not taken cycles

Any bug fix should include a regression test where practical.

## Formatting

Use standard Rust formatting.

Required commands before considering a milestone complete:

```bash
cargo fmt
cargo test
cargo clippy --all-targets --all-features
```

Clippy warnings should be fixed unless there is a clear reason not to. If a warning is allowed, document why.

## Naming

Use clear names over abbreviations, except for established hardware names.

Good established names:

```text
cpu
ppu
apu
wram
hram
vram
oam
lcdc
stat
tima
tma
tac
div
ime
```

Avoid cryptic helper names.

Good:

```rust
request_interrupt
service_interrupt
decode_tile_row
selected_rom_bank
```

Bad:

```rust
irqgo
tdr
pixm
bankthing
```

The Game Boy is already cryptic enough. Do not add fog.

## Comments

Comments should explain hardware behaviour, invariants, or non-obvious decisions.

Good:

```rust
// The lower four bits of F are always zero on the Game Boy CPU.
self.f.set_raw(value);
```

Good:

```rust
// Echo RAM mirrors C000-DDFF at E000-FDFF.
self.write8(addr - 0x2000, value);
```

Avoid comments that restate the syntax.

Bad:

```rust
// increment x by one
x += 1;
```

## Documentation

Important hardware-facing modules should include a short module-level comment explaining what they model.

Example:

```rust
//! Timer hardware for the DMG Game Boy.
//!
//! Models DIV, TIMA, TMA, and TAC. The timer is advanced by Bus::tick
//! and requests the Timer interrupt when TIMA overflows.
```

Do not paste large chunks of external documentation into the source. Summarise behaviour and cite references in project docs if needed.

## Agent-friendly implementation rules

When generating or editing code, agents should:

1. Keep changes scoped to the requested milestone.
2. Avoid opportunistic refactors.
3. Add tests with each behaviour change.
4. Preserve public APIs unless the milestone asks to change them.
5. Explain any architectural tradeoff in the milestone record.
6. Avoid macros unless requested.
7. Avoid adding dependencies unless requested.
8. Avoid `unsafe`.
9. Avoid `Rc<RefCell<T>>`.
10. Run or update the relevant tests.

## Dependency policy

Keep dependencies minimal.

Suitable early dependencies may include:

* `thiserror` for error types
* `bitflags` if useful for register wrappers
* `clap` for CLI parsing in frontend
* `pixels` and `winit` for desktop display
* `cpal` for audio output later
* `png` or `image` for screenshot writing

Do not add a dependency for trivial logic.

No frontend dependency should be added to `gb-core`.

## Preferred development rhythm

For each milestone:

1. Write or update the tests first where practical.
2. Implement the smallest useful change.
3. Run formatting, tests, and clippy.
4. Update milestone records.
5. Keep the commit focused.

Small, boring changes are good. Emulators become exciting by accumulation, not by one giant wizard commit.
