# Roadmap

## Guiding rule

Each milestone should have:

```text
Goal: what you are adding
You learn: the emulator concept it teaches
Done when: a concrete pass/fail condition
Agent task: the kind of scoped prompt you can give Codex
```

For a learning-first project, I’d bias toward **many small milestones** rather than heroic “implement the PPU” slabs. That keeps the agent on a leash and gives you regular “I understand this bit now” checkpoints.

---

# Stage 0: Project foundation

## 0.1 Create the Rust workspace

**Goal:** Set up the repo shape.

```text
gb-rs/
  crates/
    gb-core/
    gb-desktop/
```

**You learn:** How to separate emulator core from frontend.

**Done when:**

```bash
cargo test
cargo run -p gb-desktop
```

both work.

**Agent task:**

```text
Create a Rust workspace with gb-core and gb-desktop crates.
gb-core should expose a placeholder GameBoy struct.
gb-desktop should depend on gb-core and print a startup message.
Add rustfmt/clippy-friendly defaults.
```

---

## 0.2 Add basic project doctrine

**Goal:** Write `docs/architecture.md`.

**You learn:** How to control the agent before it turns the repo into spaghetti carbonara.

**Done when:** The repo has written rules like:

```text
CPU only talks to Bus.
gb-core has no windowing.
Each instruction returns cycles.
No macro-generated opcode tables yet.
```

**Agent task:**

```text
Create docs/architecture.md describing the emulator architecture rules.
Do not write code.
```

This seems silly. It is not. It’s your little constitutional monarchy for silicon crimes.

---

## 0.3 Add test helpers

**Goal:** Make tiny fake ROMs easy to build in tests.

**You learn:** How test-driven emulator work feels.

**Done when:** You can create a fake cartridge byte array with known bytes at `0x0100`.

**Agent task:**

```text
Add a test helper for creating a minimal 32 KiB ROM image with configurable bytes at address 0x0100.
Use it only in tests.
```

---

# Stage 1: Cartridge and ROM loading

## 1.1 Load raw ROM bytes

**Goal:** Read a `.gb` file into memory.

**You learn:** A ROM is just bytes until hardware interprets them.

**Done when:** CLI can print ROM length.

**Agent task:**

```text
Implement a Cartridge type that stores ROM bytes.
Add Cartridge::from_bytes and Cartridge::len.
Do not parse headers yet.
Add tests.
```

---

## 1.2 Parse cartridge title

**Goal:** Read the title from the Game Boy header.

Header title area:

```text
0x0134..=0x0143
```

**You learn:** Game Boy cartridges have structured metadata.

**Done when:** A fake ROM with a title returns that title.

**Agent task:**

```text
Add cartridge header parsing for the title field.
Handle zero padding.
Add unit tests with fake ROMs.
```

---

## 1.3 Parse cartridge type, ROM size, RAM size

**Goal:** Decode header bytes:

```text
0x0147 cartridge type
0x0148 ROM size
0x0149 RAM size
```

**You learn:** Why cartridge hardware matters.

**Done when:** CLI prints:

```text
Title: TETRIS
Type: ROM ONLY
ROM: 32 KiB
RAM: None
```

**Agent task:**

```text
Parse cartridge type, ROM size code, and RAM size code.
For now only support ROM ONLY, but represent unsupported types clearly.
Add tests.
```

---

## 1.4 Validate header checksum

**Goal:** Implement the Game Boy header checksum.

**You learn:** ROM integrity and hardware expectations.

**Done when:** Known/fake headers can pass/fail checksum tests.

**Agent task:**

```text
Implement Game Boy cartridge header checksum validation.
Add tests for valid and invalid checksum.
```

---

## 1.5 Implement ROM-only cartridge reads

**Goal:** Make reads from `0x0000..=0x7FFF` return ROM data.

**You learn:** Cartridge memory is mapped into CPU address space.

**Done when:** Reading address `0x0100` returns the byte stored there.

**Agent task:**

```text
Implement ROM-only cartridge read for addresses 0x0000 through 0x7FFF.
Out-of-range reads should return a clear error in tests.
```

---

# Stage 2: CPU state

## 2.1 Implement CPU registers

**Goal:** Add A, F, B, C, D, E, H, L, SP, PC.

**You learn:** The CPU’s working state.

**Done when:** Registers can be created and inspected.

**Agent task:**

```text
Implement CpuRegisters with A, F, B, C, D, E, H, L, SP, PC.
Add Debug formatting useful for trace logs.
```

---

## 2.2 Implement 16-bit register pairs

**Goal:** Add helpers:

```text
AF, BC, DE, HL
```

**You learn:** 8-bit registers often work as 16-bit pairs.

**Done when:** Setting `BC = 0x1234` makes `B = 0x12`, `C = 0x34`.

**Agent task:**

```text
Add get/set helpers for AF, BC, DE, HL.
Ensure the lower 4 bits of F are always zero.
Add unit tests.
```

---

## 2.3 Implement flag helpers

**Goal:** Add helpers for:

```text
Z Zero
N Subtract
H Half-carry
C Carry
```

**You learn:** Arithmetic instructions leave clues for future instructions.

**Done when:** You can set/clear/read flags independently.

**Agent task:**

```text
Implement flag get/set helpers for Z, N, H, C.
Keep the lower nibble of F zero.
Add tests for every flag.
```

---

## 2.4 Add post-boot initial state

**Goal:** Initialize CPU as though the boot ROM already ran.

