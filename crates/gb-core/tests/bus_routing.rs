mod common;

use common::minimal_rom_with_entry_point;
use gb_core::{bus::Bus, cartridge::Cartridge, cpu::TCycles, interrupt::Interrupt, joypad::Button};

const TITLE_START: usize = 0x0134;
const CARTRIDGE_TYPE_ADDR: usize = 0x0147;
const ROM_SIZE_ADDR: usize = 0x0148;
const RAM_SIZE_ADDR: usize = 0x0149;
const HEADER_CHECKSUM_START: usize = 0x0134;
const HEADER_CHECKSUM_END_INCLUSIVE: usize = 0x014C;
const HEADER_CHECKSUM_ADDR: usize = 0x014D;
const ROM_BANK_SIZE: usize = 0x4000;

fn test_bus_with_rom_byte(address: usize, value: u8) -> Bus {
    let mut rom = minimal_rom_with_entry_point(&[]);
    rom[TITLE_START..TITLE_START + 7].copy_from_slice(b"BUSTEST");
    rom[CARTRIDGE_TYPE_ADDR] = 0x00;
    rom[ROM_SIZE_ADDR] = 0x00;
    rom[RAM_SIZE_ADDR] = 0x00;
    rom[address] = value;
    rom[HEADER_CHECKSUM_ADDR] = calculate_header_checksum(&rom);

    let cartridge = Cartridge::from_bytes(rom).expect("test ROM should parse");
    Bus::new(cartridge)
}

fn test_bus_with_mbc1_banks() -> Bus {
    let mut rom = minimal_rom_with_entry_point(&[]);
    rom.resize(4 * ROM_BANK_SIZE, 0);
    rom[TITLE_START..TITLE_START + 7].copy_from_slice(b"BANKBUS");
    rom[CARTRIDGE_TYPE_ADDR] = 0x01;
    rom[ROM_SIZE_ADDR] = 0x01;
    rom[RAM_SIZE_ADDR] = 0x00;
    rom[ROM_BANK_SIZE] = 0x11;
    rom[2 * ROM_BANK_SIZE] = 0x22;
    rom[3 * ROM_BANK_SIZE] = 0x33;
    rom[HEADER_CHECKSUM_ADDR] = calculate_header_checksum(&rom);

    let cartridge = Cartridge::from_bytes(rom).expect("MBC1 test ROM should parse");
    Bus::new(cartridge)
}

fn calculate_header_checksum(rom: &[u8]) -> u8 {
    rom[HEADER_CHECKSUM_START..=HEADER_CHECKSUM_END_INCLUSIVE]
        .iter()
        .fold(0_u8, |checksum, byte| {
            checksum.wrapping_sub(*byte).wrapping_sub(1)
        })
}

#[test]
fn read8_routes_cartridge_rom_reads() {
    let bus = test_bus_with_rom_byte(0x0100, 0x42);

    assert_eq!(
        bus.read8(0x0100),
        0x42,
        "0x0100 should read from cartridge ROM through the bus"
    );
}

#[test]
fn read8_returns_ff_for_unsupported_addresses() {
    let bus = test_bus_with_rom_byte(0x0100, 0x42);

    assert_eq!(
        bus.read8(0xA000),
        0xFF,
        "unsupported memory regions should read as 0xFF for now"
    );
}

#[test]
fn write8_roundtrips_wram() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xC000, 0x12);
    bus.write8(0xDFFF, 0x34);

    assert_eq!(bus.read8(0xC000), 0x12, "WRAM start should roundtrip");
    assert_eq!(bus.read8(0xDFFF), 0x34, "WRAM end should roundtrip");
}

#[test]
fn write8_roundtrips_hram() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xFF80, 0x56);
    bus.write8(0xFFFE, 0x78);

    assert_eq!(bus.read8(0xFF80), 0x56, "HRAM start should roundtrip");
    assert_eq!(bus.read8(0xFFFE), 0x78, "HRAM end should roundtrip");
}

#[test]
fn write8_roundtrips_interrupt_enable() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xFFFF, 0xFF);

    assert_eq!(
        bus.read8(0xFFFF),
        0x1F,
        "0xFFFF should store only the five interrupt enable bits"
    );
}

#[test]
fn write8_roundtrips_interrupt_flags_with_if_upper_bits_set_on_read() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xFF0F, 0xFF);

    assert_eq!(
        bus.interrupt_flags(),
        0x1F,
        "IF storage should keep only the five interrupt request bits"
    );
    assert_eq!(
        bus.read8(0xFF0F),
        0xFF,
        "IF reads should report unused upper bits as set"
    );
}

