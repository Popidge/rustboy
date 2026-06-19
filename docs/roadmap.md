# DMG Accuracy Roadmap

## Scope and operating rules

This is the active roadmap for Rustboy. The target is the original monochrome
DMG Game Boy. CGB, SGB, and SGB2 emulation are explicitly out of scope. A
future-facing boundary is welcome only when it makes the DMG model clearer; do
not implement dormant colour or Super Game Boy behaviour.

The architecture remains:

```text
GameBoy owns Cpu and Bus
Bus owns cartridge, PPU, timer, serial, joypad, APU, memory, DMA, and IF/IE
CPU performs every memory operation through Bus
```

Components do not store references to one another. They exchange data through
owned state, return values, or temporary borrows during a Bus clock step.

Gambatte-Speedrun is a behavioural reference, not an implementation source.
Do not copy its GPLv2 code. Its test cases, timing observations, and the local
reverse-engineered boot-ROM source are useful evidence to independently model
and test hardware behaviour.

The timing model is a simple, deterministic Bus-owned dispatcher that advances
one DMG T-cycle at a time. This deliberately favours explicit hardware ordering
over a deadline/event scheduler. A future optimisation may skip to known event
deadlines only if it preserves the same externally observable cycle trace.

## Completion rules

Every milestone must:

- Stay scoped to one hardware rule or tightly related timing boundary.
- Add focused unit or integration tests where practical.
- Run `cargo fmt`, `cargo test`, and `cargo clippy --all-targets --all-features`.
- Name the exact external ROM gates used and record failures honestly.
- Update `docs/milestones.md`.

External ROM suites are diagnostic gates, not a reason to add a compatibility
hack. If a failure exposes an unmodelled timing rule, model the rule.

## Accuracy campaign

### 0048: Establish an accuracy baseline and trace language

**Goal:** Make timing failures observable before changing timing behaviour.

**Changes:** Add a deterministic, test-only Bus-cycle trace with T-cycle time,
cycle kind, address, value, and small snapshots of relevant state: PPU LY/mode,
IF/IE, timer overflow/reload state, and DMA state. Establish the initial result
for a focused DMG gate set.

**Gates:** Existing Rust tests; Blargg `instr_timing`, `mem_timing`, and
`halt_bug`; selected Mooneye timer, interrupt, and OAM-DMA cases; AGE
`vram`/`oam`/`stat`; selected GBMicrotest cases.

**Done when:** A failing timing case can identify the first differing bus-cycle
record, not merely a later serial or framebuffer mismatch.

### 0049: Install the one-T-cycle Bus dispatcher

**Goal:** Give all bus-owned hardware one shared, monotonic DMG timebase.

**Changes:** Centralise a documented one-T-cycle dispatcher in `Bus`; make
clocked CPU helpers express ordered fetch, read, write, and idle M-cycles; keep
the public instruction-step API and explicit `TCycles` values.

**Gates:** Trace tests from 0048, timer reload-window cases, and Blargg
instruction/memory timing.

**Done when:** No component is advanced directly by CPU code, and traces prove
that all four T-cycles of a CPU M-cycle are observable in Bus order.

### 0050: Tighten CPU exceptional sequencing

**Goal:** Make SM83 control-flow and interrupt edge cases occur in bus order.

**Changes:** Validate and correct opcode prefetch cancellation, interrupt entry
and priority re-sampling, EI/DI/RETI delay, HALT wake, HALT bug, and DMG STOP
wake behaviour.

**Gates:** Blargg `halt_bug`; AGE halt cases; Mooneye interrupt timing; short
trace goldens for interrupt entry and prefetch discard.

**Done when:** The CPU has no timing-only exceptions outside its explicit state
and ordered Bus operations.

### 0051: Complete DMG OAM-DMA arbitration

**Goal:** Model DMA as ownership of the CPU-visible bus, not simply an OAM copy.

**Changes:** Correct startup and restart timing, HRAM-only CPU access during
active DMA, source-region handling, echo-RAM behaviour, and any demonstrated
source-specific conflicts. Keep all decisions in `Bus`.