**You learn:** Why skipping the boot ROM requires fake initial hardware state.

**Done when:** `Cpu::new_dmg_post_boot()` gives expected register values.

**Agent task:**

```text
Add Cpu::new_dmg_post_boot with standard DMG post-boot register values.
Document that we are not running the boot ROM yet.
Add a test.
```

---

# Stage 3: Bus and memory map

## 3.1 Create the Bus type

**Goal:** Introduce memory routing.

**You learn:** The CPU does not own memory. It asks the bus.

**Done when:** Bus exists and owns cartridge, WRAM, HRAM.

**Agent task:**

```text
Create a Bus type that owns Cartridge, WRAM, HRAM, interrupt enable, and interrupt flags.
Do not implement PPU/timer yet.
```

---

## 3.2 Implement basic bus reads

**Goal:** Route reads:

```text
0x0000-0x7FFF cartridge ROM
0xC000-0xDFFF WRAM
0xFF80-0xFFFE HRAM
0xFFFF IE
```

**You learn:** Memory map dispatch.

**Done when:** Unit tests prove reads come from the right region.

**Agent task:**

```text
Implement Bus::read8 for cartridge ROM, WRAM, HRAM, and IE.
Unsupported reads should return 0xFF for now and optionally log in debug builds.
Add tests.
```

---

## 3.3 Implement basic bus writes

**Goal:** Route writes to WRAM, HRAM, IE.

**You learn:** Writes can mean “store this” or “poke hardware.”

**Done when:** Write/read roundtrips pass for WRAM and HRAM.

**Agent task:**

```text
Implement Bus::write8 for WRAM, HRAM, and IE.
Ignore writes to ROM for ROM-only cartridge.
Add tests.
```

---

## 3.4 Add 16-bit bus helpers

**Goal:** Add little-endian `read16` and `write16`.

**You learn:** Game Boy stores 16-bit immediates little-endian.

**Done when:** Writing `0x1234` stores low byte first.

**Agent task:**

```text
Add Bus::read16 and Bus::write16 using little-endian ordering.
Add tests.
```

---

# Stage 4: CPU fetch and tiny execution loop

## 4.1 CPU fetch byte

**Goal:** CPU reads opcode at PC and increments PC.

**You learn:** Instruction execution starts with fetch.

**Done when:** PC increments after fetching.

**Agent task:**

```text
Add Cpu::fetch8(&mut self, bus: &Bus) -> u8.
It should read at PC and increment PC wrapping correctly.
Add tests.
```

---

## 4.2 CPU fetch word

**Goal:** Fetch 16-bit immediate operands.

**You learn:** Multi-byte instructions.

**Done when:** Bytes `[0x34, 0x12]` become `0x1234`.

**Agent task:**

```text
Add Cpu::fetch16 using two fetch8 calls, little-endian.
Add tests.
```

---

## 4.3 Implement `NOP`

Opcode:

```text
0x00 NOP
```

**You learn:** The basic instruction loop.

**Done when:** CPU executes one NOP and returns correct cycles.

**Agent task:**

```text
Implement Cpu::step with opcode 0x00 NOP only.
Unknown opcodes should return a clear error including PC and opcode.
Add tests.
```

---

## 4.4 Add trace logging

**Goal:** Print CPU state before/after instruction.

**You learn:** Debugging emulators is mostly trace archaeology.

**Done when:** You can get lines like:

```text
PC=0100 OP=00 AF=01B0 BC=0013 DE=00D8 HL=014D SP=FFFE
```

**Agent task:**

```text
Add optional CPU trace formatting.
Do not print directly from gb-core.
Return or format a trace string that callers can use.
Add tests for stable formatting.
```

---

# Stage 5: Load instructions

## 5.1 Immediate 8-bit loads

Implement:

```text
LD B,d8
LD C,d8
LD D,d8
LD E,d8
LD H,d8
LD L,d8
LD A,d8
```

**You learn:** Instructions often differ only by target register.

**Done when:** Fake ROM can set each register.

**Agent task:**

```text
Implement immediate 8-bit LD r,d8 instructions.
Use explicit match arms.
Add tests for each target register.
```

---

## 5.2 Immediate 16-bit loads

Implement:

```text
LD BC,d16
LD DE,d16
LD HL,d16
LD SP,d16
```

**You learn:** Instruction operands and endian order.

**Done when:** Register pairs get expected values.

**Agent task:**

```text
Implement LD rr,d16 for BC, DE, HL, SP.
Add tests.
```

---

## 5.3 Register-to-register loads

Implement the `LD r,r` family, except illegal/odd cases as needed.

**You learn:** Opcode patterns.

**Done when:** You can move values between A/B/C/D/E/H/L.

**Agent task:**

```text
Implement LD r,r instructions for A, B, C, D, E, H, L.
Avoid macro generation.
Add table-style tests.
```

---

## 5.4 Memory loads through HL

Implement:

```text
LD A,(HL)
LD (HL),A
LD (HL),d8
```

**You learn:** Registers can point into memory.

**Done when:** CPU can read/write via address in HL.

**Agent task:**

```text
Implement basic HL indirect loads.
Add tests using WRAM addresses.
```

---

## 5.5 Special A memory loads

Implement:

```text
LD A,(BC)
LD A,(DE)
LD (BC),A
LD (DE),A
LDH A,(a8)
LDH (a8),A
LD A,(a16)
LD (a16),A
```

**You learn:** IO memory uses special compact addressing.

