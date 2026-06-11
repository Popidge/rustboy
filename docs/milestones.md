# Milestones

## 0044: Convert CPU memory access by instruction group

Date: 2026-06-11

Status: Complete

### Goal

Convert load, stack, call/return, CB `(HL)`, and absolute/high-memory CPU access to ordered bus cycles in preparation for modelling SM83 fetch/execute overlap.

### Changes

- Added CPU operand fetch helpers that clock bus reads while preserving PC increment order.
- Converted load, high-memory, absolute-memory, `(HL)` ALU/inc/dec, and CB `(HL)` reads/writes to `Bus` CPU-cycle helpers.
- Converted stack push/pop, calls, returns, restarts, and related internal cycles to ordered bus reads, writes, and idle machine cycles.
- Adjusted interrupt service idle accounting now that stack writes are clocked.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Updated the private stack helper test to call the now-clocked `pop16` with mutable bus access.

### Decisions

- Kept debug/test-only unclocked memory helpers available for inspection paths.
- Treated this as foundational timing work, not a ROM-specific compatibility patch.
- Added no dependencies.

### Notes

- Whole-instruction cycle totals remain unchanged; `GameBoy::step` continues ticking any unclocked remainder for instruction groups not yet converted.
- The next timing step can focus on SM83 fetch/execute overlap and remaining internal-cycle ordering.

## 0043: Introduce clocked CPU bus access

Date: 2026-06-11

Status: Complete

### Goal

Add CPU-facing bus-cycle helpers and convert the first timing-sensitive CPU paths without broad instruction rewrites.

### Changes

- Added `Bus::cpu_fetch8`, `Bus::cpu_read8`, `Bus::cpu_write8`, and `Bus::cpu_idle_mcycle`.
- Clocked opcode fetches, including the HALT bug repeated fetch, through the bus.
- Clocked HALT idle and interrupt-service idle cycles through the bus.
- Kept `GameBoy::step` returning whole-instruction T-cycles while ticking only any unclocked remainder for instructions not yet converted.
- Added regression coverage for CPU bus helpers advancing DIV and for NOP fetches avoiding double ticking.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- `cargo run -p gb-romtest -- run --rom test-roms/blargg/cpu_instrs/cpu_instrs.gb --target dmg --format both --jobs 1 --case-timeout-seconds 60`
- `cargo run -p gb-romtest -- run --rom test-roms/blargg/instr_timing/instr_timing.gb --target dmg --format both --jobs 1 --case-timeout-seconds 60`
- `cargo run -p gb-romtest -- run --rom test-roms/blargg/halt_bug.gb --target dmg --format both --jobs 1 --case-timeout-seconds 60`
- Added `cpu_bus_cycle_helpers_advance_hardware_one_machine_cycle`.
- Added `game_boy_does_not_double_tick_clocked_nop_fetches`.

### Decisions

- Treated this as foundational timing work, not a compatibility patch.
- Added bus-side clocked-cycle accounting so partially converted CPU execution can coexist with existing public `GameBoy::step` behaviour.
- Left operand reads, data reads/writes, stack writes, and most internal instruction cycles on the existing cycle-total path for future scoped milestones.
- Added no dependencies.

### Notes

- Blargg `cpu_instrs`, `instr_timing`, and `halt_bug` all pass with this slice.
- Follow-up work should convert operand/data memory access and stack/internal cycles instruction family by instruction family.

## 0042: Document accuracy timing architecture

Date: 2026-06-11

Status: Complete

### Goal

Pause broad feature work and define the next architecture direction for timing-focused emulator accuracy.

### Changes

- Added `docs/timing-architecture.md` describing ordered CPU bus cycles, timer edge behaviour, stateful OAM DMA, PPU access restrictions, interrupt timing, and focused ROM-test gates.
- Updated `AGENTS.md` to mark the project as entering an accuracy-first phase.
- Updated architecture, testing strategy, and roadmap docs to point future work at the new timing model.

### Tests

- `cargo fmt --check`
- Documentation-only milestone; no Rust behaviour or tests changed.

### Decisions