**Gates:** Mooneye `oam_dma_*`; Blargg `mem_timing`; GBMicrotest DMA cases;
trace tests around FF46 writes and the first transferred byte.

**Done when:** Existing remaining DMA timing failures are explained by a known
rule or represented by a narrow next milestone, never hidden by a ROM patch.

### 0052: Finish timer reload ordering and model serial transfer time

**Goal:** Complete two interrupt-producing peripherals at cycle granularity.

**Changes:** Resolve TIMA/TMA write and reload-cycle ordering. Replace
immediate serial debug capture with timed DMG internal-clock transfer,
start/completion semantics, and Serial IF requests; preserve completed-byte
collection as a headless test observer.

**Gates:** Mooneye timer acceptance cases; focused generated-ROM timer/serial
tests; applicable serial and interrupt ROM cases.

**Done when:** Timer and serial events are requested by the Bus dispatcher at
their documented times.

### 0053: Add optional user-supplied DMG boot-ROM startup

**Goal:** Support console-like reset without distributing boot-ROM material.

**Changes:** Accept and validate a user-supplied 256-byte DMG boot ROM; overlay
it at `0000..=00FF`; disable it through FF50; initialise reset state separately
from the existing post-boot fast-start path.

**Gates:** Unit tests for mapping, FF50, and reset state; optional local
user-ROM smoke tests. No boot-ROM bytes are committed, embedded, or built.

**Done when:** A supplied ROM reaches cartridge address `0100` through normal
execution, while existing tests can still use deterministic post-boot startup.

### 0054: Make PPU mode, LCD, STAT, and memory access rules accurate

**Goal:** Establish bus-visible PPU correctness before pixel-pipeline detail.

**Changes:** Model LCD enable/disable transitions, OAM search, VRAM/OAM access
locks, LY/LYC timing, and STAT as a rising-edge interrupt line rather than
independent interrupt sources.

**Gates:** AGE `vram`, `oam`, `stat`, and `ly`; GBMicrotest PPU/STAT cases;
DMG Acid 2 as a visual integration check.

**Done when:** CPU reads and writes observe correct PPU access restrictions and
STAT/LY failures have a traceable dot-level explanation.

### 0055: Replace fixed mode 3 with a DMG fetcher/FIFO pipeline

**Goal:** Render pixels at the time the LCD produces them.

**Changes:** Replace end-of-line rendering and fixed mode-3 duration with a
dot-driven fetcher/FIFO. Model SCX discard, window activation, sprite selection
and fetch stalls, variable mode-3 duration, and mid-line register effects.

**Gates:** AGE/window and PPU cases; Mealybug/GBMicrotest PPU cases; targeted
framebuffer and trace tests.

**Done when:** Mode 3 length emerges from pipeline state and register writes
affect only pixels that have not yet been produced.

### 0056: Close mapper and RTC DMG edge cases

**Goal:** Make supported cartridges behave as hardware rather than storage.

**Changes:** Audit MBC1 forbidden-bank mapping, MBC2 nibble RAM behaviour,
MBC3 RTC latch/halt/day carry semantics, and save-RAM/RTC persistence policy.

**Gates:** Existing mapper tests, MBC3 tester, RTC suites, and synthetic bank
and latch regression tests.

**Done when:** Every mapper change has a documented DMG rule and does not alter
unrelated mapper behaviour.

### 0057: Perform a DMG APU hardware pass

**Goal:** Improve sound timing without letting audio backend concerns leak into
the core.

**Changes:** Model frame-sequencer phase, length-enable extra clocks, sweep and
envelope edge cases, trigger behaviour, NR52 power quirks, and wave-RAM access
while channel 3 is active.

**Gates:** Focused APU state tests and accepted DMG audio test ROMs where
available; sample-output tests remain supplementary to register timing.

**Done when:** APU register and channel state transitions are driven by the same
Bus timebase as the rest of the machine.

## Deferred work

- CGB, SGB, and SGB2 hardware.
- Link cable and multiplayer serial transport.
- Desktop/debugger feature expansion unrelated to timing observability.
- Performance optimisation through an event scheduler.

These are not forbidden forever; they are intentionally not accuracy-campaign
work.
