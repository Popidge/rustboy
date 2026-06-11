# Timing Architecture

This document describes the accuracy-focused timing direction for the emulator.

The early architecture deliberately used instruction-level stepping: the CPU
executed one instruction, returned a total T-cycle count, and the bus advanced
hardware afterward. That was the right shape for building the emulator in a
learning-first way. It is now too coarse for the next phase.

The next phase should model CPU-visible bus activity in the order it happens.
Timers, PPU modes, OAM DMA, interrupt requests, and memory access restrictions
must be able to observe individual machine-cycle boundaries inside an
instruction.

## Goals

* Keep the existing ownership model: `GameBoy` owns `Cpu` and `Bus`.
* Keep all CPU memory access routed through `Bus`.
* Advance hardware during CPU bus cycles, not only after whole instructions.
* Preserve readable explicit opcode execution while making timing observable.
* Prefer small timing milestones with ROM-test gates over broad compatibility
  patches.
* Avoid behaviour-specific hacks whose only purpose is to pass one external
  ROM while making the hardware model less clear.

## Time Units

`TCycles` remains the core hardware time unit. One DMG machine cycle is four
T-cycles.

If a machine-cycle type is introduced, it must be distinct from `TCycles`:

```rust
pub struct TCycles(pub u32);
pub struct MCycles(pub u32);
```

The implementation should not mix naked `u32` cycle values across public or
cross-component APIs. When a helper consumes or returns cycles, the unit should
be visible from the type or the function name.

## CPU And Bus Sequencing

The public `GameBoy::step` API may continue to execute one logical CPU
instruction and return its total T-cycles.

Internally, CPU execution should move toward ordered bus-cycle helpers. Example
shape:

```rust
impl Bus {
    pub fn cpu_fetch8(&mut self, address: u16) -> u8;
    pub fn cpu_read8(&mut self, address: u16) -> u8;
    pub fn cpu_write8(&mut self, address: u16, value: u8);
    pub fn cpu_idle_mcycle(&mut self);
}
```

Each helper should advance hardware by the appropriate number of T-cycles at the
point where the CPU performs that operation. For ordinary DMG CPU bus cycles,
that usually means four T-cycles per machine cycle.

Instruction helpers should express the order of events, not just the final
state. For example, a stack push should expose the internal cycle and the two
ordered writes rather than calling an instantaneous `write16`.

Debug and test-only helpers may still inspect memory without advancing time, but
CPU execution must use the clocked bus path.

## Bus Cycle Kinds

The bus will eventually need to distinguish CPU-facing operations because DMA,
PPU access restrictions, and some tests care about the kind and timing of the
access.

Recommended internal vocabulary:

* Opcode fetch
* Operand read
* Data read
* Data write
* Internal CPU cycle
* DMA transfer cycle

This does not require a large abstraction up front. It is acceptable to begin
with focused helper methods and introduce an enum only when repeated logic makes
it useful.

## Hardware Tick Ordering

The bus is responsible for applying time to owned hardware components.

During a CPU machine cycle, the bus should advance:

* Timer
* PPU
* APU
* OAM DMA
* Interrupt request side effects from those components

Exact sub-cycle ordering can be refined as tests demand, but all code should be
written so that ordering decisions are centralized in `Bus`, not scattered
through CPU instruction helpers.

## Timer Direction

The timer should be rebuilt around an internal divider counter rather than
independent `DIV` and `TIMA` countdown counters.

Accuracy work should model:

* `DIV` as the visible upper bits of an internal divider.
* `TIMA` increments from the selected divider bit's falling edge while enabled.
* `DIV` writes and `TAC` writes affecting the selected timer bit.
* TIMA overflow, reload delay, `TMA` reload, and Timer interrupt request timing.
* TIMA/TMA writes during the overflow and reload window.

Mooneye timer tests and focused unit tests should be used as the first gates for
this work.

## OAM DMA Direction

OAM DMA should become stateful.

Writing `FF46` should start a transfer rather than copying all 160 bytes
immediately. The DMA engine should track:

* Source high byte
* Current byte index
* Startup delay, if modelled
* Active transfer state
* CPU access restrictions while DMA is active

The CPU should continue executing through the bus. The bus decides what reads
and writes return during DMA.

## PPU Direction

The current scanline renderer is a useful framebuffer model, but PPU timing work
should proceed in layers:

1. Preserve dot-based LY and mode progression.
2. Gate VRAM and OAM CPU access by PPU mode.
3. Tighten STAT interrupt edge behaviour.
4. Add LCD enable/disable timing.
5. Rebuild pixel transfer around fetcher/FIFO behaviour.
6. Model variable Mode 3 length from scroll, window, and sprites.

Do not attempt exact pixel FIFO behaviour before the CPU/bus/timer/DMA
foundation can observe machine-cycle timing.

## Interrupt Direction

Interrupt servicing should be expressed as ordered CPU cycles:

* Detection before opcode fetch.
* Internal interrupt service cycles.
* Ordered stack writes.
* IF bit clear and IME clear at documented points.
* PC update to the interrupt vector.

HALT, EI/DI delay, and pending interrupt wake behaviour should remain explicit
CPU state, but their timing should be validated against the clocked bus model.

## Test Gates

Accuracy milestones should use small, named gates rather than the whole external
ROM suite.

Suggested initial gates:

* Existing in-repo unit and integration tests.
* Blargg `cpu_instrs`, `instr_timing`, `halt_bug`, and `mem_timing`.
* Mooneye timer acceptance tests.
* Mooneye interrupt and DMA timing tests.
* AGE `halt`, `ly`, `stat`, `oam`, and `vram` groups.
* GBMicrotest timer, DMA, OAM, VRAM, and STAT cases.

The full suite report remains useful as a broad dashboard, not as the acceptance
criterion for every milestone.

## What To Avoid

Avoid one-ROM compatibility patches when the failure points to missing timing
architecture. A passing result is not useful if it depends on behaviour that
cannot be explained in hardware terms.

Avoid broad rewrites of CPU decoding, cartridge controllers, desktop UI, audio,
or rendering quality while rebuilding timing. Those systems should move only
when they directly support the timing milestone being worked.

## Useful References

* Pan Docs CPU, timer, PPU, DMA, and interrupt documentation.
* Gekkio's Game Boy: Complete Technical Reference.
* Mooneye GB test suite documentation and acceptance ROMs.
* Blargg test ROMs for CPU, instruction timing, memory timing, and HALT.
* AGE, GBMicrotest, and Mealybug test ROMs for PPU and bus-visible behaviour.
