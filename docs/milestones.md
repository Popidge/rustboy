# Milestones

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
