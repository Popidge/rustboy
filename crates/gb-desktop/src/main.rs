use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use font8x8::UnicodeFonts;
use gb_core::{
    apu::{StereoSample, AUDIO_SAMPLE_RATE},
    boot_rom::DmgBootRom,
    cartridge::Cartridge,
    joypad::Button,
    ppu::{SCREEN_HEIGHT, SCREEN_WIDTH},
    GameBoy,
};
use pixels::{Pixels, SurfaceTexture};
use std::{
    cmp::Ordering,
    collections::VecDeque,
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
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
const HARNESS_WIDTH: usize = 1280;
const HARNESS_HEIGHT: usize = 800;
const TREE_WIDTH: usize = 360;
const STATUS_LEFT: usize = 1016;
const TOP_PANEL_HEIGHT: usize = 610;
const BOTTOM_PANEL_TOP: usize = 620;
const GB_VIEW_SCALE: usize = 3;
const DMG_CPU_CLOCK_HZ: u64 = 4_194_304;
const DMG_TCYCLES_PER_FRAME: u64 = 456 * 154;
const FRAME_HASH_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FRAME_HASH_PRIME: u64 = 0x0000_0100_0000_01B3;
const EMULATION_THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;

fn main() -> Result<(), Box<dyn Error>> {
    let options = Options::parse(env::args().skip(1))?;

    if options.demo {
        run_window(DisplaySource::Demo(Box::new(DemoFrame::new())))?;
        return Ok(());
    }

    if options.harness || options.rom_path.is_none() {
        run_window(DisplaySource::Harness(Box::new(Harness::discover(
            PathBuf::from("test-roms"),
        )?)))?;
        return Ok(());
    }

    let Some(rom_path) = options.rom_path else {
        eprintln!("Usage: gb-desktop [--harness]");
        eprintln!("       gb-desktop <rom.gb> [--boot-rom dmg_boot.bin] [--serial-steps N]");
        eprintln!("       gb-desktop <rom.gb> --mooneye-steps N");
        eprintln!("       gb-desktop <rom.gb> --frames N --dump-frame hash|blocks:N|pixels|all");
        eprintln!("       gb-desktop --demo");
        return Ok(());
    };

    let rom_path = PathBuf::from(rom_path);
    let rom = fs::read(&rom_path)?;
    let cartridge = Cartridge::from_bytes(rom)?;
    let boot_rom = options
        .boot_rom_path
        .map(fs::read)
        .transpose()?
        .map(DmgBootRom::from_bytes)
        .transpose()?;

    println!("Title: {}", cartridge.title());
    println!("Type: {}", cartridge.cartridge_type());
    println!("ROM: {}", cartridge.rom_size());
    println!("RAM: {}", cartridge.ram_size());

    if let Some(steps) = options.serial_steps {
        run_serial_output(cartridge, boot_rom, steps)?;
    } else if let Some(steps) = options.mooneye_steps {
        run_mooneye_output(cartridge, boot_rom, steps)?;
    } else if let Some(frames) = options.frames {
        run_frame_dump(cartridge, boot_rom, frames, options.dump_frame)?;
    } else {
        let save_path = save_path_for_rom(&rom_path);
        let mut game_boy = new_game_boy(cartridge, boot_rom);
        load_save_if_present(&mut game_boy, &save_path)?;
        run_window(DisplaySource::Emulator(Box::new(game_boy), save_path))?;
    }

    Ok(())
}

#[derive(Debug, Default)]
struct Options {
    rom_path: Option<String>,
    boot_rom_path: Option<String>,
    serial_steps: Option<usize>,
    mooneye_steps: Option<usize>,
    frames: Option<usize>,
    dump_frame: FrameDumpMode,
    demo: bool,
    harness: bool,
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
                "--boot-rom" => {
                    let Some(path) = iter.next() else {
                        return Err("--boot-rom requires a 256-byte DMG boot ROM path".into());
                    };
                    options.boot_rom_path = Some(path);
                }
                "--mooneye-steps" => {
                    let Some(value) = iter.next() else {
                        return Err("--mooneye-steps requires a step count".into());
                    };
                    options.mooneye_steps = Some(value.parse()?);
                }
                "--frames" => {
                    let Some(value) = iter.next() else {
                        return Err("--frames requires a frame count".into());
                    };
                    options.frames = Some(value.parse()?);
                }
                "--dump-frame" => {
                    let Some(value) = iter.next() else {
                        return Err("--dump-frame requires hash, blocks:N, pixels, or all".into());
                    };
                    options.dump_frame = value.parse()?;
                }
                "--demo" => options.demo = true,
                "--harness" => options.harness = true,
                _ if options.rom_path.is_none() => options.rom_path = Some(arg),
                _ => return Err(format!("unknown argument: {arg}").into()),
            }
        }

        Ok(options)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum FrameDumpMode {
    #[default]
    Hash,
    Blocks {
        block_size: usize,
    },
    Pixels,
    All,
}

impl std::str::FromStr for FrameDumpMode {
    type Err = Box<dyn Error>;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "hash" => Ok(Self::Hash),
            "pixels" => Ok(Self::Pixels),
            "all" => Ok(Self::All),
            _ => {
                let Some(block_size) = value.strip_prefix("blocks:") else {
                    return Err(
                        format!("unknown frame dump mode {value:?}; expected hash, blocks:N, pixels, or all")
                            .into(),
                    );
                };
                let block_size = block_size.parse::<usize>()?;
                if block_size == 0 {
                    return Err("block size must be greater than zero".into());
                }
                Ok(Self::Blocks { block_size })
            }
        }
    }
}

fn new_game_boy(cartridge: Cartridge, boot_rom: Option<DmgBootRom>) -> GameBoy {
    match boot_rom {
        Some(boot_rom) => GameBoy::new_with_boot_rom(cartridge, boot_rom),
        None => GameBoy::new(cartridge),
    }
}

fn run_serial_output(
    cartridge: Cartridge,
    boot_rom: Option<DmgBootRom>,
    steps: usize,
) -> Result<(), Box<dyn Error>> {
    run_on_emulation_thread(move || {
        let mut game_boy = new_game_boy(cartridge, boot_rom);

        for _ in 0..steps {
            game_boy.step().map_err(|error| error.to_string())?;
        }

        let output = game_boy.take_serial_output();

        if !output.is_empty() {
            println!("Serial: {}", String::from_utf8_lossy(&output));
        }

        Ok(())
    })
}