**Done when:** You can write/read high RAM via `0xFF00 + n`.

**Agent task:**

```text
Implement special A load/store instructions involving BC, DE, a8 high-memory, and a16.
Add tests.
```

---

# Stage 6: Arithmetic and flags

## 6.1 INC and DEC 8-bit registers

Implement:

```text
INC r
DEC r
```

**You learn:** Flags are as important as values.

**Done when:** Z/N/H flags behave correctly.

**Agent task:**

```text
Implement INC and DEC for 8-bit registers.
Correctly set Z, N, H and preserve C.
Add edge-case tests for 0x00, 0x0F, 0x10, 0xFF.
```

---

## 6.2 ADD A,r

**Goal:** Add register values to A.

**You learn:** Carry and half-carry.

**Done when:** Edge cases pass.

**Agent task:**

```text
Implement ADD A,r for all 8-bit registers.
Set Z, N, H, C correctly.
Add edge-case tests.
```

---

## 6.3 ADC, SUB, SBC

**Goal:** Add/subtract with carry.

**You learn:** Carry flag as input.

**Done when:** Carry-in and borrow cases pass.

**Agent task:**

```text
Implement ADC A,r, SUB A,r, and SBC A,r.
Add tests covering carry, half-carry, zero, and borrow behavior.
```

---

## 6.4 AND, OR, XOR, CP

**Goal:** Implement logical operations.

**You learn:** Compare is subtract without storing.

**Done when:** Flags match expected behavior.

**Agent task:**

```text
Implement AND, OR, XOR, and CP for A with registers.
Add tests for flags and result preservation for CP.
```

---

## 6.5 Immediate arithmetic

Implement:

```text
ADD A,d8
ADC A,d8
SUB d8
SBC A,d8
AND d8
OR d8
XOR d8
CP d8
```

**You learn:** Same ALU, different operand source.

**Done when:** Immediate variants reuse tested logic.

**Agent task:**

```text
Implement immediate ALU instructions by reusing internal ALU helpers.
Add tests.
```

---

## 6.6 `DAA`, `CPL`, `SCF`, `CCF`

**Goal:** Implement the weird flaggy instructions.

**You learn:** CPU historical baggage.

**Done when:** `DAA` test cases pass.

**Agent task:**

```text
Implement DAA, CPL, SCF, and CCF.
For DAA, include a table of known edge-case tests.
Explain the algorithm in comments.
```

`DAA` is the cursed little accountant under the floorboards. Treat it with ritual care.

---

# Stage 7: 16-bit arithmetic

## 7.1 INC/DEC 16-bit registers

Implement:

```text
INC BC
INC DE
INC HL
INC SP
DEC BC
DEC DE
DEC HL
DEC SP
```

**You learn:** Not all arithmetic affects flags.

**Done when:** Values wrap correctly.

**Agent task:**

```text
Implement 16-bit INC/DEC for BC, DE, HL, SP.
Ensure flags are unaffected.
Add tests.
```

---

## 7.2 ADD HL,rr

Implement:

```text
ADD HL,BC
ADD HL,DE
ADD HL,HL
ADD HL,SP
```

**You learn:** 16-bit half-carry rules.

**Done when:** H/C flags are correct.

**Agent task:**

```text
Implement ADD HL,rr.
Set N=false, H and C correctly, preserve Z.
Add edge-case tests.
```

---

## 7.3 Stack pointer arithmetic

Implement:

```text
ADD SP,e8
LD HL,SP+e8
LD SP,HL
```

**You learn:** Signed immediates and awkward flags.

**Done when:** Positive and negative offsets work.

**Agent task:**

```text
Implement ADD SP,e8, LD HL,SP+e8, and LD SP,HL.
Handle e8 as signed i8.
Add tests for positive and negative offsets plus flag behavior.
```

---

# Stage 8: Jumps, calls, returns

## 8.1 Absolute jumps

Implement:

```text
JP a16
JP HL
```

**You learn:** PC is just another register until it ruins your evening.

**Done when:** PC changes correctly.

**Agent task:**

```text
Implement JP a16 and JP HL.
Add tests.
```

---

## 8.2 Relative jumps

Implement:

```text
JR e8
```

**You learn:** Signed offsets from current PC.

**Done when:** Forward and backward jumps work.

**Agent task:**

```text
Implement JR e8 with signed offset from PC after operand fetch.
Add tests for forward and backward jumps.
```

---

## 8.3 Conditional jumps

Implement conditions:

```text
NZ
Z
NC
C
```

For:

```text
JP cc,a16
JR cc,e8
```

**You learn:** Different cycle counts depending on branch taken.

**Done when:** Taken and not-taken cases pass.

**Agent task:**

```text
Implement conditional JP and JR.
Return correct cycles for taken and not taken branches.
Add tests.
```

---

## 8.4 Stack push/pop primitives

**Goal:** Implement stack memory behavior.

**You learn:** Stack grows downward.

**Done when:** Push then pop returns same value and SP is restored.

**Agent task:**

```text
Add Cpu::push16 and Cpu::pop16 using Bus.
Stack grows downward.
Add tests.
```

---

## 8.5 CALL and RET

Implement:

```text
CALL a16
RET
```

**You learn:** Function calls are stack tricks.

**Done when:** CALL pushes return address, RET restores it.

**Agent task:**

```text
Implement CALL a16 and RET.
Add tests checking PC and SP.
```

---

## 8.6 Conditional CALL/RET and RST

Implement:

