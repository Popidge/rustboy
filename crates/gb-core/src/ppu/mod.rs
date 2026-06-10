//! Pixel Processing Unit hardware for the DMG Game Boy.
//!
//! This module owns video memory, sprite attribute memory, LCD registers, and
//! the framebuffer. Rendering starts with background-only output and grows with
//! later milestones.

use crate::{
    cpu::TCycles,
    interrupt::{Interrupt, InterruptFlags},
};

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;
pub const FRAMEBUFFER_PIXELS: usize = SCREEN_WIDTH * SCREEN_HEIGHT;

const VRAM_SIZE: usize = 0x2000;
const OAM_SIZE: usize = 0x00A0;

const LCDC_ADDR: u16 = 0xFF40;
const STAT_ADDR: u16 = 0xFF41;
const SCY_ADDR: u16 = 0xFF42;
const SCX_ADDR: u16 = 0xFF43;
const LY_ADDR: u16 = 0xFF44;
const LYC_ADDR: u16 = 0xFF45;
const BGP_ADDR: u16 = 0xFF47;
const OBP0_ADDR: u16 = 0xFF48;
const OBP1_ADDR: u16 = 0xFF49;
const WY_ADDR: u16 = 0xFF4A;
const WX_ADDR: u16 = 0xFF4B;

const DOTS_PER_LINE: u32 = 456;
const VISIBLE_LINES: u8 = 144;
const LINES_PER_FRAME: u8 = 154;
const OAM_DOTS: u32 = 80;
const TRANSFER_DOTS: u32 = 172;

const DMG_SHADES: [u32; 4] = [0xFFFF_FFFF, 0xFFAA_AAAA, 0xFF55_5555, 0xFF00_0000];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ppu {
    vram: [u8; VRAM_SIZE],
    oam: [u8; OAM_SIZE],
    lcdc: Lcdc,
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,
    mode: PpuMode,
    line_dots: u32,
    framebuffer: [u32; FRAMEBUFFER_PIXELS],
    frame_ready: bool,
}