fn run_mooneye_output(
    cartridge: Cartridge,
    boot_rom: Option<DmgBootRom>,
    steps: usize,
) -> Result<(), Box<dyn Error>> {
    run_on_emulation_thread(move || {
        let mut game_boy = new_game_boy(cartridge, boot_rom);

        for step in 0..steps {
            match game_boy.step() {
                Ok(_) => {}
                Err(gb_core::cpu::CpuError::UnimplementedOpcode { pc, opcode })
                    if opcode == 0xED =>
                {
                    let registers = game_boy.registers();
                    let passed = registers.b == 3
                        && registers.c == 5
                        && registers.d == 8
                        && registers.e == 13
                        && registers.h == 21
                        && registers.l == 34;

                    println!(
                        "Mooneye: {} step={} pc={pc:04X} opcode={opcode:02X} B={} C={} D={} E={} H={} L={}",
                        if passed { "Passed" } else { "Failed" },
                        step + 1,
                        registers.b,
                        registers.c,
                        registers.d,
                        registers.e,
                        registers.h,
                        registers.l
                    );

                    return Ok(());
                }
                Err(error) => return Err(error.to_string()),
            }
        }

        let registers = game_boy.registers();
        println!(
            "Mooneye: Timeout steps={steps} B={} C={} D={} E={} H={} L={}",
            registers.b, registers.c, registers.d, registers.e, registers.h, registers.l
        );

        Ok(())
    })
}

fn run_frame_dump(
    cartridge: Cartridge,
    boot_rom: Option<DmgBootRom>,
    frames: usize,
    dump_mode: FrameDumpMode,
) -> Result<(), Box<dyn Error>> {
    run_on_emulation_thread(move || {
        let mut game_boy = new_game_boy(cartridge, boot_rom);

        for _ in 0..frames {
            game_boy
                .run_until_frame()
                .map_err(|error| error.to_string())?;
        }

        dump_framebuffer(game_boy.framebuffer(), frames, dump_mode);

        Ok(())
    })
}

/// Runs bounded headless emulation away from the platform main-thread stack.
///
/// Debug builds can exceed the Windows main-thread stack while stepping the
/// emulator. Windowed execution must remain on the main thread, but the
/// headless diagnostic modes have no platform-thread requirement.
fn run_on_emulation_thread(
    task: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Result<(), Box<dyn Error>> {
    let worker = thread::Builder::new()
        .name("gb-emulation".to_string())
        .stack_size(EMULATION_THREAD_STACK_SIZE)
        .spawn(task)?;

    match worker.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(std::io::Error::other(error).into()),
        Err(_) => Err(std::io::Error::other("emulation worker panicked").into()),
    }
}

fn dump_framebuffer(framebuffer: &[u32], frame_count: usize, mode: FrameDumpMode) {
    println!(
        "frame={frame_count} size={}x{} hash=fnv1a64:{:016x}",
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
        framebuffer_hash(framebuffer)
    );

    match mode {
        FrameDumpMode::Hash => {}
        FrameDumpMode::Blocks { block_size } => print_block_dump(framebuffer, block_size),
        FrameDumpMode::Pixels => print_pixel_dump(framebuffer),
        FrameDumpMode::All => {
            print_block_dump(framebuffer, 4);
            print_pixel_dump(framebuffer);
        }
    }
}

fn framebuffer_hash(framebuffer: &[u32]) -> u64 {
    let mut hash = FRAME_HASH_OFFSET;

    for pixel in framebuffer {
        for byte in pixel.to_be_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FRAME_HASH_PRIME);
        }
    }

    hash
}

fn print_block_dump(framebuffer: &[u32], block_size: usize) {
    println!("blocks={block_size}");

    for y in (0..SCREEN_HEIGHT).step_by(block_size) {
        let mut row = String::new();

        for x in (0..SCREEN_WIDTH).step_by(block_size) {
            row.push(shade_digit(majority_shade(framebuffer, x, y, block_size)));
        }

        println!("{row}");
    }
}

fn majority_shade(framebuffer: &[u32], left: usize, top: usize, block_size: usize) -> u8 {
    let mut counts = [0_usize; 4];
    let bottom = (top + block_size).min(SCREEN_HEIGHT);
    let right = (left + block_size).min(SCREEN_WIDTH);

    for y in top..bottom {
        for x in left..right {
            counts[usize::from(shade_index(framebuffer[y * SCREEN_WIDTH + x]))] += 1;
        }
    }

    let (shade, _) = counts
        .iter()
        .enumerate()
        .max_by_key(|(shade, count)| (**count, std::cmp::Reverse(*shade)))
        .expect("there are always four DMG shade buckets");

    u8::try_from(shade).expect("shade bucket fits in u8")
}

fn print_pixel_dump(framebuffer: &[u32]) {
    println!("pixels={SCREEN_WIDTH}x{SCREEN_HEIGHT}");

    for y in 0..SCREEN_HEIGHT {
        let mut row = String::with_capacity(SCREEN_WIDTH);

        for x in 0..SCREEN_WIDTH {
            row.push(shade_digit(shade_index(framebuffer[y * SCREEN_WIDTH + x])));
        }

        println!("{row}");
    }
}

fn shade_digit(shade: u8) -> char {
    char::from(b'0' + shade)
}

fn shade_index(pixel: u32) -> u8 {
    match pixel {
        0xFFFF_FFFF => 0,
        0xFFAA_AAAA => 1,
        0xFF55_5555 => 2,
        0xFF00_0000 => 3,
        _ => {
            let [_alpha, red, green, blue] = pixel.to_be_bytes();
            let luma = (u16::from(red) + u16::from(green) + u16::from(blue)) / 3;
            match luma {
                213..=255 => 0,
                128..=212 => 1,
                43..=127 => 2,
                _ => 3,
            }
        }
    }
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
    Emulator(Box<GameBoy>, PathBuf),
    Demo(Box<DemoFrame>),
    Harness(Box<Harness>),
}

impl DisplaySource {
    fn dimensions(&self) -> (usize, usize, u32) {
        match self {
            Self::Emulator(..) | Self::Demo(_) => (SCREEN_WIDTH, SCREEN_HEIGHT, SCALE),
            Self::Harness(_) => (HARNESS_WIDTH, HARNESS_HEIGHT, 1),
        }
    }