```text
CALL cc,a16
RET cc
RST vec
```

**You learn:** Compact interrupt-like calls.

**Done when:** All vectors and conditions pass.

**Agent task:**

```text
Implement conditional CALL/RET and RST instructions.
Add tests for all RST vectors.
```

---

# Stage 9: Rotate, shift, bit operations

## 9.1 Non-CB rotates

Implement:

```text
RLCA
RLA
RRCA
RRA
```

**You learn:** Carry as a rotate participant.

**Done when:** A and C flag behave correctly.

**Agent task:**

```text
Implement RLCA, RLA, RRCA, RRA.
Add edge-case tests.
```

---

## 9.2 CB-prefixed decode

**Goal:** Recognize `0xCB` and fetch second opcode.

**You learn:** Prefix instruction tables.

**Done when:** Unknown CB opcode gives useful error.

**Agent task:**

```text
Add CB-prefixed opcode dispatch.
Do not implement CB operations yet except a placeholder error.
Add tests for PC advancement.
```

---

## 9.3 CB rotate/shift register ops

Implement:

```text
RLC r
RRC r
RL r
RR r
SLA r
SRA r
SRL r
SWAP r
```

**You learn:** Bit manipulation and flags.

**Done when:** Register variants pass tests.

**Agent task:**

```text
Implement CB rotate, shift, and swap operations for registers.
Use shared helpers.
Add table tests.
```

---

## 9.4 CB bit/set/reset register ops

Implement:

```text
BIT b,r
SET b,r
RES b,r
```

**You learn:** Packed opcode patterns.

**Done when:** All bits 0-7 work on all registers.

**Agent task:**

```text
Implement BIT, SET, and RES for register operands.
Add table tests for representative opcodes and all bit positions.
```

---

## 9.5 CB operations on `(HL)`

**Goal:** Apply CB ops to memory pointed by HL.

**You learn:** Same operation, slower memory operand.

**Done when:** `(HL)` variants work and use correct cycles.

**Agent task:**

```text
Implement CB-prefixed operations for (HL).
Add tests using WRAM.
Ensure cycle counts differ from register operations.
```

---

# Stage 10: Interrupts and CPU control

## 10.1 Add interrupt registers

**Goal:** Model `IE` and `IF`.

```text
IE: 0xFFFF
IF: 0xFF0F
```

**You learn:** Interrupts are requested and enabled separately.

**Done when:** Bus reads/writes both registers.

**Agent task:**

```text
Implement IF at 0xFF0F and IE at 0xFFFF.
Add typed helpers for requesting and clearing interrupts.
```

---

## 10.2 Implement IME and EI/DI

Implement:

```text
EI
DI
```

**You learn:** Interrupt master enable.

**Done when:** IME changes correctly, including EI delay if you choose accuracy now.

**Agent task:**

```text
Add IME state to Cpu.
Implement DI and EI with documented behavior.
For now implement EI delayed by one instruction.
Add tests.
```

---

## 10.3 Implement interrupt servicing

**Goal:** CPU jumps to vectors.

**You learn:** Hardware can interrupt normal control flow.

**Done when:** Requested enabled VBlank interrupt pushes PC and jumps to `0x0040`.

**Agent task:**

```text
Implement interrupt servicing at the start of Cpu::step.
Priority order: VBlank, LCD STAT, Timer, Serial, Joypad.
Clear IF bit, clear IME, push PC, jump to vector.
Add tests.
```

---

## 10.4 HALT and STOP placeholder

Implement:

```text
HALT
STOP
```

**You learn:** CPU low-power states affect stepping.

**Done when:** HALT pauses instruction fetch until interrupt.

**Agent task:**

```text
Implement HALT state.
STOP may be a documented placeholder for now.
Add tests for HALT waking on interrupt.
```

---

# Stage 11: Timers

## 11.1 Implement DIV

**Goal:** Divider register increments with time.

**You learn:** Hardware ticks independently of CPU instructions.

**Done when:** After enough cycles, `DIV` changes.

**Agent task:**

```text
Implement DIV timer at 0xFF04.
Writing to DIV resets it.
Advance it through Bus::tick(cycles).
Add tests.
```

---

## 11.2 Implement TIMA/TMA/TAC

Registers:

```text
FF05 TIMA
FF06 TMA
FF07 TAC
```

**You learn:** Programmable timer frequencies.

**Done when:** TIMA increments at selected rate.

**Agent task:**

```text
Implement TIMA, TMA, TAC.
Support timer enable and frequency select.
Add tests for each frequency.
```

---

## 11.3 Timer overflow interrupt

**Goal:** TIMA overflow reloads TMA and requests Timer interrupt.

**You learn:** Timer links into interrupt system.

**Done when:** Overflow sets IF timer bit.

**Agent task:**

```text
When TIMA overflows, reload from TMA and request Timer interrupt.
Add tests.
```

---

# Stage 12: Serial debug

## 12.1 Implement serial registers

Registers:

```text
FF01 SB
FF02 SC
```

**You learn:** Test ROMs often report via serial.

**Done when:** Writes can be captured.

**Agent task:**

```text
Implement minimal serial registers SB and SC.
When SC is written with transfer start bit, expose the SB byte through a debug output buffer.
Add tests.
```

---

## 12.2 Add test ROM text output

**Goal:** Desktop/CLI prints serial output.

**You learn:** How emulator test ROMs communicate.

**Done when:** A test ROM can print readable text.

**Agent task:**

