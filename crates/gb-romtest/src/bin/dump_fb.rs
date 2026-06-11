//! Quick utility to dump the framebuffer after running a ROM for N seconds.
//! Usage: cargo run --release --bin dump_fb -- <rom_path> [emulated_seconds]
use gb_core::{cartridge::Cartridge, ppu, GameBoy};
use std::env;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let rom_path = &args[1];
    let emu_seconds: u64 = args.get(2).map_or(37, |s| s.parse().unwrap_or(37));

    let rom = fs::read(rom_path)?;
    let cartridge = Cartridge::from_bytes(rom)?;
    let mut gb = GameBoy::new(cartridge);

    let target_tcycles = emu_seconds * 4_194_304;
    let mut cycles = 0u64;

    while cycles < target_tcycles {
        let c = gb.step()?;
        cycles += u64::from(c.0);
    }

    let fb = gb.framebuffer();
    let out_stem = Path::new(rom_path).file_stem().unwrap().to_str().unwrap();
    let out_path = format!("{out_stem}_dump.png");

    // Save as PNG
    let mut img = image::RgbaImage::new(ppu::SCREEN_WIDTH as u32, ppu::SCREEN_HEIGHT as u32);
    for (i, &pixel) in fb.iter().enumerate() {
        let x = (i % ppu::SCREEN_WIDTH) as u32;
        let y = (i / ppu::SCREEN_WIDTH) as u32;
        img.put_pixel(
            x,
            y,
            image::Rgba([
                ((pixel >> 16) & 0xFF) as u8,
                ((pixel >> 8) & 0xFF) as u8,
                (pixel & 0xFF) as u8,
                0xFF,
            ]),
        );
    }
    img.save(&out_path)?;
    println!("wrote {}", out_path);
    Ok(())
}