- Kept the existing ownership boundaries: `GameBoy` owns `Cpu` and `Bus`, and CPU memory access still goes through `Bus`.
- Treated instruction-after-the-fact bus ticking as historical baseline, not the accuracy target.
- Chose focused ROM gates over full-suite pass/fail as the milestone acceptance style for timing work.

### Notes

- Next suggested milestone: introduce clocked CPU bus access helpers without converting every instruction at once.

## 0041: Implement DMG HALT bug

Date: 2026-06-11

Status: Complete

### Goal

Use the blargg `halt_bug.gb` failure to add the DMG HALT bug behaviour without changing the broader CPU/bus timing model.

### Changes

- Added CPU state for the one-instruction HALT bug fetch quirk.
- Made `HALT` avoid entering the halted state when `IME` is clear and an enabled interrupt is already requested.
- Repeated the next opcode fetch once by suppressing the following `PC` increment.

### Tests

- `cargo test -p gb-core halt_bug`
- `cargo test -p gb-core halt_pauses`
- `cargo run -p gb-romtest -- run --rom test-roms/blargg/halt_bug.gb --target dmg --format both --jobs 1 --case-timeout-seconds 20`
- `cargo run -p gb-romtest -- run --suite blargg --target dmg --format both --jobs 8 --case-timeout-seconds 20`
- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added `cpu::tests::halt_bug_repeats_next_opcode_fetch_when_ime_is_clear_and_interrupt_pending`.

### Decisions

- Kept the quirk local to CPU fetch state; no bus ownership or timing architecture changed.
- Added no dependencies.

### Notes

- `blargg/halt_bug.gb` now passes.
- Fresh blargg suite run improved from 14 to 15 passed; remaining non-audio failures are memory timing and OAM bug tests.
- The memory timing failures point toward future intra-instruction bus timing work.

## 0040: Add APU and desktop sound output

Date: 2026-06-11

Status: Complete

### Goal

Implement the roadmap APU/sound milestone so the emulator core can generate DMG audio samples and the desktop frontend can play them.

### Changes

- Added an `Apu` component with audio register routing, NR52 power control, frame sequencer timing, two square channels, wave channel, noise channel, stereo mixing, and drainable PCM samples.
- Routed `0xFF10..=0xFF3F` through `Bus`, ticked APU hardware with CPU cycles, and exposed audio samples through `Bus` and `GameBoy`.
- Added a `cpal`-backed desktop audio sink that queues core-generated stereo samples and drains them from the host audio callback.
- Added APU unit tests and bus routing coverage for generated audio sample draining.

### Tests

- `cargo fmt`
- `cargo test -p gb-core apu`
- `cargo test -p gb-desktop`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added tests in `crates/gb-core/src/apu.rs` and `crates/gb-core/tests/bus_routing.rs`.

### Decisions

- Kept audio synthesis in `gb-core` and host playback in `gb-desktop`.
- Added `cpal` to `gb-desktop` only because audio backend dependencies belong at the frontend boundary.
- Used signed 16-bit stereo PCM samples at 44.1 kHz as a simple frontend-facing core audio format.
- Implemented practical first-pass APU behaviour; obscure register edge cases and audio test-ROM accuracy remain follow-up compatibility work.

### Notes

- Pan Docs audio register/timing references informed the implementation.
- The desktop backend falls back to disabling audio if no host output device is available.

## 0039: Resolve remaining unsupported ROM registry gaps

Date: 2026-06-11

Status: Complete

### Goal

Use suite source/docs to classify the remaining unsupported DMG-profile ROMs more accurately.

### Changes

- Classified Mealybug `mbc3_rtc.gb` as a Fibonacci breakpoint-register test based on its base harness result flow.
- Allowed golden lookup to match ROM/PNG stems across separator differences such as `statcount-auto` and `statcount_auto`.
- Treated Mealybug `win_without_bg.gb` as a known skipped screenshot artifact when no local golden PNG is present.

### Tests

- `cargo fmt`
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- `cargo run -p gb-romtest -- run --rom test-roms/mealybug-tearoom-tests/mbc/mbc3_rtc.gb --target dmg --format both --jobs 1 --case-timeout-seconds 15`
- `cargo run -p gb-romtest -- run --suite scribbltests --target dmg --format both --jobs 4 --case-timeout-seconds 20`
- `cargo run -p gb-romtest -- run --suite mealybug-tearoom-tests --target dmg --format both --jobs 8 --case-timeout-seconds 20`
- Added runner unit tests for `mbc3_rtc` classification, flexible golden lookup, and known missing-golden skip classification.

