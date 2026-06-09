use gb_core::cartridge::Cartridge;
use std::{env, error::Error, fs};

fn main() -> Result<(), Box<dyn Error>> {
    let Some(rom_path) = env::args().nth(1) else {
        eprintln!("Usage: gb-desktop <rom.gb>");
        return Ok(());
    };

    let rom = fs::read(rom_path)?;
    let cartridge = Cartridge::from_bytes(rom)?;

    println!("Title: {}", cartridge.title());
    println!("Type: {}", cartridge.cartridge_type());
    println!("ROM: {}", cartridge.rom_size());
    println!("RAM: {}", cartridge.ram_size());

    Ok(())
}