    fn next_frame(&mut self) -> Option<&[u32]> {
        match self {
            Self::Emulator(game_boy, _) => {
                if let Err(error) = game_boy.run_until_frame() {
                    eprintln!("{error}");
                    return None;
                }

                Some(game_boy.framebuffer())
            }
            Self::Demo(demo) => Some(demo.next_frame()),
            Self::Harness(harness) => Some(harness.next_frame()),
        }
    }

    fn set_button(&mut self, button: Button, pressed: bool) {
        if let Self::Emulator(game_boy, _) = self {
            game_boy.set_button(button, pressed);
        }
    }

    fn take_audio_samples(&mut self) -> Vec<StereoSample> {
        match self {
            Self::Emulator(game_boy, _) => game_boy.take_audio_samples(),
            Self::Harness(harness) => harness
                .running
                .as_mut()
                .map_or_else(Vec::new, |running| running.game_boy.take_audio_samples()),
            Self::Demo(_) => Vec::new(),
        }
    }

    fn handle_key(&mut self, key: PhysicalKey, state: ElementState) {
        if let Self::Harness(harness) = self {
            harness.handle_key(key, state);
        }
    }

    fn save_battery_ram(&self) {
        let (game_boy, save_path) = match self {
            Self::Emulator(game_boy, save_path) => (game_boy.as_ref(), save_path),
            Self::Harness(harness) => {
                let Some(running) = harness.running.as_ref() else {
                    return;
                };
                (&running.game_boy, &running.save_path)
            }
            Self::Demo(_) => return,
        };

        if let Some(ram) = game_boy.save_ram() {
            if let Err(error) = fs::write(save_path, ram) {
                eprintln!("failed to save RAM to {}: {error}", save_path.display());
            }
        }

        if let Some(rtc) = game_boy.save_rtc() {
            let rtc_path = rtc_path_for_save(save_path);
            if let Err(error) = fs::write(&rtc_path, rtc) {
                eprintln!("failed to save RTC to {}: {error}", rtc_path.display());
            }
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

#[derive(Debug)]
struct Harness {
    rom_root: PathBuf,
    tree: Vec<TreeEntry>,
    selected_visible: usize,
    running: Option<RunningRom>,
    paused: bool,
    frame: Box<[u32]>,
    last_error: Option<String>,
}

#[derive(Debug, Clone)]
struct TreeEntry {
    path: PathBuf,
    label: String,
    name: String,
    depth: usize,
    kind: TreeEntryKind,
}

#[derive(Debug, Clone)]
enum TreeEntryKind {
    Directory { expanded: bool, rom_count: usize },
    Rom { golden_path: Option<PathBuf> },
}

#[derive(Debug)]
struct RunningRom {
    path: PathBuf,
    game_boy: GameBoy,
    save_path: PathBuf,
    golden: Option<GoldenImage>,
    frames: u64,
}

#[derive(Debug)]
struct GoldenImage {
    path: PathBuf,
    pixels: Vec<u32>,
    width: usize,
    height: usize,
}

impl Harness {
    fn discover(rom_root: PathBuf) -> Result<Self, Box<dyn Error>> {
        let tree = discover_tree(&rom_root)?;

        Ok(Self {
            rom_root,
            tree,
            selected_visible: 0,
            running: None,
            paused: false,
            frame: vec![0xFF18_1A1F; HARNESS_WIDTH * HARNESS_HEIGHT].into_boxed_slice(),
            last_error: None,
        })
    }

    fn next_frame(&mut self) -> &[u32] {
        if !self.paused {
            if let Some(running) = self.running.as_mut() {
                match running.game_boy.run_until_frame() {
                    Ok(()) => running.frames += 1,
                    Err(error) => {
                        self.last_error = Some(error.to_string());
                        self.paused = true;
                    }
                }
            }
        }

        self.render();
        &self.frame
    }

    fn handle_key(&mut self, key: PhysicalKey, state: ElementState) {
        if state != ElementState::Pressed {
            return;
        }

        match key {
            PhysicalKey::Code(KeyCode::ArrowDown) => self.move_selection(1),
            PhysicalKey::Code(KeyCode::ArrowUp) => self.move_selection(-1),
            PhysicalKey::Code(KeyCode::PageDown) => self.move_selection(12),
            PhysicalKey::Code(KeyCode::PageUp) => self.move_selection(-12),
            PhysicalKey::Code(KeyCode::Home) => self.selected_visible = 0,
            PhysicalKey::Code(KeyCode::End) => {
                self.selected_visible = self.visible_indices().len().saturating_sub(1);
            }
            PhysicalKey::Code(KeyCode::ArrowRight) => self.expand_selected(),
            PhysicalKey::Code(KeyCode::ArrowLeft) => self.collapse_selected(),
            PhysicalKey::Code(KeyCode::Enter | KeyCode::KeyR) => self.load_selected(),
            PhysicalKey::Code(KeyCode::Space) => self.paused = !self.paused,
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let visible_len = self.visible_indices().len();
        if visible_len == 0 {
            return;
        }

        let selected = self.selected_visible.saturating_add_signed(delta);
        self.selected_visible = selected.min(visible_len - 1);
    }

    fn load_selected(&mut self) {
        let Some(index) = self.selected_tree_index() else {
            self.last_error = Some("no entries found under test-roms/".to_string());
            return;
        };
        if let TreeEntryKind::Directory { expanded, .. } = &mut self.tree[index].kind {
            *expanded = !*expanded;
            return;
        }
        let entry = self.tree[index].clone();

        match load_running_rom(&entry) {
            Ok(running) => {
                self.running = Some(running);
                self.paused = false;
                self.last_error = None;
            }
            Err(error) => {
                self.running = None;
                self.paused = true;
                self.last_error = Some(error.to_string());
            }
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        let mut visible = Vec::new();
        let mut collapsed_depth: Option<usize> = None;

        for (index, entry) in self.tree.iter().enumerate() {
            if let Some(depth) = collapsed_depth {
                if entry.depth > depth {
                    continue;
                }
                collapsed_depth = None;
            }

            visible.push(index);
            if matches!(
                entry.kind,
                TreeEntryKind::Directory {
                    expanded: false,
                    ..
                }
            ) {
                collapsed_depth = Some(entry.depth);
            }
        }

        visible
    }

    fn selected_tree_index(&self) -> Option<usize> {
        self.visible_indices().get(self.selected_visible).copied()
    }

    fn expand_selected(&mut self) {
        let Some(index) = self.selected_tree_index() else {
            return;
        };
        if let TreeEntryKind::Directory { expanded, .. } = &mut self.tree[index].kind {
            *expanded = true;
        }
    }

    fn collapse_selected(&mut self) {
        let Some(index) = self.selected_tree_index() else {
            return;
        };
        if let TreeEntryKind::Directory { expanded, .. } = &mut self.tree[index].kind {
            if *expanded {
                *expanded = false;
                return;
            }
        }

        let depth = self.tree[index].depth;
        if depth == 0 {
            return;
        }
        if let Some(parent_index) = self.tree[..index]
            .iter()
            .rposition(|entry| entry.depth + 1 == depth)
        {
            let visible = self.visible_indices();
            if let Some(position) = visible
                .iter()
                .position(|entry_index| *entry_index == parent_index)
            {
                self.selected_visible = position;
            }
        }
    }

    fn render(&mut self) {
        self.frame.fill(0xFF18_1A1F);
        draw_rect(
            &mut self.frame,
            0,
            0,
            TREE_WIDTH,
            TOP_PANEL_HEIGHT,
            0xFF23_252C,
        );
        draw_rect(
            &mut self.frame,
            TREE_WIDTH,
            0,
            2,
            TOP_PANEL_HEIGHT,
            0xFF3D_414B,
        );
        draw_rect(
            &mut self.frame,
            TREE_WIDTH + 2,
            0,
            STATUS_LEFT - TREE_WIDTH - 2,
            TOP_PANEL_HEIGHT,
            0xFF0F_1117,
        );
        draw_rect(
            &mut self.frame,
            STATUS_LEFT,
            0,
            2,
            TOP_PANEL_HEIGHT,
            0xFF3D_414B,
        );
        draw_rect(
            &mut self.frame,
            STATUS_LEFT + 2,
            0,
            HARNESS_WIDTH - STATUS_LEFT - 2,
            TOP_PANEL_HEIGHT,
            0xFF15_171E,
        );
        draw_rect(
            &mut self.frame,
            0,
            TOP_PANEL_HEIGHT,
            HARNESS_WIDTH,
            2,
            0xFF3D_414B,
        );
        draw_rect(
            &mut self.frame,
            0,
            BOTTOM_PANEL_TOP,
            HARNESS_WIDTH / 2,
            HARNESS_HEIGHT - BOTTOM_PANEL_TOP,
            0xFF1D_2028,
        );
        draw_rect(
            &mut self.frame,
            HARNESS_WIDTH / 2,
            BOTTOM_PANEL_TOP,
            HARNESS_WIDTH / 2,
            HARNESS_HEIGHT - BOTTOM_PANEL_TOP,
            0xFF1A_1D24,
        );
        draw_rect(
            &mut self.frame,
            HARNESS_WIDTH / 2,
            BOTTOM_PANEL_TOP,
            2,
            HARNESS_HEIGHT - BOTTOM_PANEL_TOP,
            0xFF3D_414B,
        );

        draw_text(
            &mut self.frame,
            16,
            14,
            "rustboy manual harness",
            0xFFE8_ECF2,
        );
        draw_text(
            &mut self.frame,
            16,
            34,
            &format!("root: {}", self.rom_root.display()),
            0xFFAC_B4C0,
        );
        self.render_rom_list();
        self.render_game_view();
        self.render_side_panel();
        self.render_bottom_panels();
    }

    fn render_rom_list(&mut self) {
        let visible = self.visible_indices();
        if visible.is_empty() {
            draw_text(
                &mut self.frame,
                16,
                72,
                "no .gb/.gbc files found",
                0xFFFF_C56D,
            );
            return;
        }

        let list_top = 64;
        let list_bottom = TOP_PANEL_HEIGHT - 76;
        let visible_rows = (list_bottom - list_top) / 16;
        self.selected_visible = self.selected_visible.min(visible.len() - 1);
        let first = self.selected_visible.saturating_sub(visible_rows / 2);
        let last = (first + visible_rows).min(visible.len());

        for (row, visible_index) in (first..last).enumerate() {
            let index = visible[visible_index];
            let y = list_top + row * 16;
            let entry = &self.tree[index];
            if visible_index == self.selected_visible {
                draw_rect(&mut self.frame, 8, y - 2, TREE_WIDTH - 16, 14, 0xFF3B_4E68);
            }
            let marker = match &entry.kind {
                TreeEntryKind::Directory { expanded, .. } => {
                    if *expanded {
                        "-"
                    } else {
                        "+"
                    }
                }
                TreeEntryKind::Rom { golden_path } => {
                    if golden_path.is_some() {
                        "*"
                    } else {
                        " "
                    }
                }
            };
            let indent = " ".repeat(entry.depth * 2);
            let suffix = match &entry.kind {
                TreeEntryKind::Directory { rom_count, .. } => format!(" ({rom_count})"),
                TreeEntryKind::Rom { .. } => String::new(),
            };
            let label = truncate_start(
                &format!("{indent}{marker} {}{suffix}", entry.name),
                (TREE_WIDTH - 32) / 8,
            );
            let color = if visible_index == self.selected_visible {
                0xFFFF_FFFF
            } else if matches!(entry.kind, TreeEntryKind::Directory { .. }) {
                0xFFFF_C56D
            } else {
                0xFFCD_D3DD
            };
            draw_text(&mut self.frame, 16, y, &label, color);
        }

        draw_text(
            &mut self.frame,
            16,
            TOP_PANEL_HEIGHT - 50,
            "up/down select  left/right fold",
            0xFF8F_98A8,
        );
        draw_text(
            &mut self.frame,
            16,
            TOP_PANEL_HEIGHT - 32,
            "enter run  space pause  r reset",
            0xFF8F_98A8,
        );
    }

    fn render_game_view(&mut self) {
        draw_text(
            &mut self.frame,
            TREE_WIDTH + 26,
            18,
            "emulator",
            0xFFE8_ECF2,
        );
        let left = TREE_WIDTH + 16;
        let top = 44;
        draw_rect(
            &mut self.frame,
            left - 4,
            top - 4,
            SCREEN_WIDTH * GB_VIEW_SCALE + 8,
            SCREEN_HEIGHT * GB_VIEW_SCALE + 8,
            0xFF28_2C35,
        );

        if let Some(running) = self.running.as_ref() {
            draw_scaled_framebuffer(
                &mut self.frame,
                running.game_boy.framebuffer(),
                left,
                top,
                GB_VIEW_SCALE,
            );
        } else {
            draw_rect(
                &mut self.frame,
                left,
                top,
                SCREEN_WIDTH * GB_VIEW_SCALE,
                SCREEN_HEIGHT * GB_VIEW_SCALE,
                0xFFCF_D7DF,
            );
            draw_text(
                &mut self.frame,
                left + 88,
                top + 204,
                "select a ROM and press enter",
                0xFF26_2B33,
            );
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "The harness side panel intentionally keeps related debug drawing in one scan-friendly block."
    )]
    fn render_side_panel(&mut self) {
        let x = STATUS_LEFT + 20;
        draw_text(&mut self.frame, x, 18, "status", 0xFFE8_ECF2);

        if let Some(running) = self.running.as_ref() {
            let registers = running.game_boy.registers();
            let serial = running.game_boy.serial_output();
            let serial_text = String::from_utf8_lossy(serial);
            draw_text(
                &mut self.frame,
                x,
                48,
                &format!("frames {}", running.frames),
                0xFFAC_B4C0,
            );
            draw_text(
                &mut self.frame,
                x,
                66,
                if self.paused { "paused" } else { "running" },
                if self.paused {
                    0xFFFF_C56D
                } else {
                    0xFFA6_E3A1
                },
            );
            draw_text(
                &mut self.frame,
                x,
                96,
                &format!("{registers:?}"),
                0xFFE8_ECF2,
            );
            draw_text(
                &mut self.frame,
                x,
                122,
                &format!(
                    "LY {:02X} IF {:02X} IE {:02X}",
                    running.game_boy.debug_read8(0xFF44),
                    running.game_boy.debug_read8(0xFF0F),
                    running.game_boy.debug_read8(0xFFFF)
                ),
                0xFFCD_D3DD,
            );
            draw_text(
                &mut self.frame,
                x,
                140,
                &format!(
                    "LCDC {:02X} STAT {:02X}",
                    running.game_boy.debug_read8(0xFF40),
                    running.game_boy.debug_read8(0xFF41)
                ),
                0xFFCD_D3DD,
            );
            draw_text(
                &mut self.frame,
                x,
                170,
                &format!("serial bytes {}", serial.len()),
                0xFFE8_ECF2,
            );
            for (line, text) in serial_text.lines().rev().take(8).enumerate() {
                draw_text(
                    &mut self.frame,
                    x,
                    190 + line * 16,
                    &truncate_middle(text, 27),
                    0xFFAC_B4C0,
                );
            }

            draw_text(&mut self.frame, x, 340, "golden", 0xFFE8_ECF2);
            if let Some(golden) = running.golden.as_ref() {
                draw_text(
                    &mut self.frame,
                    x,
                    362,
                    &truncate_middle(&golden.path.display().to_string(), 27),
                    0xFFAC_B4C0,
                );
                draw_scaled_image_fit(&mut self.frame, golden, x, 392, 220, 180);
            } else {
                draw_text(&mut self.frame, x, 362, "none found", 0xFF8F_98A8);
            }
        } else {
            draw_text(&mut self.frame, x, 48, "no ROM running", 0xFF8F_98A8);
        }

        if let Some(error) = self.last_error.as_ref() {
            draw_text(
                &mut self.frame,
                x,
                580,
                &truncate_middle(error, 27),
                0xFFFF_8A8A,
            );
        }
    }

    fn render_bottom_panels(&mut self) {
        self.render_selected_rom_panel();
        self.render_running_rom_panel();
    }

    fn render_selected_rom_panel(&mut self) {
        let x = 20;
        draw_text(
            &mut self.frame,
            x,
            BOTTOM_PANEL_TOP + 18,
            "selected ROM",
            0xFFE8_ECF2,
        );
        let Some(index) = self.selected_tree_index() else {
            draw_text(
                &mut self.frame,
                x,
                BOTTOM_PANEL_TOP + 46,
                "no ROMs found",
                0xFF8F_98A8,
            );
            return;
        };

        let visible_len = self.visible_indices().len();
        let entry = &self.tree[index];
        draw_text(
            &mut self.frame,
            x,
            BOTTOM_PANEL_TOP + 46,
            &format!(
                "row {}/{}  depth {}",
                self.selected_visible + 1,
                visible_len,
                entry.depth
            ),
            0xFFAC_B4C0,
        );
        draw_text(
            &mut self.frame,
            x,
            BOTTOM_PANEL_TOP + 68,
            &truncate_middle(&entry.label, 72),
            0xFFE8_ECF2,
        );
        match &entry.kind {
            TreeEntryKind::Directory {
                expanded,
                rom_count,
            } => draw_text(
                &mut self.frame,
                x,
                BOTTOM_PANEL_TOP + 90,
                &format!(
                    "folder: {}  ROMs: {}",
                    if *expanded { "expanded" } else { "collapsed" },
                    rom_count
                ),
                0xFFFF_C56D,
            ),
            TreeEntryKind::Rom { golden_path } => {
                draw_text(
                    &mut self.frame,
                    x,
                    BOTTOM_PANEL_TOP + 90,
                    if golden_path.is_some() {
                        "ROM file  golden available"
                    } else {
                        "ROM file  no golden"
                    },
                    if golden_path.is_some() {
                        0xFFA6_E3A1
                    } else {
                        0xFF8F_98A8
                    },
                );
            }
        }
    }

    fn render_running_rom_panel(&mut self) {
        let x = HARNESS_WIDTH / 2 + 20;
        draw_text(
            &mut self.frame,
            x,
            BOTTOM_PANEL_TOP + 18,
            "running ROM",
            0xFFE8_ECF2,
        );

        let Some(running) = self.running.as_ref() else {
            draw_text(
                &mut self.frame,
                x,
                BOTTOM_PANEL_TOP + 46,
                "nothing running",
                0xFF8F_98A8,
            );
            return;
        };

        draw_text(
            &mut self.frame,
            x,
            BOTTOM_PANEL_TOP + 46,
            &truncate_middle(&running.path.display().to_string(), 72),
            0xFFE8_ECF2,
        );
        draw_text(
            &mut self.frame,
            x,
            BOTTOM_PANEL_TOP + 68,
            &format!(
                "frames {}  state {}",
                running.frames,
                if self.paused { "paused" } else { "running" }
            ),
            0xFFAC_B4C0,
        );
        draw_text(
            &mut self.frame,
            x,
            BOTTOM_PANEL_TOP + 90,
            &format!(
                "save: {}",
                truncate_middle(&running.save_path.display().to_string(), 64)
            ),
            0xFF8F_98A8,
        );
    }
}

fn discover_tree(root: &Path) -> Result<Vec<TreeEntry>, Box<dyn Error>> {
    let mut tree = Vec::new();
    discover_tree_in(root, root, 0, &mut tree)?;
    Ok(tree)
}

fn discover_tree_in(
    root: &Path,
    directory: &Path,
    depth: usize,
    tree: &mut Vec<TreeEntry>,
) -> Result<usize, Box<dyn Error>> {
    if !directory.exists() {
        return Ok(0);
    }

    let mut directories = Vec::new();
    let mut roms = Vec::new();

    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            directories.push(path);
        } else if is_rom_path(&path) {
            roms.push(path);
        }
    }

    directories.sort_by(|left, right| natural_path_cmp(&path_name(left), &path_name(right)));
    roms.sort_by(|left, right| natural_path_cmp(&path_name(left), &path_name(right)));

    let start_len = tree.len();
    if directory != root {
        let label = directory
            .strip_prefix(root)
            .unwrap_or(directory)
            .display()
            .to_string();
        tree.push(TreeEntry {
            path: directory.to_path_buf(),
            name: path_name(directory),
            label,
            depth,
            kind: TreeEntryKind::Directory {
                expanded: depth == 1,
                rom_count: 0,
            },
        });
    }

    let child_depth = if directory == root { depth } else { depth + 1 };
    let mut rom_count = 0;

    for directory in directories {
        rom_count += discover_tree_in(root, &directory, child_depth, tree)?;
    }

    for path in roms {
        let label = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let golden_path = find_golden_for_rom(&path);
        let name = path_name(&path);
        tree.push(TreeEntry {
            path,
            name,
            label,
            depth: child_depth,
            kind: TreeEntryKind::Rom { golden_path },
        });
        rom_count += 1;
    }

    if directory != root {
        if let TreeEntryKind::Directory {
            rom_count: stored, ..
        } = &mut tree[start_len].kind
        {
            *stored = rom_count;
        }
    }

    Ok(rom_count)
}

fn is_rom_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "gb" | "gbc"))
}

