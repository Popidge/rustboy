# Milestones

## 0019: Implement serial debug output

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 12 by adding serial registers, exposing collected serial bytes from core, and printing serial text from the desktop CLI runner.

### Changes

- Added a minimal `Serial` component with `SB` and `SC` register behavior.
- Routed serial registers at `0xFF01..=0xFF02` through `Bus`.
- Added `Bus::serial_output` and `Bus::take_serial_output`.
- Added `gb-desktop <rom.gb> --serial-steps N` to execute a bounded number of CPU steps and print collected serial text.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- `cargo run -p gb-desktop --quiet -- <generated serial smoke ROM> --serial-steps 8`
- Added serial unit tests, bus routing tests, and a CPU ROM-style test that emits `OK` through `LDH` writes to `SB`/`SC`.

### Decisions

- Kept serial output buffered in `gb-core`; printing belongs to `gb-desktop`.
- Cleared the SC transfer-start bit immediately after capturing `SB` for this minimal debug implementation.
- Used an opt-in bounded CLI step count instead of an open-ended runner.
- Added no dependencies.

### Notes

- The generated CLI smoke ROM printed `Serial: OK`.
- A fuller ROM runner is still left to Stage 13.

## 0018: Implement timers

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 11 by adding DIV, TIMA, TMA, TAC, bus ticking, and Timer interrupt requests.

### Changes

- Added a `Timer` component with `DIV`, `TIMA`, `TMA`, and `TAC` register behavior.
- Routed timer registers at `0xFF04..=0xFF07` through `Bus`.
- Added `Bus::tick(TCycles)` to advance bus-owned hardware.
- Reloaded `TIMA` from `TMA` and requested the Timer interrupt on overflow.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added timer unit tests for DIV reset/increment, all TAC frequencies, and overflow interrupt behavior.
- Added a bus integration test for timer register routing and interrupt requests through `Bus::tick`.

### Decisions

- Used T-cycles for all timer periods: 1024, 16, 64, and 256.
- Implemented immediate TIMA reload on overflow for this milestone; delayed reload edge cases can be revisited during accuracy work.
- Added no dependencies.

### Notes

- Serial output is the remaining stage in this goal range.

## 0017: Implement interrupts and CPU control

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 10 by routing interrupt registers, adding IME control, servicing interrupts, and modelling HALT/STOP control states.

### Changes

- Added typed `Interrupt` and `InterruptFlags` helpers with DMG priority order and vectors.
- Routed `IF` at `0xFF0F` and masked `IE` at `0xFFFF` through the bus.
- Added CPU `IME`, delayed `EI`, immediate `DI`, and interrupt servicing before opcode fetch.
- Added `HALT` idling/wake behavior and a documented `STOP` placeholder state.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added interrupt flag unit tests, bus routing tests for `IF`/`IE`, and CPU tests for `EI`/`DI`, servicing priority, HALT wake, and STOP placeholder behavior.

### Decisions

- Used a small `CpuRunState` enum for running, halted, and stopped states rather than separate booleans.
- Implemented `EI` as delayed until after the following instruction.
- Kept the HALT bug and detailed STOP behavior out of scope for this milestone.
- Added no dependencies.

### Notes

- Timer and serial hardware can now request interrupts through typed bus helpers.

## 0016: Implement rotate, shift, and bit operations

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 9 by adding accumulator rotates and CB-prefixed rotate, shift, bit, set, and reset operations.

### Changes

- Added `RLCA`, `RLA`, `RRCA`, and `RRA`.
- Added CB-prefixed dispatch and operand decoding for registers and `(HL)`.
- Added CB rotate, shift, `SWAP`, `BIT`, `RES`, and `SET` behavior.
- Applied longer cycle counts for `(HL)` CB operands.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added CPU tests for non-CB rotates, CB register rotate/shift/swap operations, all bit positions for `BIT`/`RES`/`SET`, and `(HL)` memory cycles.

### Decisions

- Kept CB decoding explicit and local to the CPU module without macro-generated tables.
- Used a small internal `CbOperand` enum to distinguish register operands from `(HL)`.
- Added no dependencies.

### Notes

- Interrupts, timers, and serial output remain the next roadmap stages.

## 0015: Implement CPU control flow instructions

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 8 by adding jumps, relative branches, stack primitives, calls, returns, and reset-vector calls.

### Changes

- Added `JP a16`, `JP HL`, `JR e8`, conditional `JP cc,a16`, and conditional `JR cc,e8`.
- Added CPU stack helpers for 16-bit push/pop through `Bus`.
- Added `CALL a16`, `RET`, conditional `CALL cc,a16`, conditional `RET cc`, and all `RST` vectors.
- Added a small internal `Condition` enum for NZ/Z/NC/C branch checks.

