# AGENTS.md

This project is an agent-assisted DMG Game Boy emulator written in Rust.

The early project goal was learning-first construction: build the emulator in a way that helped the human maintainer understand emulator architecture, Rust ownership, hardware modelling, testing, and key technical decisions.

The project has now moved into an accuracy-first phase. The core emulator exists; future work should strengthen the timing and hardware model instead of adding broad surface area or cheap compatibility patches.

Agents should act as careful pair programmers, not autonomous code volcanoes.

## Project goal

Build a non-colour DMG Game Boy emulator in Rust.

Initial target:

* DMG only
* No Game Boy Color
* No Super Game Boy
* No link cable
* No boot ROM dependency at first
* Start with no audio, then add APU later
* Support ROM-only first
* Add MBC1, then MBC3/MBC5
* Prioritise accuracy, correctness, tests, and understandability

The emulator should eventually support:

* CPU instruction execution
* Bus and memory map
* Cartridge loading and banking
* Timers
* Interrupts
* Serial test output
* PPU background, window, and sprites
* Joypad input
* Save RAM
* APU audio
* Headless test running
* Desktop frontend
* Debug tools

## Required reading

Before making architectural changes, read:

* `docs/architecture.md`
* `docs/timing-architecture.md`
* `docs/styleguide.md`
* `docs/testing-strategy.md`

Architecture decisions should follow those documents unless the task explicitly asks to revise them.

## Methodology

Work in small milestones.

Each milestone should be:

* Small enough to complete in one focused session
* Easy to review
* Covered by tests where practical
* Narrow in scope
* Recorded in the milestone log

Prefer incremental progress over large rewrites.

Do not implement multiple unrelated subsystems in one change.

In the accuracy phase, prefer hardware-model improvements over behaviour-targeted hacks. If a test ROM failure points to missing timing architecture, stop and model the timing instead of special-casing the symptom.

## Agent behaviour rules

Agents must:

1. Keep changes scoped to the requested milestone.
2. Avoid opportunistic refactors.
3. Ask before changing architecture rules.
4. Add or update tests for behaviour changes.
5. Avoid `unsafe` unless explicitly approved.
6. Avoid `Rc<RefCell<T>>` in `gb-core` unless explicitly justified.
7. Avoid frontend dependencies in `gb-core`.
8. Avoid macro-generated opcode tables unless requested.
9. Avoid broad dependency additions.
10. Preserve clear ownership boundaries between CPU, bus, and hardware components.
11. Keep CPU memory access routed through `Bus`.
12. Advance hardware at the bus-cycle boundary for timing work rather than only after whole CPU instructions.
13. Update record keeping after each milestone.

## Coding standards

Follow `docs/styleguide.md`.

Important defaults:

* Fixed hardware memory uses arrays.
* Cartridge ROM/RAM uses `Vec<u8>`.
* CPU steps with temporary mutable access to `Bus`.
* Components do not store references to each other.
* Runtime hardware reads generally return `u8`.
* Setup/parsing operations use `Result`.
* Cycle units should be explicit.
* CPU-visible memory access should move toward clocked bus-cycle helpers.
* Hardware modes should use enums internally.
* Bitflag-heavy registers should have wrappers or focused helper methods.

## Architecture standards

Follow `docs/architecture.md`.

Important defaults:

* `gb-core` contains emulator logic only.
* `gb-desktop` contains windowing, input, audio backend, CLI, save files, and debug UI.
* `GameBoy` owns `Cpu` and `Bus`.
* `Bus` owns cartridge, PPU, timer, joypad, serial, APU, WRAM, HRAM, and interrupt registers.
* CPU does not directly access PPU, cartridge, timer, joypad, serial, or APU.
* All CPU reads and writes go through the bus.
* Timing-sensitive CPU reads, writes, fetches, and idle cycles should advance bus-owned hardware in order.

## Record keeping

Each milestone should be recorded in `docs/milestones.md`.

If the file does not exist, create it.

Use this format:

```md
# Milestones

## 0001: Short milestone title

Date: YYYY-MM-DD

Status: Complete | Partial | Blocked

### Goal

One or two sentences describing the intended milestone.

### Changes

- Bullet list of concrete changes made.

### Tests

- Commands run.
- Test files added or changed.
- Any known test gaps.

### Decisions

- Any architecture, style, dependency, or modelling decisions made.

### Notes

- Bugs found.
- Follow-up work.
- Useful references.
```

Keep milestone entries short but useful.

Do not write a novel. The log is a trail of breadcrumbs, not a fantasy trilogy.

## When a milestone is partial

If work is incomplete, still update `docs/milestones.md`.

Use:

```md
Status: Partial
```

Include:

* What works
* What does not work yet
* What should happen next
* Whether tests pass

## When blocked

If blocked, record:

```md
Status: Blocked
```

Include:

* The blocking issue
* The file or subsystem involved
* What was attempted
* The smallest suggested next step

Do not hide uncertainty.

## Testing expectations

Follow `docs/testing-strategy.md` when adding tests or test tooling.

Before marking a milestone complete, run:

```bash
cargo fmt
cargo test
cargo clippy --all-targets --all-features
```

If a command cannot be run, say so in the milestone record.

If a command fails, record the failure and do not mark the milestone complete unless the human maintainer explicitly accepts the partial state.

## Commit style

Keep commits focused.

Suggested commit message style:

```text
milestone 0001: add workspace skeleton
milestone 0002: parse cartridge title
milestone 0003: add CPU register pairs
```

Avoid vague commits like:

```text
stuff
updates
fixes
big emulator work
```

Tiny precise commits beat majestic mystery slabs.

## Dependency policy

Do not add dependencies without a reason.

If adding a dependency, record in the milestone log:

* Dependency name
* Crate it was added to
* Why it is needed
* Why it belongs in that crate

No desktop, graphics, windowing, or audio backend dependency may be added to `gb-core`.

## Useful milestone completion checklist

Before saying a milestone is complete:

* The change is scoped to the milestone.
* The code is formatted.
* Tests were added or updated.
* Existing tests pass, or failures are recorded.
* Clippy passes, or warnings are recorded.
* `docs/milestones.md` is updated.
* No architecture rule was changed silently.
* No unnecessary dependency was added.
* No `unsafe` was added.
* No `Rc<RefCell<T>>` was added to `gb-core`.

## Human-in-the-loop expectation

The human maintainer should remain involved in:

* Architecture choices
* Cycle model decisions
* Cartridge mapper design
* PPU timing strategy
* Debug tooling direction
* Accuracy target decisions
* Dependency choices
* Any use of unsafe code
* Any major refactor

Agents may propose these changes, but should not silently apply them.

## Project tone

This project should stay understandable.

Prefer code that can be read, tested, and explained.

The emulator is allowed to be incomplete. It is not allowed to become inscrutable.