fn find_golden_for_rom(rom_path: &Path) -> Option<PathBuf> {
    let directory = rom_path.parent()?;
    let stem = rom_path.file_stem()?.to_string_lossy().to_ascii_lowercase();
    let mut candidates = fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        })
        .filter(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.to_ascii_lowercase().starts_with(&stem))
        })
        .collect::<Vec<_>>();

    candidates.sort_by_key(|path| {
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        (
            !name.contains("dmg"),
            name.contains("cgb") && !name.contains("dmg"),
            name.len(),
        )
    });
    candidates.into_iter().next()
}

fn path_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn load_running_rom(entry: &TreeEntry) -> Result<RunningRom, Box<dyn Error>> {
    let rom = fs::read(&entry.path)?;
    let mut game_boy = GameBoy::from_rom(rom)?;
    let save_path = save_path_for_rom(&entry.path);
    load_save_if_present(&mut game_boy, &save_path)?;
    let golden = entry
        .kind
        .golden_path()
        .and_then(|path| load_golden(path).ok());

    Ok(RunningRom {
        path: entry.path.clone(),
        game_boy,
        save_path,
        golden,
        frames: 0,
    })
}

impl TreeEntryKind {
    fn golden_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Rom { golden_path } => golden_path.as_ref(),
            Self::Directory { .. } => None,
        }
    }
}