```text
Expose serial output from gb-core.
In gb-desktop or a CLI runner, print serial bytes as characters.
```

---

# Stage 13: First external CPU test ROMs

## 13.1 Add ROM runner mode

**Goal:** Run emulator headlessly for N steps/cycles.

**You learn:** Automated emulator testing.

**Done when:** CLI can run:

```bash
cargo run -p gb-desktop -- --headless test.gb --max-cycles 1000000
```

**Agent task:**

```text
Add a headless runner mode that loads a ROM, steps the emulator for a maximum cycle count, and prints serial output.
```

---

## 13.2 Run first CPU instruction test

**Goal:** Use an external test ROM.

**You learn:** Tests are your second compiler.

**Done when:** You get output, even if it fails.

**Agent task:**

```text
Add documentation for placing test ROMs locally.
Do not commit ROM files.
Add a command example for running them.
```

---

## 13.3 Fix CPU test failures one group at a time

**Goal:** Turn failures into targeted implementation fixes.

**You learn:** Trace comparison.

**Done when:** One test ROM passes.

**Agent task:**

```text
Given this failing serial output and CPU trace, identify the likely instruction group at fault.
Propose a minimal fix and tests before changing code.
```

This is where the agent becomes genuinely useful. You feed it failures. It proposes suspects. You approve the fix.

---

# Stage 14: PPU skeleton

## 14.1 Add PPU type and VRAM

**Goal:** Route VRAM through PPU.

Memory:

```text
0x8000-0x9FFF VRAM
```

**You learn:** Video memory is hardware-owned.

**Done when:** Bus reads/writes VRAM via PPU.

**Agent task:**

```text
Create Ppu with 8 KiB VRAM.
Route 0x8000-0x9FFF bus reads/writes through PPU.
Add tests.
```

---

## 14.2 Add OAM

Memory:

```text
0xFE00-0xFE9F OAM
```

**You learn:** Sprites live in separate attribute memory.

**Done when:** Bus reads/writes OAM.

**Agent task:**

```text
Add OAM storage to PPU and route 0xFE00-0xFE9F.
Return 0xFF for unusable 0xFEA0-0xFEFF.
Add tests.
```

---

## 14.3 Add LCD registers

Important registers:

```text
FF40 LCDC
FF41 STAT
FF42 SCY
FF43 SCX
FF44 LY
FF45 LYC
FF47 BGP
FF48 OBP0
FF49 OBP1
FF4A WY
FF4B WX
```

**You learn:** Video hardware is controlled through IO.

**Done when:** Registers can be read/written, with `LY` special-cased.

**Agent task:**

```text
Add basic LCD registers to PPU and route reads/writes.
LY should be read-only from CPU writes for now.
Add tests.
```

---

# Stage 15: Tile decoding and background rendering

## 15.1 Decode a single tile row

**Goal:** Convert tile bytes into colour indices.

Each tile row uses two bytes. Together they produce 8 pixels, each 0-3.

**You learn:** Planar 2-bit graphics.

**Done when:** Known tile bytes decode to expected pixel values.

**Agent task:**

```text
Implement a function to decode one Game Boy tile row into 8 colour indices.
Add unit tests with hand-calculated examples.
```

---

## 15.2 Decode full tile

**Goal:** Decode 16 bytes into an 8x8 tile.

**You learn:** Tiles are compact pixel programs.

**Done when:** Full tile tests pass.

**Agent task:**

```text
Implement tile decoding from 16 bytes into 8x8 colour indices.
Add tests.
```

---

## 15.3 Render background without scrolling

**Goal:** Use tile map and tile data to fill framebuffer.

Screen:

```text
160 x 144
```

**You learn:** Background maps are tile references.

**Done when:** A fake VRAM setup renders known pixels.

**Agent task:**

```text
Implement background rendering without scroll.
Use LCDC to select tile map and tile data region.
Render into a 160x144 framebuffer of colour indices.
Add tests for a simple tile map.
```

---

## 15.4 Apply BGP palette

**Goal:** Convert colour indices to actual greys.

**You learn:** Palette registers remap colour numbers.

**Done when:** Framebuffer outputs 4 DMG shades.

**Agent task:**

```text
Implement BGP palette mapping from 2-bit colour index to 4 greyscale shades.
Use a simple u32 framebuffer.
Add tests.
```

---

## 15.5 Add SCX/SCY scrolling

**Goal:** Background scroll registers work.

**You learn:** The screen is a viewport into a larger background.

**Done when:** Changing SCX/SCY shifts rendered pixels.

**Agent task:**

```text
Add SCX and SCY support to background rendering.
Add tests using fake tile maps.
```

---

# Stage 16: PPU timing

## 16.1 Add PPU tick and LY increment

**Goal:** Advance PPU with cycles.

**You learn:** Screen timing is scanline-based.

Basic model:

```text
456 cycles per scanline
154 lines per frame
Visible: 0-143
VBlank: 144-153
```

**Done when:** LY increments every 456 cycles.

**Agent task:**

```text
Implement Ppu::tick(cycles) with LY incrementing every 456 cycles.
Wrap LY after line 153.
Add tests.
```

---

## 16.2 Request VBlank interrupt

**Goal:** When LY becomes 144, request VBlank.

**You learn:** PPU drives interrupts.

**Done when:** IF VBlank bit is set at line 144.

**Agent task:**

```text
When PPU enters line 144, request VBlank interrupt through the bus/interrupt interface.
Add tests.
```

---

## 16.3 Add PPU modes

