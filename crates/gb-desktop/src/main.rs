use gb_core::{
    bus::Bus,
    cartridge::Cartridge,
    cpu::Cpu,
    joypad::Button,
    ppu::{SCREEN_HEIGHT, SCREEN_WIDTH},
    GameBoy,
};
use pixels::{Pixels, SurfaceTexture};
use std::{
    env,
    error::Error,
    fs,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

const SCALE: u32 = 4;
const DMG_CPU_CLOCK_HZ: u64 = 4_194_304;
const DMG_TCYCLES_PER_FRAME: u64 = 456 * 154;

fn main() -> Result<(), Box<dyn Error>> {
    let options = Options::parse(env::args().skip(1))?;

    if options.demo {
        run_window(DisplaySource::Demo(Box::new(DemoFrame::new())))?;
        return Ok(());
    }

    let Some(rom_path) = options.rom_path else {
        eprintln!("Usage: gb-desktop <rom.gb> [--serial-steps N]");
        eprintln!("       gb-desktop --demo");
        return Ok(());
    };

    let rom = fs::read(rom_path)?;
    let cartridge = Cartridge::from_bytes(rom)?;

    println!("Title: {}", cartridge.title());
    println!("Type: {}", cartridge.cartridge_type());
    println!("ROM: {}", cartridge.rom_size());
    println!("RAM: {}", cartridge.ram_size());

    if let Some(steps) = options.serial_steps {
        run_serial_output(cartridge, steps)?;
    } else {
        run_window(DisplaySource::Emulator(Box::new(GameBoy::new(cartridge))))?;
    }

    Ok(())
}

#[derive(Debug, Default)]
struct Options {
    rom_path: Option<String>,
    serial_steps: Option<usize>,
    demo: bool,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, Box<dyn Error>> {
        let mut options = Self::default();
        let mut iter = args.into_iter();

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--serial-steps" => {
                    let Some(value) = iter.next() else {
                        return Err("--serial-steps requires a step count".into());
                    };
                    options.serial_steps = Some(value.parse()?);
                }
                "--demo" => options.demo = true,
                _ if options.rom_path.is_none() => options.rom_path = Some(arg),
                _ => return Err(format!("unknown argument: {arg}").into()),
            }
        }

        Ok(options)
    }
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

fn run_window(source: DisplaySource) -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = DesktopApp::new(source);
    event_loop.run_app(&mut app)?;

    Ok(())
}

#[derive(Debug)]
enum DisplaySource {
    Emulator(Box<GameBoy>),
    Demo(Box<DemoFrame>),
}

impl DisplaySource {
    fn next_frame(&mut self) -> Option<&[u32]> {
        match self {
            Self::Emulator(game_boy) => {
                if let Err(error) = game_boy.run_until_frame() {
                    eprintln!("{error}");
                    return None;
                }

                Some(game_boy.framebuffer())
            }
            Self::Demo(demo) => Some(demo.next_frame()),
        }
    }

    fn set_button(&mut self, button: Button, pressed: bool) {
        if let Self::Emulator(game_boy) = self {
            game_boy.set_button(button, pressed);
        }
    }
}

#[derive(Debug)]
struct DemoFrame {
    frame: Box<[u32]>,
    tick: u8,
}

impl DemoFrame {
    fn new() -> Self {
        Self {
            frame: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT].into_boxed_slice(),
            tick: 0,
        }
    }

    fn next_frame(&mut self) -> &[u32] {
        self.tick = self.tick.wrapping_add(1);

        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let checker = ((x / 8) + (y / 8) + usize::from(self.tick / 16)) % 4;
                self.frame[y * SCREEN_WIDTH + x] = match checker {
                    0 => 0xFFFF_FFFF,
                    1 => 0xFFAA_AAAA,
                    2 => 0xFF55_5555,
                    _ => 0xFF00_0000,
                };
            }
        }

        &self.frame
    }
}

struct DesktopApp {
    source: DisplaySource,
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    frame_pacer: FramePacer,
}