### Tests

- `cargo fmt`
- `cargo test -p gb-core cpu::tests`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added CPU unit tests for absolute and relative jumps, taken and not-taken conditional cycles, stack byte ordering, CALL/RET stack behavior, conditional CALL/RET, and every RST vector.

### Decisions

- Kept opcode dispatch explicit in the existing `step` match.
- Kept stack helpers private to `Cpu` for now; tests cover them from the CPU module without expanding the public API.
- Used `u8::cast_signed()` plus `wrapping_add_signed` for signed relative offsets.
- Added no dependencies.

### Notes

- Interrupt-driven stack use will build on the same push/pop helpers in Stage 10.
- Rotate/shift and CB-prefixed opcodes remain future milestones.

## 0014: Implement CPU arithmetic instructions

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stages 6 and 7 by adding 8-bit ALU instructions, flag-specific accumulator operations, and 16-bit/SP arithmetic.

### Changes

- Added `INC r` and `DEC r` for A, B, C, D, E, H, and L.
- Added register and immediate ALU operations: `ADD`, `ADC`, `SUB`, `SBC`, `AND`, `OR`, `XOR`, and `CP`.
- Added `DAA`, `CPL`, `SCF`, and `CCF`.
- Added 16-bit `INC rr`, `DEC rr`, `ADD HL,rr`, `ADD SP,e8`, `LD HL,SP+e8`, and `LD SP,HL`.
- Added focused ALU helpers for shared flag behavior.

### Tests

- `cargo fmt`
- `cargo test -p gb-core cpu::tests`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added CPU unit tests for wrapping arithmetic, zero/carry/half-carry/subtract flags, carry-in and borrow behavior, DAA edge cases, CP result preservation, 16-bit flag preservation, and signed stack-pointer offsets.

### Decisions

- Kept opcode dispatch explicit and allowed the long `step` match locally to preserve the learning-first opcode layout.
- Used shared ALU helpers for arithmetic flag rules while keeping opcode decoding plain.
- Treated signed `e8` operands with `u8::cast_signed()` and `wrapping_add_signed` so positive and negative offsets stay explicit.
- Added no dependencies.

### Notes

- Arithmetic involving the `(HL)` memory operand is not included yet; this milestone follows the roadmap wording for register and immediate arithmetic.
- Jump, stack, rotate/shift, CB-prefixed, interrupt, and control instructions remain future milestones.

## 0013: Implement CPU load instructions

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 5 tasks 5.1 through 5.5 by adding the first CPU load instruction groups.

### Changes

- Added immediate 8-bit loads for A, B, C, D, E, H, and L.
- Added immediate 16-bit loads for BC, DE, HL, and SP.
- Added register-to-register loads among A, B, C, D, E, H, and L.
- Added basic HL-indirect loads: `LD A,(HL)`, `LD (HL),A`, and `LD (HL),d8`.
- Added the remaining basic HL-indirect register loads: `LD r,(HL)` and `LD (HL),r` for A, B, C, D, E, H, and L.
- Added special A memory loads and stores through BC, DE, high-memory `a8`, and absolute `a16` addresses.
- Added small internal CPU helpers for 8-bit registers and register pairs.

### Tests

- `cargo fmt`
- `cargo test -p gb-core cpu::tests::ld_r_d8_sets_each_8_bit_register`
- `cargo test -p gb-core cpu::tests::ld_`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added CPU unit tests for each Stage 5 load group, including WRAM and HRAM bus-backed memory cases.

### Decisions

- Kept opcode dispatch as explicit `match` arms with no generated table or macro.
- Kept all memory-facing instruction behavior routed through `Bus`.
- Limited Stage 5.3 register-to-register coverage to A/B/C/D/E/H/L register operands; broader `(HL)` table variants remain future CPU work unless called out by a later milestone.
- Added no dependencies.

### Notes

- Load instructions do not modify flags, so this milestone intentionally leaves flag behavior unchanged.
- Arithmetic, jumps, stack operations, interrupts, and the remaining load-family variants are still future milestones.

## 0012: Add CPU fetch and tiny execution loop

Date: 2026-06-09

Status: Complete

### Goal

Implement roadmap Stage 4 tasks 4.1 through 4.4 by adding CPU byte/word fetch, one-instruction stepping for `NOP`, structured unknown-opcode errors, and trace formatting.

### Changes

- Added `TCycles` for explicit CPU T-cycle counts.
- Added `CpuError::UnimplementedOpcode` with the original opcode fetch PC and opcode byte.
- Added `Cpu::fetch8` and `Cpu::fetch16`, including wrapping PC increment behavior.
- Added `Cpu::step` with opcode `0x00` `NOP` returning 4 T-cycles.
- Added CPU trace formatting that reads the next opcode without mutating CPU state or printing from `gb-core`.

