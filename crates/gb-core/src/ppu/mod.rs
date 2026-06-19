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
    stat_interrupt_active: bool,
    window_y_triggered: bool,
    window_line: u8,
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
            stat_interrupt_active: false,
            window_y_triggered: false,
            window_line: 0,
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
            STAT_ADDR => {
                (self.stat & 0xF8) | (u8::from(self.ly == self.lyc) << 2) | self.mode.stat_bits()
            }
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
            self.update_mode(interrupts);

            if self.line_dots >= DOTS_PER_LINE {
                self.finish_scanline(interrupts);
            }
        }
    }

    #[cfg(any(test, feature = "test-trace"))]
    #[must_use]
    pub(crate) fn trace_ly_and_mode(&self) -> (u8, PpuMode) {
        (self.ly, self.mode)
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
            self.update_stat_interrupt(interrupts);
        } else if self.ly >= LINES_PER_FRAME {
            self.ly = 0;
            self.mode = PpuMode::OamSearch;
            self.window_y_triggered = false;
            self.window_line = 0;
            self.frame_ready = false;
            self.update_stat_interrupt(interrupts);
        } else {
            self.update_mode(interrupts);
        }
    }

    fn update_mode(&mut self, interrupts: &mut InterruptFlags) {
        let mode = if self.ly >= VISIBLE_LINES {
            PpuMode::VBlank
        } else if self.line_dots < OAM_DOTS {
            PpuMode::OamSearch
        } else if self.line_dots < OAM_DOTS + TRANSFER_DOTS {
            PpuMode::PixelTransfer
        } else {
            PpuMode::HBlank
        };

        if self.mode == mode {
            self.update_stat_interrupt(interrupts);
        } else {
            self.mode = mode;
            self.update_stat_interrupt(interrupts);
        }
    }

    fn update_stat_interrupt(&mut self, interrupts: &mut InterruptFlags) {
        let signal = self.stat_interrupt_signal();

        if signal && !self.stat_interrupt_active {
            interrupts.request(Interrupt::LcdStat);
        }

        self.stat_interrupt_active = signal;
    }

    fn stat_interrupt_signal(&self) -> bool {
        let lyc_signal = self.ly == self.lyc && self.stat & 0x40 != 0;
        let mode_signal = match self.mode {
            PpuMode::HBlank => self.stat & 0x08 != 0,
            PpuMode::VBlank => self.stat & 0x10 != 0,
            PpuMode::OamSearch => self.stat & 0x20 != 0,
            PpuMode::PixelTransfer => false,
        };

        lyc_signal || mode_signal
    }

    fn render_scanline(&mut self, line: u8) {
        let screen_y = usize::from(line);
        let mut window_used = false;

        if self.lcdc.window_enabled() && line == self.wy {
            self.window_y_triggered = true;
        }

        for screen_x in 0..SCREEN_WIDTH {
            let screen_x_u8 = u8::try_from(screen_x).expect("screen width is smaller than u8::MAX");
            let (color_index, used_window) =
                self.background_or_window_color_index(screen_x_u8, line);
            window_used |= used_window;
            let color = self.map_background_color(color_index);
            self.framebuffer[screen_y * SCREEN_WIDTH + screen_x] = color;
        }

        self.render_sprites_on_scanline(line);

        if window_used {
            self.window_line = self.window_line.wrapping_add(1);
        }
    }

    fn background_or_window_color_index(&self, screen_x: u8, screen_y: u8) -> (u8, bool) {
        if !self.lcdc.background_enabled() {
            return (0, false);
        }

        if let Some(color_index) = self.window_color_index(screen_x) {
            return (color_index, true);
        }

        (self.background_color_index(screen_x, screen_y), false)
    }

    fn background_color_index(&self, screen_x: u8, screen_y: u8) -> u8 {
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

    fn window_color_index(&self, screen_x: u8) -> Option<u8> {
        if !self.lcdc.window_enabled() || !self.window_y_triggered {
            return None;
        }

        let window_left = i16::from(self.wx) - 7;
        let screen_x = i16::from(screen_x);

        if screen_x < window_left {
            return None;
        }

        let window_x = usize::try_from(screen_x - window_left)
            .expect("window x should be non-negative after bounds check");
        let window_y = usize::from(self.window_line);
        let tile_col = (window_x / 8) & 31;
        let tile_row = (window_y / 8) & 31;
        let tile_map_index = tile_row * 32 + tile_col;
        let tile_number = self.vram[self.lcdc.window_tile_map_offset() + tile_map_index];
        let tile_data_offset = self.lcdc.tile_data_offset(tile_number);
        let row = window_y % 8;
        let low = self.vram[tile_data_offset + row * 2];
        let high = self.vram[tile_data_offset + row * 2 + 1];

        Some(decode_tile_row(low, high)[window_x % 8])
    }

    fn map_background_color(&self, color_index: u8) -> u32 {
        let palette_index = usize::from((self.bgp >> (color_index * 2)) & 0x03);
        DMG_SHADES[palette_index]
    }

    fn render_sprites_on_scanline(&mut self, line: u8) {
        if !self.lcdc.sprites_enabled() {
            return;
        }

        let sprite_height = self.lcdc.sprite_height();
        let line_i16 = i16::from(line);

        let mut visible_sprites = Vec::with_capacity(10);

        for sprite_index in 0..40 {
            let sprite = OamEntry::from_oam(&self.oam[sprite_index * 4..sprite_index * 4 + 4]);
            let sprite_y = i16::from(sprite.y) - 16;

            if line_i16 < sprite_y || line_i16 >= sprite_y + i16::from(sprite_height) {
                continue;
            }

            visible_sprites.push((sprite_index, sprite));
            if visible_sprites.len() == 10 {
                break;
            }
        }

        visible_sprites.sort_by(|(left_index, left), (right_index, right)| {
            right
                .x
                .cmp(&left.x)
                .then_with(|| right_index.cmp(left_index))
        });

        for (_, sprite) in visible_sprites {
            let sprite_y = i16::from(sprite.y) - 16;

            let row_in_sprite = usize::try_from(line_i16 - sprite_y)
                .expect("visible sprite row should be non-negative");
            let tile_row = if sprite.y_flip {
                usize::from(sprite_height) - 1 - row_in_sprite
            } else {
                row_in_sprite
            };
            let tile_number = if sprite_height == 16 {
                sprite.tile_index & 0xFE
            } else {
                sprite.tile_index
            };
            let tile_number = tile_number.wrapping_add(u8::try_from(tile_row / 8).unwrap_or(0));
            let row = tile_row % 8;
            let tile_offset = usize::from(tile_number) * 16;
            let pixels = decode_tile_row(
                self.vram[tile_offset + row * 2],
                self.vram[tile_offset + row * 2 + 1],
            );
            let sprite_x = i16::from(sprite.x) - 8;

            for screen_x in 0..SCREEN_WIDTH {
                let screen_x_i16 = i16::try_from(screen_x).expect("screen width should fit in i16");

                if screen_x_i16 < sprite_x || screen_x_i16 >= sprite_x + 8 {
                    continue;
                }

                let column = usize::try_from(screen_x_i16 - sprite_x)
                    .expect("visible sprite column should be non-negative");
                let tile_column = if sprite.x_flip { 7 - column } else { column };
                let color_index = pixels[tile_column];

                if color_index == 0 {
                    continue;
                }

                let screen_x_u8 =
                    u8::try_from(screen_x).expect("screen width is smaller than u8::MAX");
                let (background_index, _) =
                    self.background_or_window_color_index(screen_x_u8, line);

                if sprite.bg_priority && background_index != 0 {
                    continue;
                }

                self.framebuffer[usize::from(line) * SCREEN_WIDTH + screen_x] =
                    self.map_object_color(color_index, sprite.palette);
            }
        }
    }

    fn map_object_color(&self, color_index: u8, palette: ObjectPalette) -> u32 {
        let register = match palette {
            ObjectPalette::Obp0 => self.obp0,
            ObjectPalette::Obp1 => self.obp1,
        };
        let palette_index = usize::from((register >> (color_index * 2)) & 0x03);
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
    fn window_enabled(self) -> bool {
        self.bits & 0x20 != 0
    }

    #[must_use]
    fn sprites_enabled(self) -> bool {
        self.bits & 0x02 != 0
    }

    #[must_use]
    fn sprite_height(self) -> u8 {
        if self.bits & 0x04 != 0 {
            16
        } else {
            8
        }
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

    #[must_use]
    fn window_tile_map_offset(self) -> usize {
        if self.bits & 0x40 != 0 {
            0x1C00
        } else {
            0x1800
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectPalette {
    Obp0,
    Obp1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OamEntry {
    pub y: u8,
    pub x: u8,
    pub tile_index: u8,
    pub bg_priority: bool,
    pub y_flip: bool,
    pub x_flip: bool,
    pub palette: ObjectPalette,
}

impl OamEntry {
    #[must_use]
    pub fn from_oam(bytes: &[u8]) -> Self {
        let flags = bytes.get(3).copied().unwrap_or(0);

        Self {
            y: bytes.first().copied().unwrap_or(0),
            x: bytes.get(1).copied().unwrap_or(0),
            tile_index: bytes.get(2).copied().unwrap_or(0),
            bg_priority: flags & 0x80 != 0,
            y_flip: flags & 0x40 != 0,
            x_flip: flags & 0x20 != 0,
            palette: if flags & 0x10 != 0 {
                ObjectPalette::Obp1
            } else {
                ObjectPalette::Obp0
            },
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
    use super::{
        decode_tile, decode_tile_row, OamEntry, ObjectPalette, Ppu, PpuMode, DMG_SHADES,
        SCREEN_WIDTH,
    };
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
    fn oam_entry_decodes_sprite_attributes() {
        let sprite = OamEntry::from_oam(&[16, 8, 3, 0b1111_0000]);

        assert_eq!(sprite.y, 16);
        assert_eq!(sprite.x, 8);
        assert_eq!(sprite.tile_index, 3);
        assert!(sprite.bg_priority);
        assert!(sprite.y_flip);
        assert!(sprite.x_flip);
        assert_eq!(sprite.palette, ObjectPalette::Obp1);
    }

    #[test]
    fn basic_8x8_sprite_renders_nonzero_pixels_over_background() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0x93);
        ppu.write_register(0xFF48, 0b1110_0100);
        ppu.write_vram(0x0020, 0b1000_0000);
        ppu.write_vram(0x0021, 0);
        ppu.write_oam(0, 16);
        ppu.write_oam(1, 8);
        ppu.write_oam(2, 2);

        ppu.render_background();

        assert_eq!(
            ppu.framebuffer()[0],
            DMG_SHADES[1],
            "sprite at hardware x=8 y=16 should draw at screen origin"
        );
        assert_eq!(
            ppu.framebuffer()[1],
            DMG_SHADES[0],
            "sprite color index 0 should remain transparent"
        );
    }

    #[test]
    fn sprite_flips_and_obp1_palette_are_applied() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0x93);
        ppu.write_register(0xFF49, 0b0001_1011);
        ppu.write_vram(0x001E, 0b0000_0001);
        ppu.write_vram(0x001F, 0);
        ppu.write_oam(0, 16);
        ppu.write_oam(1, 8);
        ppu.write_oam(2, 1);
        ppu.write_oam(3, 0b0111_0000);

        ppu.render_background();

        assert_eq!(
            ppu.framebuffer()[0],
            DMG_SHADES[2],
            "x/y flipped sprite should use OBP1 for the visible pixel"
        );
    }

    #[test]
    fn sprite_priority_hides_sprite_behind_nonzero_background() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0x93);
        ppu.write_register(0xFF47, 0b1110_0100);
        ppu.write_register(0xFF48, 0b0001_1011);
        ppu.write_vram(0x0000, 0b1000_0000);
        ppu.write_vram(0x0001, 0);
        ppu.write_vram(0x1800, 0);
        ppu.write_vram(0x0020, 0b1000_0000);
        ppu.write_vram(0x0021, 0);
        ppu.write_oam(0, 16);
        ppu.write_oam(1, 8);
        ppu.write_oam(2, 2);
        ppu.write_oam(3, 0x80);

        ppu.render_background();

        assert_eq!(
            ppu.framebuffer()[0],
            DMG_SHADES[1],
            "priority sprite should stay behind nonzero background pixels"
        );
    }

    #[test]
    fn sprite_8x16_mode_uses_second_tile_for_lower_half() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0x97);
        ppu.write_register(0xFF48, 0b1110_0100);
        ppu.write_vram(0x0030, 0b1000_0000);
        ppu.write_vram(0x0031, 0);
        ppu.write_oam(0, 16);
        ppu.write_oam(1, 8);
        ppu.write_oam(2, 2);

        ppu.render_background();

        assert_eq!(
            ppu.framebuffer()[8 * SCREEN_WIDTH],
            DMG_SHADES[1],
            "8x16 sprites should draw the lower half from the next tile"
        );
    }

    #[test]
    fn window_renders_at_wx_minus_seven_and_wy() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0xB9);
        ppu.write_register(0xFF47, 0b1110_0100);
        ppu.write_register(0xFF4A, 2);
        ppu.write_register(0xFF4B, 8);
        ppu.write_vram(0x0010, 0b1000_0000);
        ppu.write_vram(0x0011, 0);
        ppu.write_vram(0x1800, 1);
        ppu.write_vram(0x1C00, 0);

        ppu.render_background();

        assert_eq!(
            ppu.framebuffer()[2 * SCREEN_WIDTH],
            DMG_SHADES[0],
            "before WX-7 the background should remain visible"
        );
        assert_eq!(
            ppu.framebuffer()[2 * SCREEN_WIDTH + 1],
            DMG_SHADES[1],
            "window pixel 0 should appear at screen x = WX - 7"
        );
    }

    #[test]
    fn window_uses_selected_tile_map() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0xF1);
        ppu.write_register(0xFF47, 0b1110_0100);
        ppu.write_register(0xFF4A, 0);
        ppu.write_register(0xFF4B, 7);
        ppu.write_vram(0x0010, 0b1000_0000);
        ppu.write_vram(0x0011, 0);
        ppu.write_vram(0x1800, 0);
        ppu.write_vram(0x1C00, 1);

        ppu.render_background();

        assert_eq!(
            ppu.framebuffer()[0],
            DMG_SHADES[1],
            "LCDC bit 6 should select the 0x9C00 window tile map"
        );
    }

    #[test]
    fn window_uses_internal_line_counter_only_on_visible_window_lines() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0xB1);
        ppu.write_register(0xFF47, 0b1110_0100);
        ppu.write_register(0xFF4A, 0);
        ppu.write_register(0xFF4B, 7);
        ppu.write_vram(0x0000, 0b1000_0000);
        ppu.write_vram(0x0001, 0);
        ppu.write_vram(0x0002, 0b0100_0000);
        ppu.write_vram(0x0003, 0);
        ppu.write_vram(0x0004, 0);
        ppu.write_vram(0x0005, 0b1000_0000);
        ppu.write_vram(0x1800, 0);

        ppu.render_scanline(0);
        ppu.write_register(0xFF4B, 200);
        ppu.render_scanline(1);
        ppu.write_register(0xFF4B, 7);
        ppu.render_scanline(2);

        assert_eq!(
            ppu.framebuffer()[2 * SCREEN_WIDTH + 1],
            DMG_SHADES[1],
            "hidden window lines should not advance the internal window line counter"
        );
    }

    #[test]
    fn window_does_not_appear_when_wy_is_changed_after_trigger_line() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0xB1);
        ppu.write_register(0xFF47, 0b1110_0100);
        ppu.write_register(0xFF4A, 10);
        ppu.write_register(0xFF4B, 7);
        ppu.write_vram(0x0000, 0);
        ppu.write_vram(0x0001, 0);
        ppu.write_vram(0x0010, 0b1000_0000);
        ppu.write_vram(0x0011, 0);
        ppu.write_vram(0x1800, 1);

        ppu.render_scanline(8);
        ppu.write_register(0xFF4A, 0);
        ppu.render_scanline(9);

        assert_eq!(
            ppu.framebuffer()[9 * SCREEN_WIDTH],
            DMG_SHADES[0],
            "changing WY after its line has already passed should not trigger the window"
        );
    }

    #[test]
    fn sprites_are_limited_to_first_ten_visible_oam_entries_per_line() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0x93);
        ppu.write_register(0xFF48, 0b1110_0100);
        ppu.write_vram(0x0010, 0b1000_0000);
        ppu.write_vram(0x0011, 0);

        for sprite in 0..10 {
            let offset = sprite * 4;
            let offset = u16::try_from(offset).expect("test OAM offset fits in u16");
            ppu.write_oam(offset, 16);
            ppu.write_oam(offset + 1, 168);
            ppu.write_oam(offset + 2, 1);
        }

        ppu.write_oam(40, 16);
        ppu.write_oam(41, 8);
        ppu.write_oam(42, 1);

        ppu.render_background();

        assert_eq!(
            ppu.framebuffer()[0],
            DMG_SHADES[0],
            "the eleventh visible sprite on a line should not be drawn"
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

    #[test]
    fn lyc_stat_interrupt_is_requested_when_coincidence_source_rises() {
        let mut ppu = Ppu::new();
        let mut interrupts = InterruptFlags::default();
        ppu.write_register(0xFF41, 0x40);
        ppu.write_register(0xFF45, 1);

        ppu.tick(TCycles(456), &mut interrupts);

        assert_eq!(
            interrupts.raw() & 0x02,
            0x02,
            "LYC coincidence with STAT bit 6 enabled should request LCD STAT interrupt"
        );
        assert_eq!(
            ppu.read_register(0xFF41) & 0x04,
            0x04,
            "STAT coincidence bit should reflect LY == LYC"
        );
    }
}