impl DesktopApp {
    fn new(source: DisplaySource) -> Self {
        Self {
            source,
            window: None,
            pixels: None,
            frame_pacer: FramePacer::new(),
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let width = u32::try_from(SCREEN_WIDTH).expect("screen width fits in u32");
        let height = u32::try_from(SCREEN_HEIGHT).expect("screen height fits in u32");
        let size = LogicalSize::new(f64::from(width * SCALE), f64::from(height * SCALE));
        let attributes = WindowAttributes::default()
            .with_title("rustboy")
            .with_inner_size(size)
            .with_min_inner_size(size);

        let window = event_loop
            .create_window(attributes)
            .expect("desktop window should be created");
        let window: &'static Window = Box::leak(Box::new(window));
        let surface_texture = SurfaceTexture::new(width * SCALE, height * SCALE, window);
        let pixels = Pixels::new(width, height, surface_texture)
            .expect("pixel framebuffer should be created");

        self.pixels = Some(pixels);
        self.window = Some(window);
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        if !self.frame_pacer.should_present(Instant::now()) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.frame_pacer.next_frame_at()));
            return;
        }

        let Some(pixels) = self.pixels.as_mut() else {
            return;
        };
        let Some(framebuffer) = self.source.next_frame() else {
            event_loop.exit();
            return;
        };

        copy_framebuffer(framebuffer, pixels.frame_mut());

        if let Err(error) = pixels.render() {
            eprintln!("{error}");
            event_loop.exit();
            return;
        }

        self.frame_pacer.schedule_next_frame(Instant::now());
    }
}

impl ApplicationHandler for DesktopApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            self.create_window(event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(button) = key_to_button(event.physical_key) {
                    self.source
                        .set_button(button, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(pixels) = self.pixels.as_mut() {
                    let _ = pixels.resize_surface(size.width, size.height);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window {
            let now = Instant::now();

            if self.frame_pacer.should_present(now) {
                event_loop.set_control_flow(ControlFlow::Poll);
                window.request_redraw();
            } else {
                event_loop
                    .set_control_flow(ControlFlow::WaitUntil(self.frame_pacer.next_frame_at()));
            }
        }
    }
}

#[derive(Debug)]
struct FramePacer {
    frame_interval: Duration,
    next_frame_at: Instant,
}

impl FramePacer {
    fn new() -> Self {
        Self {
            frame_interval: dmg_frame_interval(),
            next_frame_at: Instant::now(),
        }
    }

    fn should_present(&self, now: Instant) -> bool {
        now >= self.next_frame_at
    }

    fn next_frame_at(&self) -> Instant {
        self.next_frame_at
    }

    fn schedule_next_frame(&mut self, now: Instant) {
        self.next_frame_at = now + self.frame_interval;
    }
}

fn dmg_frame_interval() -> Duration {
    let nanos = (DMG_TCYCLES_PER_FRAME * 1_000_000_000 + (DMG_CPU_CLOCK_HZ / 2)) / DMG_CPU_CLOCK_HZ;
    Duration::from_nanos(nanos)
}

fn key_to_button(key: PhysicalKey) -> Option<Button> {
    match key {
        PhysicalKey::Code(KeyCode::ArrowRight) => Some(Button::Right),
        PhysicalKey::Code(KeyCode::ArrowLeft) => Some(Button::Left),
        PhysicalKey::Code(KeyCode::ArrowUp) => Some(Button::Up),
        PhysicalKey::Code(KeyCode::ArrowDown) => Some(Button::Down),
        PhysicalKey::Code(KeyCode::KeyZ) => Some(Button::A),
        PhysicalKey::Code(KeyCode::KeyX) => Some(Button::B),
        PhysicalKey::Code(KeyCode::Enter) => Some(Button::Start),
        PhysicalKey::Code(KeyCode::ShiftRight) => Some(Button::Select),
        _ => None,
    }
}

fn copy_framebuffer(source: &[u32], target: &mut [u8]) {
    for (pixel, rgba) in source.iter().zip(target.chunks_exact_mut(4)) {
        let [alpha, red, green, blue] = pixel.to_be_bytes();
        rgba.copy_from_slice(&[red, green, blue, alpha]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dmg_frame_interval_matches_hardware_refresh_rate() {
        let interval = dmg_frame_interval();

        assert_eq!(
            interval.as_nanos(),
            16_742_706,
            "DMG frame interval should be approximately 16.74 ms from 70224 T-cycles at 4194304 Hz"
        );
    }

    #[test]
    fn frame_pacer_waits_until_next_scheduled_frame() {
        let now = Instant::now();
        let mut pacer = FramePacer {
            frame_interval: Duration::from_millis(10),
            next_frame_at: now,
        };

        assert!(
            pacer.should_present(now),
            "pacer should allow presentation at the scheduled instant"
        );

        pacer.schedule_next_frame(now);

        assert!(
            !pacer.should_present(now + Duration::from_millis(9)),
            "pacer should hold redraw requests until the frame interval has elapsed"
        );
        assert!(
            pacer.should_present(now + Duration::from_millis(10)),
            "pacer should allow redraw at the next scheduled frame"
        );
    }
}