Modes:

```text
2 OAM
3 Transfer
0 HBlank
1 VBlank
```

**You learn:** PPU does different things during a scanline.

**Done when:** STAT mode bits change over a scanline.

**Agent task:**

```text
Implement basic PPU mode timing for OAM, Transfer, HBlank, VBlank.
Expose mode bits through STAT.
Add tests for mode transitions.
```

---

## 16.4 Render line-by-line

**Goal:** Render visible scanline as PPU reaches it.

**You learn:** Real hardware draws progressively.

**Done when:** Each visible line is rendered once per frame.

**Agent task:**

```text
Change PPU rendering to render one scanline during the appropriate part of PPU timing.
Keep a full framebuffer.
Add tests where possible.
```

---

# Stage 17: Desktop display

## 17.1 Create a window

**Goal:** Open a window with scaled Game Boy resolution.

**You learn:** Frontend boundary.

**Done when:** Window shows a blank frame.

**Agent task:**

```text
Use pixels + winit to create a window that displays a 160x144 framebuffer scaled up.
Keep all frontend code in gb-desktop.
```

---

## 17.2 Display emulator framebuffer

**Goal:** Feed PPU framebuffer to the window.

**You learn:** Core/frontend separation.

**Done when:** Fake framebuffer pattern appears.

**Agent task:**

```text
Expose framebuffer from gb-core and display it in gb-desktop.
Add a temporary checkerboard/demo mode if needed.
```

---

## 17.3 Run emulator frame loop

**Goal:** Step emulator until a frame is ready, then present.

**You learn:** Frame pacing.

**Done when:** Emulator continuously updates window.

**Agent task:**

```text
Add a main loop that steps the emulator until the PPU reports a completed frame, then presents the framebuffer.
```

---

# Stage 18: Joypad

## 18.1 Add Joypad type

**Goal:** Track button state.

Buttons:

```text
A, B, Start, Select, Up, Down, Left, Right
```

**You learn:** Input is also memory-mapped hardware.

**Done when:** Joypad can store pressed/released state.

**Agent task:**

```text
Create a Joypad type that tracks all eight Game Boy buttons.
Add tests.
```

---

## 18.2 Implement `FF00` joypad register

**Goal:** Reads from `0xFF00` return selected button group.

**You learn:** Joypad register uses selection bits.

**Done when:** Tests prove d-pad/action selection works.

**Agent task:**

```text
Implement FF00 joypad register behavior for direction and action button groups.
Remember Game Boy button bits are active-low.
Add tests.
```

---

## 18.3 Map keyboard to joypad

**Goal:** Desktop input works.

Suggested mapping:

```text
Arrows      D-pad
Z           A
X           B
Enter       Start
Right Shift Select
```

**You learn:** Frontend sends state to core.

**Done when:** Pressing keys changes `FF00`.

**Agent task:**

```text
Map keyboard events in gb-desktop to gb-core Joypad state.
Do not put windowing types into gb-core.
```

---

## 18.4 Joypad interrupt

**Goal:** Button press requests interrupt.

**You learn:** Input can wake games/CPU.

**Done when:** Pressing a button sets joypad IF bit.

**Agent task:**

```text
Request Joypad interrupt when a button transitions from released to pressed.
Add tests.
```

---

# Stage 19: DMA and sprites

## 19.1 Implement OAM DMA

Register:

```text
FF46 DMA
```

**Goal:** Copy 160 bytes into OAM.

**You learn:** Hardware can copy memory behind the CPU’s back.

**Done when:** Writing `0xC0` to `FF46` copies from `0xC000` to OAM.

**Agent task:**

```text
Implement OAM DMA register FF46.
Writing value N copies 160 bytes from N << 8 into OAM.
Add tests.
```

---

## 19.2 Parse sprite attributes

**Goal:** Interpret OAM entries.

Each sprite is 4 bytes:

```text
Y
X
Tile index
Flags
```

**You learn:** Sprites are metadata plus tile graphics.

**Done when:** OAM entries parse into Sprite structs.

**Agent task:**

```text
Add Sprite/OamEntry parsing from OAM bytes.
Decode x, y, tile index, priority, flips, palette.
Add tests.
```

---

## 19.3 Render basic 8x8 sprites

**Goal:** Draw sprites over background.

**You learn:** Sprite coordinate quirks.

**Done when:** Fake sprite appears at expected place.

**Agent task:**

```text
Implement basic 8x8 sprite rendering without priority complexities.
Respect transparent colour index 0.
Add tests for simple cases.
```

---

## 19.4 Sprite flipping and palettes

**Goal:** Support X/Y flip and OBP0/OBP1.

**You learn:** Sprite attributes.

**Done when:** Flipped test sprites render correctly.

**Agent task:**

```text
Implement sprite x-flip, y-flip, and palette selection.
Add tests.
```

---

## 19.5 Sprite priority

**Goal:** Handle background/sprite priority rules.

**You learn:** Rendering order matters.

**Done when:** Priority test ROMs look closer.

**Agent task:**

```text
Implement DMG sprite priority behavior against background colour index 0/non-zero.
Add tests for representative cases.
```

---

## 19.6 8x16 sprites

**Goal:** Support tall sprites.

**You learn:** LCDC changes sprite interpretation.

**Done when:** 8x16 sprite test renders.

**Agent task:**

```text
Implement 8x16 sprite mode controlled by LCDC.
Add tests.
```

---

# Stage 20: Window layer