fn load_golden(path: &Path) -> Result<GoldenImage, Box<dyn Error>> {
    let image = image::ImageReader::open(path)?.decode()?.to_rgba8();
    let (width, height) = image.dimensions();
    let pixels = image
        .pixels()
        .map(|pixel| {
            let [red, green, blue, alpha] = pixel.0;
            u32::from_be_bytes([alpha, red, green, blue])
        })
        .collect();

    Ok(GoldenImage {
        path: path.to_path_buf(),
        pixels,
        width: usize::try_from(width)?,
        height: usize::try_from(height)?,
    })
}

fn natural_path_cmp(left: &str, right: &str) -> Ordering {
    left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
}

fn truncate_middle(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let left_len = (max_chars - 3) / 2;
    let right_len = max_chars - 3 - left_len;
    let left = text.chars().take(left_len).collect::<String>();
    let right = text
        .chars()
        .rev()
        .take(right_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();

    format!("{left}...{right}")
}

fn truncate_start(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let tail_len = max_chars - 3;
    let tail = text
        .chars()
        .rev()
        .take(tail_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();

    format!("...{tail}")
}

fn draw_scaled_framebuffer(
    target: &mut [u32],
    source: &[u32],
    left: usize,
    top: usize,
    scale: usize,
) {
    for y in 0..SCREEN_HEIGHT {
        for x in 0..SCREEN_WIDTH {
            let color = source[y * SCREEN_WIDTH + x];
            draw_rect(
                target,
                left + x * scale,
                top + y * scale,
                scale,
                scale,
                color,
            );
        }
    }
}

fn draw_scaled_image_fit(
    target: &mut [u32],
    image: &GoldenImage,
    left: usize,
    top: usize,
    max_width: usize,
    max_height: usize,
) {
    if image.width == 0 || image.height == 0 {
        return;
    }

    let scale = (max_width / image.width)
        .min(max_height / image.height)
        .max(1);
    for y in 0..image.height {
        for x in 0..image.width {
            let color = image.pixels[y * image.width + x];
            draw_rect(
                target,
                left + x * scale,
                top + y * scale,
                scale,
                scale,
                color,
            );
        }
    }
}

fn draw_rect(target: &mut [u32], left: usize, top: usize, width: usize, height: usize, color: u32) {
    let right = (left + width).min(HARNESS_WIDTH);
    let bottom = (top + height).min(HARNESS_HEIGHT);

    for y in top.min(HARNESS_HEIGHT)..bottom {
        for x in left.min(HARNESS_WIDTH)..right {
            target[y * HARNESS_WIDTH + x] = color;
        }
    }
}

fn draw_text(target: &mut [u32], left: usize, top: usize, text: &str, color: u32) {
    let mut x = left;
    for character in text.chars() {
        draw_char(target, x, top, character, color);
        x += 8;
        if x >= HARNESS_WIDTH.saturating_sub(8) {
            break;
        }
    }
}

fn draw_char(target: &mut [u32], left: usize, top: usize, character: char, color: u32) {
    let Some(glyph) = font8x8::BASIC_FONTS.get(character) else {
        return;
    };

    for (y, row) in glyph.iter().enumerate() {
        for x in 0..8 {
            if row & (1 << x) != 0 {
                let pixel_x = left + x;
                let pixel_y = top + y;
                if pixel_x < HARNESS_WIDTH && pixel_y < HARNESS_HEIGHT {
                    target[pixel_y * HARNESS_WIDTH + pixel_x] = color;
                }
            }
        }
    }
}

struct AudioSink {
    queue: Arc<Mutex<VecDeque<StereoSample>>>,
    _stream: cpal::Stream,
}

impl AudioSink {
    fn new() -> Result<Self, Box<dyn Error>> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no default audio output device")?;
        let supported_config = preferred_audio_config(&device)?;
        let sample_format = supported_config.sample_format();
        let config = supported_config.config();
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_audio_stream::<f32>(&device, &config, queue.clone())?,
            cpal::SampleFormat::I16 => build_audio_stream::<i16>(&device, &config, queue.clone())?,
            cpal::SampleFormat::U16 => build_audio_stream::<u16>(&device, &config, queue.clone())?,
            format => return Err(format!("unsupported audio sample format: {format:?}").into()),
        };

        stream.play()?;

        Ok(Self {
            queue,
            _stream: stream,
        })
    }

    fn push_samples(&self, samples: Vec<StereoSample>) {
        if samples.is_empty() {
            return;
        }

        let Ok(mut queue) = self.queue.lock() else {
            return;
        };
        queue.extend(samples);
        let max_buffered_samples =
            usize::try_from(AUDIO_SAMPLE_RATE / 2).expect("sample rate fits in usize");
        while queue.len() > max_buffered_samples {
            queue.pop_front();
        }
    }
}