### Decisions

- Did not invent a synthetic `win_without_bg` golden from source; exact visual comparison still needs a real reference PNG.
- Added no dependencies.

### Notes

- References used: local `mbc3_rtc.asm`, local `win_without_bg.asm`, and Scribbltests STATcount README.
- Targeted Mealybug and Scribbltests runs now report zero unsupported entries.

## 0038: Tighten test ROM harness classification

Date: 2026-06-11

Status: Complete

### Goal

Fix obvious `gb-romtest` harness/reporting issues found in the DMG profile report before treating remaining failures as emulator bugs.

### Changes

- Report breakpoint-register cases as `Timeout` when the suite breakpoint is not reached instead of evaluating stale registers.
- Added Wilbertpol Mooneye support for the legacy `0xED` undefined-opcode exit condition.
- Tightened DMG profile filtering for CGB-only, SGB-only, HDMA/GDMA, speed-switch, `ncm`, and loose root ROM cases.
- Expanded `rtc3test/rtc3test.gb` into basic, range, and sub-second scripted screenshot subtests.
- Preserved report target/profile values from the active runner options.

### Tests

- `cargo fmt`
- `cargo fmt --check`
- `cargo test -p gb-romtest`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- `cargo run -p gb-romtest -- run --suite rtc3test --target dmg --format both --jobs 3 --case-timeout-seconds 5`
- Added runner unit tests for timeout classification, model filtering, loose root ROM exclusion, Wilbertpol breakpoints, and `rtc3test` expansion.

### Decisions

- Kept dual-labelled AGE ROMs that include a DMG marker in the DMG profile.
- Represented `rtc3test` subtests as logical cases that share one ROM path but use distinct report paths and goldens.
- Added no dependencies.

### Notes

- Remaining report failures should now have less obvious harness noise before emulator subsystem triage.
- The short `rtc3test` validation produced three screenshot failures, which is expected until MBC3 RTC behaviour is implemented.

## 0037: Parallelize test ROM runner

Date: 2026-06-11

Status: Complete

### Goal

Speed up `gb-romtest` by running independent ROM cases through a worker queue and using the release build for test execution.

### Changes

- Added a central dispatcher with a shared work queue, worker threads, ordered result collection, and terminal progress output.
- Added `--jobs N` and `--case-timeout-seconds N` runner options.
- Added per-ROM wall-clock timeout handling in addition to existing emulated-time budgets.
- Made debug invocations build `gb-romtest` in release mode first, then re-run the requested test command through `target/release/gb-romtest`.

### Tests

- `cargo test -p gb-romtest`
- `cargo run -p gb-romtest -- run --profile smoke --target dmg --format both --jobs 4 --case-timeout-seconds 30`
- `cargo fmt`
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features`

### Decisions

- Kept report writing centralized after workers return results, so JSON and Markdown generation remain deterministic.
- Preserved discovery order in reports even though cases complete out of order.
- Used standard-library threads and channels instead of adding a dependency.

### Notes

- The smoke command now prints per-ROM progress as workers finish.
- The release-backed smoke run still reports 5 passed and 2 GBMicrotest timeouts.

## 0036: Refine GBMicrotest result handling

Date: 2026-06-11

Status: Complete

### Goal

Align `gb-romtest` GBMicrotest handling with the upstream result contract for `0xFF80..=0xFF82`.

### Changes

- Added early stopping when GBMicrotest writes `0x01` or `0xFF` to `0xFF82`.
- Expanded RAM-signature reports to include `0xFF80` actual result, `0xFF81` expected result, and `0xFF82` pass/fail status.

### Tests

- `cargo test -p gb-romtest ram_signature`
- `cargo run -p gb-romtest -- run --profile smoke --format both`
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features`

### Decisions

- Kept the two-frame default timeout for ordinary GBMicrotest ROMs, but now exits earlier when the documented sentinel is written.

### Notes

