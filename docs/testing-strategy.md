# Agent-First Testing Strategy

This project should make test failures easy for coding agents to understand and act on. The ideal feedback loop is fast, deterministic, narrow enough to locate the fault, and explicit about the hardware behaviour being checked.

## Goals

* Give agents short, high-signal failure output.
* Keep most tests small enough to run after every milestone.
* Encode hardware behaviour in readable test names and fixtures.
* Prefer deterministic in-repo tests over manual inspection.
* Leave enough context in assertion messages for agents to patch the right subsystem.
* Separate quick local checks from slower compatibility or visual checks.

## Default Agent Loop

Agents should use this loop for normal milestones:

```bash
cargo fmt
cargo test
cargo clippy --all-targets --all-features
```

When a failure occurs, rerun the narrowest useful test before editing:

```bash
cargo test -p gb-core cartridge_header
cargo test -p gb-core cpu::registers
cargo test -p gb-core --test bus_routing
```

After a fix, rerun the narrow test first, then the default loop.

## Test Layers

### Unit tests

Use unit tests for small pure logic:

* CPU register pairs and flags.
* ALU helpers.
* Cartridge header parsing.
* Interrupt flag wrappers.
* Timer register behaviour.
* Tile decoding.

Unit tests should live near the implementation when private details matter. Keep names behaviour-focused, such as `set_af_masks_lower_flag_nibble`.

### Integration tests

Use integration tests for subsystem boundaries:

* Bus address routing.
* Cartridge ROM/RAM reads and writes.
* CPU instruction execution through the bus.
* Serial output collection.
* Save RAM import and export.

Integration tests should use shared helpers from `crates/gb-core/tests/common`. Helpers in this tree are test-only and should not become production API.

### ROM-style tests

Use tiny generated ROM images for CPU and bus behaviour where executing from `0x0100` is useful. The helper in `crates/gb-core/tests/common` creates a minimal 32 KiB ROM with configurable bytes at the entry point.

Prefer tiny generated ROMs for focused behaviour. External test ROMs should be optional and documented, not committed unless their license permits it.

### Golden tests

Use golden outputs when a compact trace is more useful than many isolated assertions:

* CPU trace lines.
* Serial test output.
* Small memory snapshots.

Golden output should be short and reviewed. Avoid large files that hide the behavioural reason for a failure.

### Visual tests

PPU tests should start with data-level assertions:

* Decoded tile rows.
* Background map fetches.
* Sprite priority decisions.
* Framebuffer hashes for tiny deterministic scenes.

Full screenshot comparisons can come later. Store generated screenshots outside git by default, under `screenshots/`.

### Accuracy ROM gates

The accuracy phase should use focused external ROM gates. Do not treat the whole
local ROM inventory as the pass/fail condition for every timing milestone.

Choose a small set that matches the subsystem being changed:

* CPU bus sequencing: Blargg `instr_timing`, `mem_timing`, and focused generated ROM tests.
* Timer: Mooneye timer acceptance tests plus timer unit tests for DIV/TAC/TIMA edge cases.
* Interrupts and HALT: Blargg `halt_bug`, AGE halt tests, and Mooneye interrupt timing tests.
* DMA: Mooneye `oam_dma_*`, Blargg memory timing, and GBMicrotest DMA cases.
* PPU bus access and STAT: AGE `vram`, `oam`, `stat`, and GBMicrotest PPU/STAT cases.

Record the exact ROM gates used in the milestone entry. If a gate remains
failing, record the observed failure and the next suspected hardware rule.

## Agent-Friendly Failure Style

Assertions should expose the hardware context:

```rust
assert_eq!(
    registers.pc, 0x0101,
    "PC should advance by one after NOP at 0x0100"
);
```

Prefer direct expected values over derived expected values when that makes the hardware rule visible. For table tests, include the opcode, input registers, flags, and expected cycles in the case name or failure message.

Avoid tests that only say `assert failed`. Agents need the failing address, opcode, register, bit, or cycle count.

## Test Fixtures

Keep fixtures small and explicit:

* Use byte arrays or helper builders for tiny ROMs.
* Use named constants for hardware addresses.
* Keep generated ROM bytes close to the test that explains them.
* Do not add broad fixture frameworks before repeated pain justifies them.

Shared helpers should be boring and stable. They should reduce setup noise without hiding the behaviour under test.

## Tooling Wishlist

These tools should be added only when the corresponding subsystem exists:

* `cargo xtask test-fast`: format check, unit tests, integration tests, clippy.
* `cargo xtask test-roms`: run local external ROM suites from `test-roms/`.
* `cargo xtask trace`: run a ROM and emit compact CPU trace lines.
* `cargo xtask compare-trace`: compare trace output against a short expected file.
* `cargo xtask screenshot`: produce deterministic PPU screenshots under `screenshots/`.

An `xtask` is preferred once command sequences become repetitive because it gives future agents one stable command and one place to improve diagnostics.

## External Test ROM Policy

Do not commit external test ROMs unless the license clearly allows redistribution. Instead:

* Document where to put local ROMs under `test-roms/`.
* Make test commands skip gracefully when optional ROMs are absent.
* Print the expected local path and suite name when skipping.
* Keep mandatory CI tests independent from local ROM availability.

## CI Shape

Early CI should run:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

As the emulator grows, split CI into:

* Fast required checks for every change.
* Optional or scheduled external ROM suites.
* Optional visual or trace comparisons.

The required lane should stay fast enough that agents can reason from its output during normal development.

## Milestone Expectations

Every emulator milestone should include one of:

* Tests for new behaviour.
* A documented reason tests are not practical yet.
* A test-helper improvement that makes future behaviour easier to test.

Milestone records should list the exact commands run and any skipped optional checks.

For timing milestones, the record should also name:

* The timing rule being modelled.
* The external ROM gates used, if any.
* Whether the change is foundational or a compatibility patch.
* Any remaining ordering uncertainty.