impl Ppu {
    #[must_use]
    #[allow(
        clippy::large_stack_arrays,
        reason = "The architecture keeps the fixed-size framebuffer as an array owned by PPU."
    )]
    pub fn new() -> Self {
        Self {
            vram: [0; VRAM_SIZE],
            oam: [0; OAM_SIZE],
            lcdc: Lcdc::new(0x91),
            stat: 0x80,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            mode: PpuMode::OamSearch,
            line_dots: 0,
            framebuffer: [DMG_SHADES[0]; FRAMEBUFFER_PIXELS],
            frame_ready: false,
        }
    }

    #[must_use]
    pub fn read_vram(&self, offset: u16) -> u8 {
        self.vram.get(usize::from(offset)).copied().unwrap_or(0xFF)
    }

    pub fn write_vram(&mut self, offset: u16, value: u8) {
        if let Some(byte) = self.vram.get_mut(usize::from(offset)) {
            *byte = value;
        }
    }

    #[must_use]
    pub fn read_oam(&self, offset: u16) -> u8 {
        self.oam.get(usize::from(offset)).copied().unwrap_or(0xFF)
    }

    pub fn write_oam(&mut self, offset: u16, value: u8) {
        if let Some(byte) = self.oam.get_mut(usize::from(offset)) {
            *byte = value;
        }
    }

    #[must_use]
    pub fn read_register(&self, address: u16) -> u8 {
        match address {
            LCDC_ADDR => self.lcdc.raw(),
            STAT_ADDR => (self.stat & 0xF8) | self.mode.stat_bits(),
            SCY_ADDR => self.scy,
            SCX_ADDR => self.scx,
            LY_ADDR => self.ly,
            LYC_ADDR => self.lyc,
            BGP_ADDR => self.bgp,
            OBP0_ADDR => self.obp0,
            OBP1_ADDR => self.obp1,
            WY_ADDR => self.wy,
            WX_ADDR => self.wx,
            _ => 0xFF,
        }
    }

    pub fn write_register(&mut self, address: u16, value: u8) {
        if address == LY_ADDR {
            return;
        }

        match address {
            LCDC_ADDR => self.lcdc.set_raw(value),
            STAT_ADDR => self.stat = value & 0xF8,
            SCY_ADDR => self.scy = value,
            SCX_ADDR => self.scx = value,
            LYC_ADDR => self.lyc = value,
            BGP_ADDR => self.bgp = value,
            OBP0_ADDR => self.obp0 = value,
            OBP1_ADDR => self.obp1 = value,
            WY_ADDR => self.wy = value,
            WX_ADDR => self.wx = value,
            _ => {}
        }
    }

    pub fn tick(&mut self, cycles: TCycles, interrupts: &mut InterruptFlags) {
        for _ in 0..cycles.0 {
            self.line_dots += 1;
            self.update_mode();

            if self.line_dots >= DOTS_PER_LINE {
                self.finish_scanline(interrupts);
            }
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32; FRAMEBUFFER_PIXELS] {
        &self.framebuffer
    }

    #[must_use]
    pub fn frame_ready(&self) -> bool {
        self.frame_ready
    }

    pub fn take_frame_ready(&mut self) -> bool {
        let was_ready = self.frame_ready;
        self.frame_ready = false;
        was_ready
    }

    pub fn render_background(&mut self) {
        for line in 0..VISIBLE_LINES {
            self.render_scanline(line);
        }
    }

    fn finish_scanline(&mut self, interrupts: &mut InterruptFlags) {
        if self.ly < VISIBLE_LINES {
            self.render_scanline(self.ly);
        }

        self.line_dots -= DOTS_PER_LINE;
        self.ly = self.ly.wrapping_add(1);

        if self.ly == VISIBLE_LINES {
            self.mode = PpuMode::VBlank;
            self.frame_ready = true;
            interrupts.request(Interrupt::VBlank);
        } else if self.ly >= LINES_PER_FRAME {
            self.ly = 0;
            self.mode = PpuMode::OamSearch;
            self.frame_ready = false;
        } else {
            self.update_mode();
        }
    }

    fn update_mode(&mut self) {
        self.mode = if self.ly >= VISIBLE_LINES {
            PpuMode::VBlank
        } else if self.line_dots < OAM_DOTS {
            PpuMode::OamSearch
        } else if self.line_dots < OAM_DOTS + TRANSFER_DOTS {
            PpuMode::PixelTransfer
        } else {
            PpuMode::HBlank
        };
    }

    fn render_scanline(&mut self, line: u8) {
        let screen_y = usize::from(line);
        for screen_x in 0..SCREEN_WIDTH {
            let screen_x_u8 = u8::try_from(screen_x).expect("screen width is smaller than u8::MAX");
            let color_index = self.background_color_index(screen_x_u8, line);
            let color = self.map_background_color(color_index);
            self.framebuffer[screen_y * SCREEN_WIDTH + screen_x] = color;
        }
    }

    fn background_color_index(&self, screen_x: u8, screen_y: u8) -> u8 {
        if !self.lcdc.background_enabled() {
            return 0;
        }

        let bg_x = screen_x.wrapping_add(self.scx);
        let bg_y = screen_y.wrapping_add(self.scy);
        let tile_col = usize::from(bg_x / 8);
        let tile_row = usize::from(bg_y / 8);
        let tile_map_index = tile_row * 32 + tile_col;
        let tile_number = self.vram[self.lcdc.background_tile_map_offset() + tile_map_index];
        let tile_data_offset = self.lcdc.tile_data_offset(tile_number);
        let row = usize::from(bg_y % 8);
        let low = self.vram[tile_data_offset + row * 2];
        let high = self.vram[tile_data_offset + row * 2 + 1];

        decode_tile_row(low, high)[usize::from(bg_x % 8)]
    }

    fn map_background_color(&self, color_index: u8) -> u32 {
        let palette_index = usize::from((self.bgp >> (color_index * 2)) & 0x03);
        DMG_SHADES[palette_index]
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpuMode {
    HBlank,
    VBlank,
    OamSearch,
    PixelTransfer,
}

impl PpuMode {
    #[must_use]
    fn stat_bits(self) -> u8 {
        match self {
            Self::HBlank => 0,
            Self::VBlank => 1,
            Self::OamSearch => 2,
            Self::PixelTransfer => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Lcdc {
    bits: u8,
}

impl Lcdc {
    #[must_use]
    fn new(bits: u8) -> Self {
        Self { bits }
    }

    #[must_use]
    fn raw(self) -> u8 {
        self.bits
    }

    fn set_raw(&mut self, value: u8) {
        self.bits = value;
    }

    #[must_use]
    fn background_enabled(self) -> bool {
        self.bits & 0x01 != 0
    }

    #[must_use]
    fn tile_data_offset(self, tile_number: u8) -> usize {
        if self.bits & 0x10 != 0 {
            usize::from(tile_number) * 16
        } else {
            let signed_tile_number = i8::from_ne_bytes([tile_number]);
            if signed_tile_number >= 0 {
                0x1000 + usize::from(signed_tile_number.unsigned_abs()) * 16
            } else {
                0x1000 - usize::from(signed_tile_number.unsigned_abs()) * 16
            }
        }
    }

    #[must_use]
    fn background_tile_map_offset(self) -> usize {
        if self.bits & 0x08 != 0 {
            0x1C00
        } else {
            0x1800
        }
    }
}

#[must_use]
pub fn decode_tile_row(low: u8, high: u8) -> [u8; 8] {
    let mut pixels = [0; 8];

    for (x, pixel) in pixels.iter_mut().enumerate() {
        let shift = 7 - x;
        let low_bit = (low >> shift) & 0x01;
        let high_bit = (high >> shift) & 0x01;
        *pixel = low_bit | (high_bit << 1);
    }

    pixels
}

#[must_use]
pub fn decode_tile(bytes: &[u8; 16]) -> [[u8; 8]; 8] {
    let mut tile = [[0; 8]; 8];

    for row in 0..8 {
        tile[row] = decode_tile_row(bytes[row * 2], bytes[row * 2 + 1]);
    }

    tile
}

#[cfg(test)]
mod tests {
    use super::{decode_tile, decode_tile_row, Ppu, PpuMode, DMG_SHADES};
    use crate::{cpu::TCycles, interrupt::InterruptFlags};

    #[test]
    fn vram_reads_and_writes_roundtrip() {
        let mut ppu = Ppu::new();

        ppu.write_vram(0x0000, 0x12);
        ppu.write_vram(0x1FFF, 0x34);

        assert_eq!(ppu.read_vram(0x0000), 0x12, "VRAM start should roundtrip");
        assert_eq!(ppu.read_vram(0x1FFF), 0x34, "VRAM end should roundtrip");
        assert_eq!(
            ppu.read_vram(0x2000),
            0xFF,
            "out-of-range VRAM reads as 0xFF"
        );
    }

    #[test]
    fn oam_reads_and_writes_roundtrip() {
        let mut ppu = Ppu::new();

        ppu.write_oam(0x0000, 0x56);
        ppu.write_oam(0x009F, 0x78);

        assert_eq!(ppu.read_oam(0x0000), 0x56, "OAM start should roundtrip");
        assert_eq!(ppu.read_oam(0x009F), 0x78, "OAM end should roundtrip");
        assert_eq!(ppu.read_oam(0x00A0), 0xFF, "out-of-range OAM reads as 0xFF");
    }

    #[test]
    fn lcd_registers_roundtrip_and_ly_ignores_cpu_writes() {
        let mut ppu = Ppu::new();

        ppu.write_register(0xFF40, 0x80);
        ppu.write_register(0xFF42, 0x11);
        ppu.write_register(0xFF43, 0x22);
        ppu.write_register(0xFF44, 0x99);
        ppu.write_register(0xFF45, 0x33);
        ppu.write_register(0xFF47, 0xE4);
        ppu.write_register(0xFF48, 0xD2);
        ppu.write_register(0xFF49, 0xC1);
        ppu.write_register(0xFF4A, 0x44);
        ppu.write_register(0xFF4B, 0x55);

        assert_eq!(ppu.read_register(0xFF40), 0x80, "LCDC should roundtrip");
        assert_eq!(ppu.read_register(0xFF42), 0x11, "SCY should roundtrip");
        assert_eq!(ppu.read_register(0xFF43), 0x22, "SCX should roundtrip");
        assert_eq!(
            ppu.read_register(0xFF44),
            0x00,
            "LY is read-only to CPU writes"
        );
        assert_eq!(ppu.read_register(0xFF45), 0x33, "LYC should roundtrip");
        assert_eq!(ppu.read_register(0xFF47), 0xE4, "BGP should roundtrip");
        assert_eq!(ppu.read_register(0xFF48), 0xD2, "OBP0 should roundtrip");
        assert_eq!(ppu.read_register(0xFF49), 0xC1, "OBP1 should roundtrip");
        assert_eq!(ppu.read_register(0xFF4A), 0x44, "WY should roundtrip");
        assert_eq!(ppu.read_register(0xFF4B), 0x55, "WX should roundtrip");
    }

    #[test]
    fn decode_tile_row_combines_planar_bits_left_to_right() {
        assert_eq!(
            decode_tile_row(0b1001_0110, 0b0101_0011),
            [1, 2, 0, 3, 0, 1, 3, 2],
            "low and high bitplanes should produce 2-bit color indices"
        );
    }

    #[test]
    fn decode_tile_decodes_eight_rows() {
        let mut bytes = [0; 16];
        bytes[0] = 0xFF;
        bytes[1] = 0x00;
        bytes[14] = 0x00;
        bytes[15] = 0xFF;

        let tile = decode_tile(&bytes);

        assert_eq!(
            tile[0], [1; 8],
            "first row should decode from bytes 0 and 1"
        );
        assert_eq!(
            tile[7], [2; 8],
            "last row should decode from bytes 14 and 15"
        );
    }

    #[test]
    fn background_render_uses_tile_map_tile_data_and_palette() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF47, 0b1110_0100);
        ppu.write_vram(0x0000, 0b1000_0000);
        ppu.write_vram(0x0001, 0b0100_0000);
        ppu.write_vram(0x1800, 0);

        ppu.render_background();

        assert_eq!(
            ppu.framebuffer()[0],
            DMG_SHADES[1],
            "pixel 0 should use color 1"
        );
        assert_eq!(
            ppu.framebuffer()[1],
            DMG_SHADES[2],
            "pixel 1 should use color 2"
        );
        assert_eq!(
            ppu.framebuffer()[2],
            DMG_SHADES[0],
            "pixel 2 should use color 0"
        );
    }

    #[test]
    fn background_render_uses_scroll_registers() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF47, 0b1110_0100);
        ppu.write_register(0xFF43, 1);
        ppu.write_vram(0x0000, 0b0100_0000);
        ppu.write_vram(0x0001, 0);
        ppu.write_vram(0x1800, 0);

        ppu.render_background();

        assert_eq!(
            ppu.framebuffer()[0],
            DMG_SHADES[1],
            "SCX should shift the viewport to background pixel x=1"
        );
    }

    #[test]
    fn tick_increments_ly_every_scanline_and_wraps_after_vblank() {
        let mut ppu = Ppu::new();
        let mut interrupts = InterruptFlags::default();

        ppu.tick(TCycles(456), &mut interrupts);
        assert_eq!(
            ppu.read_register(0xFF44),
            1,
            "one scanline should increment LY"
        );

        ppu.tick(TCycles(456 * 153), &mut interrupts);
        assert_eq!(
            ppu.read_register(0xFF44),
            0,
            "LY should wrap after line 153"
        );
    }

    #[test]
    fn mode_bits_follow_basic_scanline_timing() {
        let mut ppu = Ppu::new();
        let mut interrupts = InterruptFlags::default();

        assert_eq!(ppu.mode, PpuMode::OamSearch);
        assert_eq!(
            ppu.read_register(0xFF41) & 0x03,
            2,
            "line starts in OAM mode"
        );

        ppu.tick(TCycles(80), &mut interrupts);
        assert_eq!(
            ppu.read_register(0xFF41) & 0x03,
            3,
            "after OAM comes transfer"
        );

        ppu.tick(TCycles(172), &mut interrupts);
        assert_eq!(
            ppu.read_register(0xFF41) & 0x03,
            0,
            "after transfer comes HBlank"
        );
    }
}