- Smoke report still shows the two selected GBMicrotest ROMs timing out, but now records all three documented HRAM bytes for debugging.

## 0035: Fix DMG golden image selection

Date: 2026-06-11

Status: Complete

### Goal

Fix a false negative where DMG runs could compare `dmg-acid2.gb` against the CGB-labelled golden image.

### Changes

- Updated golden PNG ranking to prefer target labels in the filename suffix after the ROM stem.
- Added a regression test proving `dmg-acid2-dmg.png` ranks ahead of `dmg-acid2-cgb.png` for DMG runs.

### Tests

- `cargo test -p gb-romtest golden_sort_prefers_dmg_label_for_dmg_target`
- `cargo run -p gb-romtest -- run --profile smoke --format both`
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features`

### Decisions

- Treated `dmg`/`cgb` as variant labels after the ROM stem instead of substring matching the full filename.

### Notes

- Smoke report now shows `dmg-acid2` passing; remaining smoke issues are the two GBMicrotest timeouts.

## 0034: Add agent-first test ROM runner

Date: 2026-06-10

Status: Complete

### Goal

Add a headless runner that agents can use to execute local external test ROM profiles and write structured reports.

### Changes

- Added `gb-romtest` workspace binary with `run` profiles, suite/ROM filters, DMG default target, and JSON/Markdown reports under `reports/test-roms/`.
- Added suite classification for non-Gambatte ROMs, including breakpoint register checks, serial text checks, GBMicrotest HRAM signatures, screenshot comparisons, scripted input metadata, and audio unsupported reporting.
- Added `/reports/` to ignored generated artifacts.

### Tests

- `cargo fmt`
- `cargo fmt --check`
- `cargo test`
- `cargo test -p gb-romtest`
- `cargo clippy --all-targets --all-features`
- `cargo run -p gb-romtest -- run --profile smoke --format both`
- Added runner unit tests for classification, result evaluators, screenshot diffing, and scripted joypad schedules.

### Decisions

- Kept all runner logic in `gb-romtest`; `gb-core` remains emulator-only and `gb-desktop` remains the visual/manual frontend.
- Added `serde` and `serde_json` to `gb-romtest` for machine-readable reports.
- Reused `image` with PNG support in `gb-romtest` for golden screenshot comparison.
- Skipped Gambatte explicitly because its C++ harness is out of scope for this milestone.

### Notes

- Smoke run wrote 7 results: 4 passed, 1 screenshot failure, and 2 GBMicrotest timeouts.
- Audio tests are inventoried but reported unsupported until deterministic APU output exists.
- Screenshot comparison is exact pixel equality for now; tolerance can be added later if needed.

## 0033: Improve manual harness ROM navigation

Date: 2026-06-10

Status: Complete

### Goal

Make the desktop harness easier to use with large external test ROM suites.

### Changes

- Increased the harness window to `1280x800`.
- Replaced the flat ROM list with a collapsible file tree showing folder ROM counts.
- Added left/right folder collapse and expand behaviour; enter toggles folders or runs ROMs.
- Fixed the ROM list/instructions overlap by constraining the tree list height.
- Added full-width lower panels for selected ROM and running ROM details.
- Kept the right status panel focused on live runtime state, serial output, errors, and golden preview.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added a desktop unit test for hiding children under collapsed tree entries.

### Decisions

- Kept the harness as dependency-light custom pixel UI instead of adding an immediate-mode GUI dependency.
- Stored the ROM browser as flattened tree entries with depth and expanded state for simple keyboard navigation.
- Added no dependencies.

### Notes

- The tree initially shows top-level folders collapsed; use right arrow or enter to open folders.

## 0032: Add manual GUI ROM testing harness

Date: 2026-06-10

Status: Complete

### Goal

Add an early developer GUI harness that can launch from the desktop executable,
pick local test ROMs, run them visually, and show useful comparison/debug info.

### Changes

- Added no-argument / `--harness` desktop mode that recursively lists `.gb` and `.gbc` files under `test-roms/`.
- Added keyboard selection and run controls: up/down, page up/down, home/end, enter, space pause, and `R` reset.
- Composed a harness window with ROM list, live scaled Game Boy framebuffer, CPU/PPU/interrupt/serial diagnostics, errors, and golden PNG preview when a matching image exists beside the ROM.
- Added read-only `GameBoy` debug accessors for CPU registers and bus byte inspection.
- Preserved direct ROM launch and existing headless serial/Mooneye/frame-dump tooling.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added desktop unit tests for `--harness` option parsing, ROM extension filtering, and debug path truncation.

### Decisions

- Kept the harness in `gb-desktop`; `gb-core` only exposes read-only debug inspection.
- Added `font8x8` to `gb-desktop` for simple pixel text rendering.
- Added `image` with PNG support to `gb-desktop` to load local golden screenshots.
- Kept the first harness keyboard-driven and dependency-light instead of introducing a full immediate-mode GUI stack.

### Notes

- Golden matching currently looks for PNGs in the ROM's directory whose stem starts with the ROM stem, preferring DMG-labelled images.
- This is intentionally early dev tooling: it is useful for manual inspection, not yet an automated visual assertion runner.

## 0031: Add headless compatibility gauntlet tooling

Date: 2026-06-10

Status: Complete

### Goal

Add token-efficient ROM test output for serial, Mooneye, and visual-only PPU
tests, then use it to run the next expected-pass compatibility gauntlet.

### Changes

- Added `--frames N --dump-frame hash|blocks:N|pixels|all` to print stable framebuffer hashes, block digests, and full greppable pixel maps.
- Added `--mooneye-steps N` to detect Mooneye's `0xED` exit opcode and report the Fibonacci pass/fail register signature.
- Mirrored unavailable cartridge ROM bank bits against the loaded ROM bank count so small MBC1 ROMs behave correctly.
- Added STAT coincidence/mode interrupt signalling, DMG window Y trigger state, internal window line counting, 10-sprite line selection, and DMG sprite priority sorting.

### Tests

- `cargo fmt`
- `cargo test -p gb-desktop`
- `cargo test -p gb-core mbc1_small_roms_mirror_unavailable_upper_bank_bits`
- `cargo test -p gb-core ppu::tests`
- `cargo run --release -p gb-desktop --quiet -- test-roms\blargg\cpu_instrs\cpu_instrs.gb --serial-steps 120000000`
- Blargg `cpu_instrs/individual/*.gb` all printed `Passed`.
- `cargo run --release -p gb-desktop --quiet -- test-roms\blargg\instr_timing\instr_timing.gb --serial-steps 120000000`
- Mooneye `acceptance/bits/reg_f.gb`, `acceptance/bits/mem_oam.gb`, and `emulator-only/mbc1_rom_4banks.gb` all printed `Mooneye: Passed`.
- Visual frame hashes at frame 60: `dmg-acid2` `b14891ff582424f6`, `firstwhite` `10f86562dc4dc24d`, `tellinglys` `1129354be22bf77e`, `window_y_trigger` `617480c61a3f32a6`, `window_y_trigger_wx_offscreen` `4ded2a7ac6a6a68d`, `strikethrough` `d1cbcf3f725fda4e`.
- `dmg-acid2` and both Turtle window tests were rendered to temporary PNGs under `target/` and visually matched their bundled expected images.
- `cargo run --release -p gb-desktop --quiet -- test-roms\blargg\mem_timing-2\mem_timing.gb --serial-steps 500000000` produced no serial result, so it remains an unresolved diagnostic candidate.
- `cargo run --release -p gb-desktop --quiet -- test-roms\blargg\mem_timing\mem_timing.gb --serial-steps 500000000` printed `Failed 3 tests`; this is deferred memory timing accuracy work, not part of the current expected-pass lane.

### Decisions

- Kept visual dump tooling in `gb-desktop`; `gb-core` still exposes only emulator state.
- Used an internal dependency-free FNV-1a framebuffer hash rather than adding a hashing crate.
- Treated `strikethrough` as diagnostic because it targets unusual OAM DMA behaviour beyond the current immediate-copy DMA model.
- Added no dependencies.

### Notes

- The visual text dump is now suitable for agent-readable golden output.
- Future work can promote frame hashes or pixel maps into automated optional ROM tests once expected outputs are checked in or generated locally.

## 0030: Add MBC2, MBC3, MBC5, and RTC cartridge support

Date: 2026-06-10

Status: Complete

### Goal

Complete roadmap Stage 22 and the cartridge stretch goals by adding broader MBC cartridge support, MBC3 RTC behaviour, and the MBC30 accessible-RAM quirk.

### Changes

- Decoded MBC2, MBC3, MBC5, MBC6, MBC7, MMM01, and MBC30 cartridge type names.
- Implemented MBC2 ROM banking and 512 x 4-bit internal RAM behaviour.
- Implemented MBC3 ROM/RAM banking and RTC register selection.
- Implemented MBC5 nine-bit ROM banking and RAM banking, including MBC5 rumble cartridge type families.
- Added MBC3 RTC registers backed by host time plus a cartridge-local offset so games can set RTC time through register writes.
- Modelled MBC30 as MBC5-like with only 64 KiB of accessible RAM, ignoring the high RAM bank select bit.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added cartridge tests for Stage 22 mapper decoding, MBC2 address-bit behaviour, MBC3 ROM/RAM/RTC registers, MBC5 nine-bit ROM banking, and MBC30 RAM mirroring.

### Decisions

- Used a host-time RTC offset rather than storing absolute host timestamps in `gb-core`.
- Kept RTC persistence metadata out of the save format for now; save RAM byte persistence remains the only desktop sidecar data.
- Treated MBC6, MBC7, and MMM01 as decoded cartridge identities but did not emulate their special hardware beyond clear type reporting.
- Added no dependencies.

### Notes

- RTC halt, latch, and register writes are modelled, but future compatibility work may refine persistence and edge-case carry timing.

## 0029: Complete MBC1 cartridge support

Date: 2026-06-10

Status: Complete

### Goal

Complete roadmap Stage 21 by supporting MBC1 ROM banking, external RAM banking, and battery-backed save RAM persistence.

### Changes

- Added MBC1 RAM and battery cartridge type decoding.
- Implemented MBC1 RAM enable, lower ROM bank select, upper bank bits, ROM/RAM banking mode, external RAM reads, and external RAM writes.
- Routed `0xA000..=0xBFFF` bus accesses to cartridge RAM.
- Exposed save RAM extraction/restoration through `Cartridge`, `Bus`, and `GameBoy`.
- Added `.sav` sidecar load/save support in `gb-desktop` for battery-backed cartridges.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added cartridge tests for MBC1 upper ROM bits, bank-zero remapping in RAM mode, RAM enable, RAM bank switching, and save RAM restore validation.

### Decisions

- Kept filesystem persistence in `gb-desktop`; `gb-core` only exposes save RAM bytes.
- Kept the existing `Cartridge` struct shape for this stage instead of a broad enum refactor, while modelling MBC1 control registers explicitly.
- Added no dependencies.

### Notes

- Stage 22 will add MBC3/MBC5 and extended cartridge types, including RTC handling.

## 0028: Enforce desktop frame pacing

Date: 2026-06-10

Status: Complete

### Goal

Add the extra roadmap milestone between Stages 20 and 21 by pacing desktop presentation to the DMG refresh rate instead of host redraw speed.

### Changes

- Added a `FramePacer` in `gb-desktop` that targets one frame every 70224 T-cycles at 4194304 Hz.
- Used `ControlFlow::WaitUntil` between redraws so a fast host display does not make emulation sprint.
- Scheduled the next frame after a successful `pixels.render()`.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added desktop unit tests for the 16.74 ms DMG frame interval and pacer wait behavior.

### Decisions

- Kept pacing in `gb-desktop`; `gb-core` cycle and PPU timing were not changed.
- Added no dependencies.

### Notes

- The target interval is approximately 16.742706 ms, matching about 59.7275 Hz.
- Later accuracy work can still refine CPU/PPU timing, but desktop refresh rate should no longer directly speed up gameplay.

## 0027: Add basic window layer

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 20.1 by rendering the DMG window layer using LCDC window enable, tile-map selection, and WX/WY placement.

### Changes

- Added window rendering to the PPU scanline path before sprites are composed.
- Used `WX - 7` and `WY` to position the window on screen.
- Added LCDC helpers for window enable and window tile-map selection.
- Made sprite priority compare against the composed background/window colour index.

### Tests

- `cargo fmt`
- `cargo test -p gb-core window`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added PPU tests for window placement and selected window tile map.

### Decisions

- Kept this to basic DMG window rendering; timing quirks remain deferred to roadmap Stage 20.2.
- Reused the existing background tile data selection and palette mapping.
- Added no dependencies.

### Notes

- Window rendering now exists alongside background, sprites, and keyboard input, which should make more simple DMG games visually interactive.

## 0026: Add DMA and sprite rendering

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 19 by adding OAM DMA, sprite attribute parsing, and basic DMG sprite rendering.

### Changes

- Routed `FF46` DMA writes through `Bus` and copied 160 bytes from `N << 8` into OAM.
- Added `OamEntry` parsing for sprite coordinates, tile index, priority, flips, and object palette selection.
- Rendered 8x8 and 8x16 sprites over the background with transparent colour 0.
- Applied X/Y flip, OBP0/OBP1 palette selection, OAM order, and basic background priority behaviour.

### Tests

- `cargo fmt`
- `cargo test -p gb-core sprite`
- `cargo test -p gb-core dma`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added a bus DMA test and PPU unit tests for sprite parsing, basic rendering, flips/palettes, priority, and 8x16 mode.

### Decisions

- Kept DMA as an immediate bus copy for this milestone rather than modelling CPU stall timing.
- Kept sprite rendering inside PPU scanline rendering and reused existing DMG shade palette mapping.
- Added no dependencies.

### Notes

- Detailed sprite timing and per-scanline sprite limits remain future accuracy work.

## 0025: Add joypad input

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 18 by modelling DMG joypad state, routing the `FF00` register, mapping desktop keyboard input, and requesting Joypad interrupts on new button presses.

### Changes

- Added `gb_core::joypad` with `Button` and active-low action/direction group register behaviour.
- Routed `FF00` through `Bus` and exposed `GameBoy::set_button`.
- Mapped desktop keyboard input to joypad buttons: arrows, `Z`, `X`, `Enter`, and right shift.
- Requested the Joypad interrupt when a button transitions from released to pressed.

### Tests

- `cargo fmt`
- `cargo test -p gb-core joypad`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added joypad unit tests and a bus routing test for `FF00` and interrupt requests.

### Decisions

- Kept all windowing key types in `gb-desktop`; `gb-core` only knows the `Button` enum.
- Stored joypad button state in a fixed `[bool; 8]` array.
- Added no dependencies.

### Notes

- Keyboard input now reaches the core, but gameplay also depends on the remaining PPU sprite/window work.

## 0024: Add basic desktop display

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 17 by opening a scaled desktop window, displaying a framebuffer, and driving presentation from the emulator frame-ready loop.

### Changes

- Replaced the placeholder `GameBoy` with a top-level owner for `Cpu` and `Bus`.
- Added `GameBoy::from_rom`, `step`, `run_until_frame`, framebuffer access, and serial output helpers.
- Added a `pixels` + `winit` desktop window that displays the 160x144 framebuffer at 4x scale.
- Preserved the existing `--serial-steps` runner and added `--demo` for a temporary animated checkerboard display.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added core tests for `GameBoy` framebuffer exposure and step/tick integration.

### Decisions

- Added `pixels` and `winit` to `gb-desktop` only because Stage 17 explicitly needs a desktop display backend.
- Kept `gb-core` free of frontend dependencies and exposed only emulator-facing framebuffer/frame-loop APIs.
- Used the existing PPU frame-ready flag to pace emulator presentation.

### Notes

- ROM display exits if execution reaches an unimplemented opcode before the next frame.
- The `--demo` mode gives a guaranteed visual smoke test while emulator compatibility is still early.

## 0023: Add PPU timing

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 16 by advancing the PPU by T-cycles, tracking scanlines, requesting VBlank, exposing STAT mode bits, and rendering visible lines as timing reaches them.

### Changes

- Added `Ppu::tick` with 456 T-cycles per scanline and 154 lines per frame.
- Incremented and wrapped `LY`, requested VBlank when entering line 144, and exposed basic OAM/Transfer/HBlank/VBlank mode bits through `STAT`.
- Rendered visible scanlines during PPU ticking and added frame-ready helpers through the bus.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added PPU unit tests for `LY` progression, wraparound, mode transitions, and bus-level VBlank interrupt requests.

### Decisions

- Used simple scanline timing constants: 80 dots OAM, 172 dots transfer, remaining dots HBlank.
- Requested only the VBlank interrupt for now; STAT interrupt source enables are left for a later accuracy milestone.
- Added no dependencies.

### Notes

- Line rendering is background-only; sprites and window rendering remain later roadmap stages.

## 0022: Decode tiles and render background

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 15 by decoding Game Boy tile data and rendering a scrollable background into a simple DMG greyscale framebuffer.

### Changes

- Added tile-row and full-tile decoding from 2-bit planar tile bytes.
- Added background rendering using LCDC tile data and tile map selection.
- Applied the BGP register to map 2-bit colour indices to four `u32` greyscale shades.
- Added SCX/SCY scrolling support.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added PPU unit tests for tile decoding, tile-map rendering, palette mapping, and scroll offsets.

### Decisions

- Kept the framebuffer as `[u32; 160 * 144]`, matching the architecture guidance and keeping the frontend boundary simple.
- Used fixed DMG shade values in ARGB-like `u32` form for now.
- Added no dependencies.

### Notes

- Rendering currently covers the background layer only; the window and sprites remain future milestones.

## 0021: Add PPU skeleton

Date: 2026-06-10

Status: Complete

### Goal

Implement roadmap Stage 14 by adding a PPU component with VRAM, OAM, and LCD register routing through the bus.

### Changes

- Added `gb_core::ppu::Ppu` with 8 KiB VRAM, 160 bytes of OAM, LCD registers, and a framebuffer.
- Routed `0x8000..=0x9FFF` VRAM, `0xFE00..=0xFE9F` OAM, unusable OAM reads, and `0xFF40..=0xFF4B` LCD registers through `Bus`.
- Made `LY` read-only to CPU writes for now.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- Added PPU unit tests and bus integration tests for VRAM, OAM, unusable OAM, and LCD register routing.

### Decisions

- Kept PPU-owned fixed hardware storage as arrays.
- Stored LCDC in a small wrapper and used a `PpuMode` enum internally.
- Added no dependencies.

### Notes

- LCD register behavior is intentionally basic; detailed STAT interrupt behavior and access restrictions are deferred.

## 0020: Start external CPU ROM testing

Date: 2026-06-10

Status: Complete

### Goal

Run Blargg's external `cpu_instrs` ROM through the headless serial runner and fix the CPU/cartridge gaps needed for it to pass.

### Changes

- Decoded cartridge type `0x01` as `MBC1` instead of a generic unsupported type.
- Added minimal MBC1 ROM bank switching for the lower 5-bit ROM bank register.
- Added missing CPU opcodes found by the test ROMs: HL auto-increment/decrement loads, stack `PUSH`/`POP`, ALU `(HL)` operands, `INC/DEC (HL)`, `LD (a16),SP`, `RETI`, and `LD A,(C)`/`LD (C),A`.
- Ran Blargg's individual `04-op r,imm.gb` and combined `cpu_instrs.gb` ROMs through the existing serial step runner.

### Tests

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- `cargo run -p gb-desktop --quiet -- "test-roms\blargg\cpu_instrs\individual\04-op r,imm.gb" --serial-steps 10000000`
- `cargo run -p gb-desktop --quiet -- test-roms\blargg\cpu_instrs\cpu_instrs.gb --serial-steps 120000000`
- Added unit tests for each added CPU instruction group.
- Added cartridge and bus tests for MBC1 ROM bank selection.

### Decisions

- MBC1 support remains intentionally minimal: enough ROM banking for external CPU tests, without RAM banking or mode support yet.
- Kept using the existing bounded serial step runner for the first external ROM attempt.
- Added no dependencies.

### Notes

- Blargg `cpu_instrs.gb` printed `Passed all tests`.
- A future runner improvement could stop automatically when serial output contains `Passed` or `Failed` instead of relying on a large step count.

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