## 20.1 Basic window rendering

Registers:

```text
WX, WY
```

**Goal:** Draw window tile map.

**You learn:** The Game Boy has a second background-like layer.

**Done when:** A fake window overlay appears.

**Agent task:**

```text
Implement basic DMG window rendering using LCDC window enable and tile map select.
Respect WX/WY.
Add tests.
```

---

## 20.2 Window timing quirks, later

**Goal:** Improve compatibility.

**You learn:** The window is weird.

**Done when:** Window test ROMs improve.

**Agent task:**

```text
Improve window behavior based on failing test ROM output.
Keep changes minimal and documented.
```

This one is deliberately later because exact window behavior is where the PPU goblin starts taking hostages.

---

# Stage 21: MBC1 cartridge support

## 21.1 Add cartridge trait/enum design

**Goal:** Support multiple cartridge controllers.

**You learn:** The cartridge is hardware, not storage.

**Done when:** ROM-only still works through the new abstraction.

**Agent task:**

```text
Refactor Cartridge to support cartridge controller variants.
Start with RomOnly variant.
Do not implement MBC1 behavior yet.
Add regression tests.
```

---

## 21.2 Implement MBC1 ROM banking

**Goal:** Switch banks in `0x4000..=0x7FFF`.

**You learn:** Banked memory.

**Done when:** Writes to MBC registers change readable ROM bank.

**Agent task:**

```text
Implement MBC1 ROM banking.
Support lower 5-bit bank register and upper bank bits.
Add tests using synthetic multi-bank ROM data.
```

---

## 21.3 Implement MBC1 RAM banking

**Goal:** External RAM works.

**You learn:** Save RAM and bank mode.

**Done when:** External RAM read/write works when enabled.

**Agent task:**

```text
Implement MBC1 external RAM enable and RAM banking.
Add tests.
```

---

## 21.4 Add save RAM persistence

**Goal:** Save files.

**You learn:** Emulator frontend owns persistence.

**Done when:** RAM can be extracted/restored from core.

**Agent task:**

```text
Expose cartridge RAM save data from gb-core and allow restoring it.
In gb-desktop, save/load .sav files next to the ROM.
```

---

# Stage 22: More cartridge support

## 22.1 MBC3 without RTC

**Goal:** Support more games.

**You learn:** Cartridge variants.

**Done when:** MBC3 non-RTC games can bank ROM/RAM.

**Agent task:**

```text
Implement MBC3 ROM/RAM banking without RTC support.
Return documented placeholder behavior for RTC registers.
Add tests.
```

---

## 22.2 MBC5

**Goal:** Support larger later DMG games.

**You learn:** Wider ROM bank numbers.

**Done when:** Synthetic MBC5 bank tests pass.

**Agent task:**

```text
Implement MBC5 ROM/RAM banking.
Add tests.
```

---

# Stage 23: APU, optional but part of “full DMG”

You can defer this until the emulator is fun visually.

## 23.1 APU register skeleton

**Goal:** Bus routes audio registers.

**You learn:** Audio hardware has lots of state.

**Done when:** Games can read/write APU registers without chaos.

**Agent task:**

```text
Add APU register storage and bus routing for 0xFF10-0xFF3F.
Do not generate audio yet.
Add tests for read/write behavior where appropriate.
```

---

## 23.2 Frame sequencer

**Goal:** Add APU timing backbone.

**You learn:** Audio has its own timing schedule.

**Done when:** Frame sequencer ticks at expected intervals.

**Agent task:**

```text
Implement APU frame sequencer timing.
Add tests for sequencer step progression.
```

---

## 23.3 Square channel 1

**Goal:** Generate first audible tone.

**You learn:** Duty cycles, envelope, sweep.

**Done when:** A simple ROM produces a tone.

**Agent task:**

```text
Implement square channel 1 enough to generate samples.
Expose audio samples from gb-core.
Add basic tests for frequency timer behavior.
```

---

## 23.4 Square channel 2

**Goal:** Add second square channel.

**You learn:** Reuse without over-abstracting.

**Done when:** Channel 2 works.

**Agent task:**

```text
Implement square channel 2 using shared square channel logic where sensible.
Add tests.
```

---

## 23.5 Wave channel

**Goal:** Implement channel 3.

**You learn:** Programmable waveform RAM.

**Done when:** Wave RAM can generate samples.

**Agent task:**

```text
Implement APU wave channel and wave RAM.
Add tests for sample extraction.
```

---

## 23.6 Noise channel

**Goal:** Implement channel 4.

**You learn:** LFSR noise generation.

**Done when:** Noise channel produces plausible output.

**Agent task:**

```text
Implement noise channel using LFSR behavior.
Add tests for deterministic LFSR progression.
```

---

## 23.7 Audio backend

**Goal:** Play sound in desktop app.

**You learn:** Buffering and sync.

**Done when:** Audio plays without destroying frame pacing.

**Agent task:**

```text
Add an audio output backend in gb-desktop.
Keep audio generation in gb-core.
Use a ring buffer or callback-safe design.
```

---

# Stage 24: Accuracy and compatibility pass

## 24.1 Add instruction trace compare tooling

**Goal:** Compare your CPU trace with known-good traces.

**You learn:** Debugging by divergence.

**Done when:** Tool shows first mismatching line.

**Agent task:**

```text
Create a trace comparison utility that compares emulator trace output against a reference trace and reports first divergence.
```

---

## 24.2 Add screenshot test harness

**Goal:** Compare rendered output.

