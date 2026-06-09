# Milestones

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