### Tests

- `cargo fmt`
- `cargo test -p gb-core cpu::tests::fetch8_wraps_pc_after_ffff`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added CPU unit tests for fetch byte, fetch word, PC wrapping, NOP stepping, unknown opcode errors, and stable trace output.

### Decisions

- Kept CPU memory access routed through `Bus`.
- Kept `Cpu::step` fallible because early CPU development should report unimplemented opcodes clearly instead of panicking.
- Kept trace output as returned strings so `gb-core` remains callback-free and does not print directly.
- Added no dependencies.

### Notes

- Only `NOP` is executable so far; load instructions begin in Stage 5.

## 0011: Add basic bus and memory map

Date: 2026-06-09

Status: Complete

### Goal

Implement roadmap Stage 3 tasks 3.1 through 3.4 by introducing the bus, routing the first memory regions, and adding little-endian 16-bit helpers.

### Changes

- Added `gb_core::bus::Bus`.
- Made `Bus` own `Cartridge`, WRAM, HRAM, interrupt enable, and interrupt flags storage.
- Added `Bus::read8` routing for cartridge ROM, WRAM, HRAM, and IE.
- Added `Bus::write8` routing for WRAM, HRAM, and IE, with ROM writes ignored for ROM-only cartridges.
- Added little-endian `Bus::read16` and `Bus::write16`.
- Returned `0xFF` for unsupported reads until later hardware components exist.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added `crates/gb-core/tests/bus_routing.rs` for ROM reads, unsupported reads, WRAM, HRAM, IE, ignored ROM writes, and little-endian 16-bit helpers.

### Decisions

- Kept fixed internal memory as arrays: `[u8; 0x2000]` for WRAM and `[u8; 0x7F]` for HRAM.
- Kept interrupt flags owned by `Bus` but not memory-routed yet; `0xFF0F` is scheduled for Stage 10.
- Converted cartridge read errors to `0xFF` at the bus boundary to match the runtime bus API.
- Added no dependencies.

### Notes

- PPU, timer, serial, joypad, APU, echo RAM, external RAM, and IF routing remain future milestones.

## 0010: Add CPU state

Date: 2026-06-09

Status: Complete

### Goal

Implement roadmap Stage 2 tasks 2.1 through 2.4 by modelling CPU registers, register pairs, flags, and the DMG post-boot CPU state.

### Changes

- Added `gb_core::cpu` with `Cpu`, `CpuRegisters`, and `CpuFlags`.
- Added A, F, B, C, D, E, H, L, SP, and PC register storage.
- Added AF, BC, DE, and HL get/set helpers.
- Added Z, N, H, and C flag get/set helpers.
- Added `Cpu::new_dmg_post_boot` with standard DMG post-boot register values.
- Added compact `Debug` formatting for CPU registers.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added unit tests for zeroed registers, register pairs, lower-nibble masking for F, each flag helper, register debug formatting, and post-boot CPU state.

### Decisions

- Represented `F` with a `CpuFlags` wrapper so the lower nibble is masked at the type boundary.
- Kept `Cpu` focused on register ownership only; instruction execution and bus access remain future milestones.
- Added no dependencies.

### Notes

- The post-boot constructor documents that we are seeding CPU state because the boot ROM is not executed yet.

## 0009: Implement ROM-only cartridge reads

Date: 2026-06-09

Status: Complete

### Goal

Implement milestone 1.5 by reading ROM-only cartridge bytes from addresses `0x0000..=0x7FFF`.

### Changes

- Added `Cartridge::read_rom`.
- Added `CartridgeReadError` for read-time cartridge failures.
- Returned a clear error for addresses outside the ROM range.
- Returned a clear error when a ROM-range address is not present in the loaded ROM bytes.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added unit tests for reading byte `0x0100`, rejecting `0x8000`, and rejecting a missing ROM byte.

### Decisions

- Kept cartridge read errors separate from cartridge loading errors.
- Used `Result<u8, CartridgeReadError>` at the cartridge component boundary; later bus reads can decide their hardware-facing fallback behaviour.
- Added no dependencies.

### Notes

- Bus routing for cartridge ROM remains future work.

## 0008: Validate cartridge header checksum

Date: 2026-06-09

Status: Complete

### Goal

Implement milestone 1.4 by validating the Game Boy cartridge header checksum.

### Changes

- Added header checksum calculation over bytes `0x0134..=0x014C`.
- Compared the computed checksum against byte `0x014D` during cartridge loading.
- Added `CartridgeError::InvalidHeaderChecksum` with expected and actual checksum bytes.
- Updated fake ROM test helpers to write valid checksum bytes by default.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added unit tests for valid and invalid header checksums.