#[test]
fn typed_interrupt_helpers_request_clear_and_prioritize_pending_interrupts() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xFFFF, Interrupt::Timer.mask() | Interrupt::Joypad.mask());
    bus.request_interrupt(Interrupt::Joypad);
    bus.request_interrupt(Interrupt::Timer);

    assert_eq!(
        bus.pending_interrupt(),
        Some(Interrupt::Timer),
        "Timer should have priority over Joypad"
    );

    bus.clear_interrupt(Interrupt::Timer);

    assert_eq!(
        bus.pending_interrupt(),
        Some(Interrupt::Joypad),
        "Joypad should remain pending after Timer is cleared"
    );
}

#[test]
fn timer_registers_are_routed_and_tick_requests_timer_interrupt() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xFF05, 0xFF);
    bus.write8(0xFF06, 0x77);
    bus.write8(0xFF07, 0x05);
    bus.tick(TCycles(16));

    assert_eq!(
        bus.read8(0xFF05),
        0x77,
        "TIMA overflow should reload TMA through bus ticking"
    );
    assert_eq!(
        bus.interrupt_flags(),
        Interrupt::Timer.mask(),
        "TIMA overflow should request Timer interrupt through IF"
    );
}

#[test]
fn cpu_bus_cycle_helpers_advance_hardware_one_machine_cycle() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    let value = bus.cpu_fetch8(0x0100);
    for _ in 0..63 {
        bus.cpu_idle_mcycle();
    }

    assert_eq!(value, 0x42, "CPU opcode fetch should return the bus byte");
    assert_eq!(
        bus.read8(0xFF04),
        0x01,
        "64 CPU machine cycles should advance DIV by 256 T-cycles"
    );
}

#[test]
fn serial_registers_are_routed_and_output_can_be_drained() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xFF01, b'O');
    bus.write8(0xFF02, 0x81);
    bus.write8(0xFF01, b'K');
    bus.write8(0xFF02, 0x81);

    assert_eq!(bus.read8(0xFF01), b'K', "SB should keep the latest byte");
    assert_eq!(
        bus.serial_output(),
        b"OK",
        "SC transfer starts should expose serial output"
    );
    assert_eq!(
        bus.take_serial_output(),
        b"OK",
        "take should return serial output"
    );
    assert!(
        bus.serial_output().is_empty(),
        "take_serial_output should drain the buffer"
    );
}

#[test]
fn audio_registers_are_routed_and_samples_can_be_drained() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xFF11, 0x80);
    bus.write8(0xFF12, 0xF0);
    bus.write8(0xFF14, 0x80);
    bus.tick(TCycles(4_194_304 / 60));

    assert_eq!(
        bus.read8(0xFF12),
        0xF0,
        "APU envelope register should route through the bus"
    );
    assert!(
        !bus.audio_samples().is_empty(),
        "Bus ticking should advance APU sample generation"
    );
    assert!(
        !bus.take_audio_samples().is_empty(),
        "Bus should expose generated audio to frontends"
    );
    assert!(
        bus.audio_samples().is_empty(),
        "taking audio samples should drain the bus-owned APU buffer"
    );
}

#[test]
fn joypad_register_routes_selected_active_low_buttons_and_interrupts() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xFF00, 0x10);
    bus.set_button(Button::A, true);

    assert_eq!(
        bus.read8(0xFF00) & 0x0F,
        0b1110,
        "FF00 should expose selected action buttons as active-low bits"
    );
    assert_eq!(
        bus.interrupt_flags() & Interrupt::Joypad.mask(),
        Interrupt::Joypad.mask(),
        "new button presses should request the Joypad interrupt"
    );
}

#[test]
fn vram_reads_and_writes_route_through_ppu() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0x8000, 0x12);
    bus.write8(0x9FFF, 0x34);

    assert_eq!(
        bus.read8(0x8000),
        0x12,
        "VRAM start should roundtrip through PPU"
    );
    assert_eq!(
        bus.read8(0x9FFF),
        0x34,
        "VRAM end should roundtrip through PPU"
    );
}

#[test]
fn oam_reads_and_writes_route_through_ppu_and_unusable_oam_reads_ff() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xFE00, 0x56);
    bus.write8(0xFE9F, 0x78);
    bus.write8(0xFEA0, 0x99);

    assert_eq!(
        bus.read8(0xFE00),
        0x56,
        "OAM start should roundtrip through PPU"
    );
    assert_eq!(
        bus.read8(0xFE9F),
        0x78,
        "OAM end should roundtrip through PPU"
    );
    assert_eq!(
        bus.read8(0xFEA0),
        0xFF,
        "unusable OAM range should read as 0xFF"
    );
    assert_eq!(
        bus.read8(0xFEFF),
        0xFF,
        "unusable OAM range end should read as 0xFF"
    );
}