fn preferred_audio_config(
    device: &cpal::Device,
) -> Result<cpal::SupportedStreamConfig, Box<dyn Error>> {
    let supported_configs = device.supported_output_configs()?;

    for config in supported_configs {
        if config.channels() < 2 {
            continue;
        }
        if config.min_sample_rate().0 <= AUDIO_SAMPLE_RATE
            && config.max_sample_rate().0 >= AUDIO_SAMPLE_RATE
        {
            return Ok(config.with_sample_rate(cpal::SampleRate(AUDIO_SAMPLE_RATE)));
        }
    }

    Ok(device.default_output_config()?)
}

fn build_audio_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Arc<Mutex<VecDeque<StereoSample>>>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = usize::from(config.channels);
    device.build_output_stream(
        config,
        move |data: &mut [T], _| fill_audio_output(data, channels, &queue),
        |error| eprintln!("audio stream error: {error}"),
        None,
    )
}

fn fill_audio_output<T>(data: &mut [T], channels: usize, queue: &Arc<Mutex<VecDeque<StereoSample>>>)
where
    T: cpal::Sample + cpal::FromSample<f32>,
{
    let mut guard = queue.lock().ok();

    for frame in data.chunks_mut(channels) {
        let sample = guard.as_mut().and_then(|queue| queue.pop_front());
        let (left, right) = sample.map_or((0.0, 0.0), |sample| {
            (
                f32::from(sample.left) / f32::from(i16::MAX),
                f32::from(sample.right) / f32::from(i16::MAX),
            )
        });

        if let Some(channel) = frame.get_mut(0) {
            *channel = T::from_sample(left);
        }
        if let Some(channel) = frame.get_mut(1) {
            *channel = T::from_sample(right);
        }
        for channel in frame.iter_mut().skip(2) {
            *channel = T::from_sample(0.0);
        }
    }
}