### Decisions

- Header checksum validation now runs as part of `Cartridge::from_bytes`.
- Used wrapping `u8` arithmetic to match Game Boy checksum behaviour.
- Added no dependencies.

### Notes

- Existing fake ROM tests now build checksum-valid headers before parsing.

## 0007: Parse cartridge type and size codes

Date: 2026-06-09

Status: Complete

### Goal

Implement milestone 1.3 by decoding header bytes `0x0147`, `0x0148`, and `0x0149`, and printing a cartridge summary from the CLI.

### Changes

- Added decoded cartridge type, ROM size, and RAM size fields to `CartridgeHeader`.
- Added `Cartridge::cartridge_type`, `Cartridge::rom_size`, and `Cartridge::ram_size`.
- Updated `gb-desktop` to read a ROM path and print title, type, ROM size, and RAM size.
- Added `Display` and `Error` support for `CartridgeError`.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- `cargo run -p gb-desktop --quiet -- <fake TETRIS ROM>`
- Added unit tests for ROM ONLY, unsupported cartridge type codes, and ROM/RAM size code decoding.

### Decisions

- Only cartridge type `0x00` is supported as `ROM ONLY` for now.
- Nonzero cartridge type codes are represented as `Unsupported (0xNN)` instead of being decoded into mapper names early.
- ROM and RAM size codes decode to readable sizes, with unknown codes represented clearly.
- Added no dependencies; the CLI uses `std::env` and `std::fs`.

### Notes

- The fake CLI smoke test printed `Title: TETRIS`, `Type: ROM ONLY`, `ROM: 32 KiB`, and `RAM: None`.

## 0006: Parse cartridge title

Date: 2026-06-09

Status: Complete

### Goal

Implement milestone 1.2 by reading the cartridge title from header bytes `0x0134..=0x0143`.

### Changes

- Added `CartridgeHeader` for parsed header fields.
- Parsed the cartridge title during `Cartridge::from_bytes`.
- Added `Cartridge::title`.
- Added short-ROM validation for missing title header bytes.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added fake-ROM unit tests for title parsing, zero padding, and missing title bytes.

### Decisions

- Handled zero padding by stopping title parsing at the first `0x00` byte.
- Kept parsing limited to the title field; cartridge type, ROM size, RAM size, and checksum remain future work.
- Added no dependencies.

### Notes

- Title bytes are converted with UTF-8 lossless/lossy conversion for now; stricter header validation can be revisited when broader header parsing is added.

## 0005: Load raw cartridge ROM bytes

Date: 2026-06-09

Status: Complete

### Goal

Implement milestone 1.1 by adding a cartridge type that stores owned raw ROM bytes without parsing headers yet.

### Changes

- Added `gb_core::cartridge`.
- Added `Cartridge::from_bytes` for owned ROM loading.
- Added `Cartridge::len` for reporting stored ROM byte length.
- Added a minimal `CartridgeError` for empty ROM input.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added unit tests for successful raw ROM loading and empty ROM rejection.

### Decisions

- Kept cartridge ROM storage as `Vec<u8>`.
- Used `Result` for `from_bytes` setup, while avoiding header parsing or mapper selection.
- Added no dependencies.

### Notes

- Header parsing remains follow-up work for a later cartridge milestone.

## 0004: Add repository hygiene and testing strategy

Date: 2026-06-09

Status: Complete

### Goal

Add a sensible project `.gitignore` and document an agent-first testing strategy for emulator development.

### Changes

- Added `.gitignore` entries for Rust build output, local emulator artifacts, logs, editor files, and coverage output.
- Added `docs/testing-strategy.md`.
- Signposted the testing strategy from `AGENTS.md`.
- Corrected existing `AGENTS.md` references from `docs/style.md` to `docs/styleguide.md`.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`

### Decisions

- Kept optional ROMs, saves, traces, and screenshots out of git by default.
- Framed the testing plan around fast, deterministic, narrow feedback for future agent sessions.

### Notes

- Existing tracked files under `target/` will need to be removed from the git index separately if they were already committed.

## 0003: Add test ROM helper

Date: 2026-06-09

Status: Complete

### Goal

Add a test-only helper for constructing a minimal 32 KiB ROM image with configurable bytes at address 0x0100.

### Changes

- Added an integration-test helper under `crates/gb-core/tests/common`.
- Added tests covering ROM size, entry-point byte placement, default zero fill, and overflow rejection.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`

### Decisions

- Kept the helper in the test tree so it is not part of the `gb-core` production API.

### Notes

- This helper is ready for future CPU, bus, and cartridge tests that need minimal ROM bytes at `0x0100`.
