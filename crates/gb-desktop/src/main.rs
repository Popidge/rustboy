use gb_core::{bus::Bus, cartridge::Cartridge, cpu::Cpu};
use std::{env, error::Error, fs};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let Some(rom_path) = args.next() else {
        eprintln!("Usage: gb-desktop <rom.gb> [--serial-steps N]");
        return Ok(());
    };
    let serial_steps = parse_serial_steps(args)?;

    let rom = fs::read(rom_path)?;
    let cartridge = Cartridge::from_bytes(rom)?;

    println!("Title: {}", cartridge.title());
    println!("Type: {}", cartridge.cartridge_type());
    println!("ROM: {}", cartridge.rom_size());
    println!("RAM: {}", cartridge.ram_size());

    if let Some(steps) = serial_steps {
        run_serial_output(cartridge, steps)?;
    }

    Ok(())
}

fn parse_serial_steps(
    args: impl IntoIterator<Item = String>,
) -> Result<Option<usize>, Box<dyn Error>> {
    let mut serial_steps = None;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        if arg == "--serial-steps" {
            let Some(value) = iter.next() else {
                return Err("--serial-steps requires a step count".into());
            };
            serial_steps = Some(value.parse()?);
        } else {
            return Err(format!("unknown argument: {arg}").into());
        }
    }

    Ok(serial_steps)
}

fn run_serial_output(cartridge: Cartridge, steps: usize) -> Result<(), Box<dyn Error>> {
    let mut cpu = Cpu::new_dmg_post_boot();
    let mut bus = Bus::new(cartridge);

    for _ in 0..steps {
        let cycles = cpu.step(&mut bus)?;
        bus.tick(cycles);
    }

    let output = bus.take_serial_output();

    if !output.is_empty() {
        println!("Serial: {}", String::from_utf8_lossy(&output));
    }

    Ok(())
}