struct DesktopApp {
    source: DisplaySource,
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
    frame_pacer: FramePacer,
    audio: Option<AudioSink>,
}

impl DesktopApp {
    fn new(source: DisplaySource) -> Self {
        Self {
            source,
            window: None,
            pixels: None,
            frame_pacer: FramePacer::new(),
            audio: None,
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let (width, height, scale) = self.source.dimensions();
        let width = u32::try_from(width).expect("window width fits in u32");
        let height = u32::try_from(height).expect("window height fits in u32");
        let size = LogicalSize::new(f64::from(width * scale), f64::from(height * scale));
        let attributes = WindowAttributes::default()
            .with_title("rustboy")
            .with_inner_size(size)
            .with_min_inner_size(size);

        let window = event_loop
            .create_window(attributes)
            .expect("desktop window should be created");
        let window: &'static Window = Box::leak(Box::new(window));
        let surface_texture = SurfaceTexture::new(width * scale, height * scale, window);
        let pixels = Pixels::new(width, height, surface_texture)
            .expect("pixel framebuffer should be created");

        self.pixels = Some(pixels);
        self.window = Some(window);
        self.audio = AudioSink::new()
            .map_err(|error| eprintln!("audio disabled: {error}"))
            .ok();
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
        let audio_samples = self.source.take_audio_samples();
        if let Some(audio) = self.audio.as_ref() {
            audio.push_samples(audio_samples);
        }

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
            WindowEvent::CloseRequested => {
                self.source.save_battery_ram();
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            WindowEvent::KeyboardInput { event, .. } => {
                self.source.handle_key(event.physical_key, event.state);
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

fn save_path_for_rom(rom_path: &Path) -> PathBuf {
    rom_path.with_extension("sav")
}

fn rtc_path_for_save(save_path: &Path) -> PathBuf {
    save_path.with_extension("rtc")
}

fn load_save_if_present(game_boy: &mut GameBoy, save_path: &Path) -> Result<(), Box<dyn Error>> {
    if game_boy.save_ram().is_some() && save_path.exists() {
        let save = fs::read(save_path)?;
        game_boy.load_save_ram(&save)?;
    }

    let rtc_path = rtc_path_for_save(save_path);
    if game_boy.save_rtc().is_some() && rtc_path.exists() {
        let rtc = fs::read(rtc_path)?;
        game_boy.load_save_rtc(&rtc)?;
    }

    Ok(())
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
    fn rtc_sidecar_uses_the_save_file_stem() {
        assert_eq!(
            rtc_path_for_save(Path::new("games/tetris.sav")),
            PathBuf::from("games/tetris.rtc")
        );
    }

    fn minimal_cartridge_with_entry(entry: &[u8]) -> Cartridge {
        let mut rom = vec![0; 0x8000];
        rom[0x0100..0x0100 + entry.len()].copy_from_slice(entry);
        rom[0x0147] = 0x00;
        rom[0x0148] = 0x00;
        rom[0x0149] = 0x00;

        let mut checksum = 0_u8;
        for byte in &rom[0x0134..=0x014C] {
            checksum = checksum.wrapping_sub(*byte).wrapping_sub(1);
        }
        rom[0x014D] = checksum;

        Cartridge::from_bytes(rom).expect("generated ROM should be a valid ROM-only cartridge")
    }

    #[test]
    fn headless_serial_runner_steps_on_dedicated_emulation_stack() {
        run_serial_output(minimal_cartridge_with_entry(&[0x00]), None, 1)
            .expect("a NOP should execute on the dedicated emulation stack");
    }

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

    #[test]
    fn options_parse_frame_dump_request() {
        let options = Options::parse([
            "rom.gb".to_string(),
            "--frames".to_string(),
            "3".to_string(),
            "--dump-frame".to_string(),
            "blocks:4".to_string(),
        ])
        .expect("frame dump options should parse");

        assert_eq!(options.rom_path.as_deref(), Some("rom.gb"));
        assert_eq!(options.frames, Some(3));
        assert_eq!(
            options.dump_frame,
            FrameDumpMode::Blocks { block_size: 4 },
            "blocks:N should select a stable block digest size"
        );
    }

    #[test]
    fn options_parse_mooneye_request() {
        let options = Options::parse([
            "reg_f.gb".to_string(),
            "--mooneye-steps".to_string(),
            "1000".to_string(),
        ])
        .expect("Mooneye options should parse");

        assert_eq!(options.rom_path.as_deref(), Some("reg_f.gb"));
        assert_eq!(options.mooneye_steps, Some(1000));
    }

    #[test]
    fn options_parse_user_supplied_boot_rom() {
        let options = Options::parse([
            "rom.gb".to_string(),
            "--boot-rom".to_string(),
            "dmg_boot.bin".to_string(),
        ])
        .expect("boot ROM option should parse");

        assert_eq!(options.rom_path.as_deref(), Some("rom.gb"));
        assert_eq!(options.boot_rom_path.as_deref(), Some("dmg_boot.bin"));
    }

    #[test]
    fn options_parse_harness_request() {
        let options =
            Options::parse(["--harness".to_string()]).expect("harness option should parse");

        assert!(options.harness);
        assert_eq!(options.rom_path, None);
    }

    #[test]
    fn harness_rom_filter_accepts_game_boy_extensions_case_insensitively() {
        assert!(is_rom_path(Path::new("test.gb")));
        assert!(is_rom_path(Path::new("test.GBC")));
        assert!(!is_rom_path(Path::new("expected.png")));
    }

    #[test]
    fn visible_indices_hide_collapsed_directory_children() {
        let harness = Harness {
            rom_root: PathBuf::from("test-roms"),
            tree: vec![
                TreeEntry {
                    path: PathBuf::from("suite"),
                    label: "suite".to_string(),
                    name: "suite".to_string(),
                    depth: 0,
                    kind: TreeEntryKind::Directory {
                        expanded: false,
                        rom_count: 1,
                    },
                },
                TreeEntry {
                    path: PathBuf::from("suite/test.gb"),
                    label: "suite/test.gb".to_string(),
                    name: "test.gb".to_string(),
                    depth: 1,
                    kind: TreeEntryKind::Rom { golden_path: None },
                },
                TreeEntry {
                    path: PathBuf::from("other.gb"),
                    label: "other.gb".to_string(),
                    name: "other.gb".to_string(),
                    depth: 0,
                    kind: TreeEntryKind::Rom { golden_path: None },
                },
            ],
            selected_visible: 0,
            running: None,
            paused: false,
            frame: vec![0; HARNESS_WIDTH * HARNESS_HEIGHT].into_boxed_slice(),
            last_error: None,
        };

        assert_eq!(harness.visible_indices(), vec![0, 2]);
    }

    #[test]
    fn truncate_middle_keeps_both_ends_readable() {
        assert_eq!(
            truncate_middle("test-roms/blargg/cpu_instrs/cpu_instrs.gb", 18),
            "test-ro...nstrs.gb"
        );
    }

    #[test]
    fn truncate_start_preserves_distinguishing_rom_filename_tail() {
        assert_eq!(
            truncate_start("gambatte\\cgb_palette\\cgb04c_out7.gbc", 24),
            "...lette\\cgb04c_out7.gbc"
        );
    }

    #[test]
    fn framebuffer_hash_changes_when_pixel_changes() {
        let mut frame = vec![0xFFFF_FFFF; SCREEN_WIDTH * SCREEN_HEIGHT];
        let initial_hash = framebuffer_hash(&frame);

        frame[0] = 0xFF00_0000;

        assert_ne!(
            framebuffer_hash(&frame),
            initial_hash,
            "frame hash should change when framebuffer content changes"
        );
    }

    #[test]
    fn shade_index_maps_dmg_palette_to_greppable_digits() {
        assert_eq!(shade_digit(shade_index(0xFFFF_FFFF)), '0');
        assert_eq!(shade_digit(shade_index(0xFFAA_AAAA)), '1');
        assert_eq!(shade_digit(shade_index(0xFF55_5555)), '2');
        assert_eq!(shade_digit(shade_index(0xFF00_0000)), '3');
    }
}