#[test]
fn writing_dma_register_copies_one_oam_page() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    for offset in 0..0x00A0 {
        let byte = u8::try_from(offset).expect("OAM DMA test offsets fit in u8");
        bus.write8(0xC000 + offset, byte);
    }

    bus.write8(0xFF46, 0xC0);

    assert_eq!(bus.read8(0xFE00), 0x00, "DMA should copy OAM byte 0");
    assert_eq!(bus.read8(0xFE01), 0x01, "DMA should copy OAM byte 1");
    assert_eq!(bus.read8(0xFE9F), 0x9F, "DMA should copy OAM byte 159");
}

#[test]
fn lcd_registers_route_through_ppu() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0xFF40, 0x80);
    bus.write8(0xFF42, 0x11);
    bus.write8(0xFF43, 0x22);
    bus.write8(0xFF44, 0x99);
    bus.write8(0xFF45, 0x33);
    bus.write8(0xFF47, 0xE4);
    bus.write8(0xFF48, 0xD2);
    bus.write8(0xFF49, 0xC1);
    bus.write8(0xFF4A, 0x44);
    bus.write8(0xFF4B, 0x55);

    assert_eq!(bus.read8(0xFF40), 0x80, "LCDC should route through PPU");
    assert_eq!(bus.read8(0xFF42), 0x11, "SCY should route through PPU");
    assert_eq!(bus.read8(0xFF43), 0x22, "SCX should route through PPU");
    assert_eq!(bus.read8(0xFF44), 0x00, "LY should ignore CPU writes");
    assert_eq!(bus.read8(0xFF45), 0x33, "LYC should route through PPU");
    assert_eq!(bus.read8(0xFF47), 0xE4, "BGP should route through PPU");
    assert_eq!(bus.read8(0xFF48), 0xD2, "OBP0 should route through PPU");
    assert_eq!(bus.read8(0xFF49), 0xC1, "OBP1 should route through PPU");
    assert_eq!(bus.read8(0xFF4A), 0x44, "WY should route through PPU");
    assert_eq!(bus.read8(0xFF4B), 0x55, "WX should route through PPU");
}

#[test]
fn ppu_tick_advances_ly_and_requests_vblank_interrupt() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.tick(TCycles(456));
    assert_eq!(bus.read8(0xFF44), 1, "PPU tick should increment LY");

    bus.tick(TCycles(456 * 143));
    assert_eq!(bus.read8(0xFF44), 144, "line 144 starts VBlank");
    assert_eq!(
        bus.interrupt_flags(),
        Interrupt::VBlank.mask(),
        "entering line 144 should request VBlank"
    );
}

#[test]
fn write8_ignores_rom_for_rom_only_cartridge() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write8(0x0100, 0x99);

    assert_eq!(
        bus.read8(0x0100),
        0x42,
        "writes to ROM should not alter ROM-only cartridge data"
    );
}

#[test]
fn write8_routes_mbc1_rom_bank_select() {
    let mut bus = test_bus_with_mbc1_banks();

    assert_eq!(bus.read8(0x4000), 0x11, "MBC1 should default to ROM bank 1");

    bus.write8(0x2000, 0x02);

    assert_eq!(
        bus.read8(0x4000),
        0x22,
        "MBC1 bank-select writes should affect switchable ROM reads"
    );

    bus.write8(0x2000, 0x00);

    assert_eq!(
        bus.read8(0x4000),
        0x11,
        "MBC1 bank 0 selection should map back to bank 1"
    );
}

#[test]
fn read16_reads_little_endian_values() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);
    bus.write8(0xC000, 0x34);
    bus.write8(0xC001, 0x12);

    assert_eq!(
        bus.read16(0xC000),
        0x1234,
        "read16 should combine low byte first, then high byte"
    );
}

#[test]
fn write16_writes_little_endian_values() {
    let mut bus = test_bus_with_rom_byte(0x0100, 0x42);

    bus.write16(0xC000, 0x1234);

    assert_eq!(bus.read8(0xC000), 0x34, "low byte should be written first");
    assert_eq!(
        bus.read8(0xC001),
        0x12,
        "high byte should be written second"
    );
}

#[test]
fn read16_and_write16_wrap_address_for_second_byte() {
    let mut bus = test_bus_with_rom_byte(0x0000, 0x12);

    bus.write16(0xFFFF, 0xABCD);

    assert_eq!(
        bus.read8(0xFFFF),
        0x0D,
        "first byte at 0xFFFF should write masked interrupt enable bits"
    );
    assert_eq!(
        bus.read8(0x0000),
        0x12,
        "second byte wraps to ROM and should be ignored for ROM-only cartridges"
    );
    assert_eq!(
        bus.read16(0xFFFF),
        0x120D,
        "read16 should use wrapping address arithmetic for the high byte"
    );
}