**You learn:** PPU regression testing.

**Done when:** A test ROM can produce a saved PNG.

**Agent task:**

```text
Add a headless mode that runs for N frames and writes framebuffer PNG.
Do not depend on desktop windowing.
```

---

## 24.3 Run PPU test ROMs

**Goal:** Improve rendering correctness.

**You learn:** Which PPU details are still wrong.

**Done when:** One PPU test improves from fail to pass.

**Agent task:**

```text
Given this screenshot/result from a PPU test ROM, identify likely missing behavior.
Propose one minimal fix.
```

---

## 24.4 Run compatibility smoke list

**Goal:** Try a small set of common DMG games.

**You learn:** Real games expose integration bugs.

**Done when:** You maintain a compatibility matrix.

Example matrix:

```text
Game       Boots   Title   Input   Gameplay   Audio
Tetris     yes     yes     yes     yes        no
Dr Mario   yes     yes     yes     partial    no
Kirby      yes     yes     yes     partial    no
```

**Agent task:**

```text
Create docs/compatibility.md with columns for boot, render, input, gameplay, audio, notes.
```

---

# Stage 25: Developer tools

These are optional, but they make the project feel powerful.

## 25.1 CPU debugger panel

**Goal:** Display registers and current instruction.

**You learn:** Emulator observability.

**Done when:** You can pause and step one instruction.

**Agent task:**

```text
Add a simple debug mode to gb-desktop showing CPU registers and allowing pause/step.
Use egui or a simple text overlay.
```

---

## 25.2 VRAM viewer

**Goal:** Show tile data.

**You learn:** How games construct graphics.

**Done when:** You can see tiles update live.

**Agent task:**

```text
Add a debug-only VRAM tile viewer.
Keep it outside gb-core.
```

---

## 25.3 Background map viewer

**Goal:** Show full 256x256 background.

**You learn:** Scroll viewport vs full map.

**Done when:** You can see where the screen sits on the background.

**Agent task:**

```text
Add debug rendering for the full background map and current viewport rectangle.
```

---

## 25.4 OAM/sprite viewer

**Goal:** Inspect sprites.

**You learn:** Sprite attribute memory.

**Done when:** Debug UI lists active sprites.

**Agent task:**

```text
Add a debug view of OAM entries with x, y, tile, flags, and preview.
```

---

# The condensed milestone map

Here’s the whole thing as a scannable campaign map:

```text
0.1  Workspace
0.2  Architecture rules
0.3  Test ROM helpers

1.1  Raw ROM loading
1.2  Cartridge title
1.3  Cartridge type/ROM/RAM size
1.4  Header checksum
1.5  ROM-only reads

2.1  CPU registers
2.2  Register pairs
2.3  Flags
2.4  Post-boot state

3.1  Bus type
3.2  Bus reads
3.3  Bus writes
3.4  16-bit bus helpers

4.1  Fetch byte
4.2  Fetch word
4.3  NOP
4.4  Trace logging

5.1  LD r,d8
5.2  LD rr,d16
5.3  LD r,r
5.4  LD via HL
5.5  Special A loads

6.1  INC/DEC 8-bit
6.2  ADD
6.3  ADC/SUB/SBC
6.4  AND/OR/XOR/CP
6.5  Immediate ALU
6.6  DAA/CPL/SCF/CCF

7.1  INC/DEC 16-bit
7.2  ADD HL,rr
7.3  SP arithmetic

8.1  JP
8.2  JR
8.3  Conditional jumps
8.4  Stack primitives
8.5  CALL/RET
8.6  Conditional CALL/RET/RST

9.1  Non-CB rotates
9.2  CB dispatch
9.3  CB rotate/shift
9.4  CB BIT/SET/RES
9.5  CB (HL)

10.1 Interrupt registers
10.2 IME/EI/DI
10.3 Interrupt service
10.4 HALT/STOP

11.1 DIV
11.2 TIMA/TMA/TAC
11.3 Timer interrupt

12.1 Serial registers
12.2 Serial test output

13.1 Headless ROM runner
13.2 First CPU test ROM
13.3 Fix CPU failures

14.1 PPU + VRAM
14.2 OAM
14.3 LCD registers

15.1 Decode tile row
15.2 Decode full tile
15.3 Render background
15.4 Apply BGP palette
15.5 SCX/SCY scrolling

16.1 PPU tick + LY
16.2 VBlank interrupt
16.3 PPU modes
16.4 Line rendering

17.1 Window
17.2 Display framebuffer
17.3 Frame loop

18.1 Joypad state
18.2 FF00 register
18.3 Keyboard mapping
18.4 Joypad interrupt

19.1 OAM DMA
19.2 Sprite parsing
19.3 Basic sprites
19.4 Sprite flips/palettes
19.5 Sprite priority
19.6 8x16 sprites

20.1 Window layer
20.2 Window quirks

21.1 Cartridge controller abstraction
21.2 MBC1 ROM banking
21.3 MBC1 RAM banking
21.4 Save RAM

22.1 MBC3 without RTC
22.2 MBC5

23.1 APU register skeleton
23.2 Frame sequencer
23.3 Square 1
23.4 Square 2
23.5 Wave
23.6 Noise
23.7 Audio backend

24.1 Trace compare
24.2 Screenshot harness
24.3 PPU test ROMs
24.4 Compatibility matrix

25.1 CPU debugger
25.2 VRAM viewer
25.3 Background viewer
25.4 OAM viewer
```