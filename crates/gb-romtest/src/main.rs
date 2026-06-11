#![doc = "Headless external test ROM runner for rustboy."]

use gb_core::{
    cartridge::Cartridge,
    cpu::CpuRegisters,
    joypad::Button,
    ppu::{SCREEN_HEIGHT, SCREEN_WIDTH},
    GameBoy,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    env,
    error::Error,
    fmt::{self, Write as _},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime},
};

const DEFAULT_ROM_ROOT: &str = "test-roms";
const DEFAULT_REPORT_OUT: &str = "reports/test-roms";
const DMG_TCYCLES_PER_FRAME: u64 = 456 * 154;
const DMG_CPU_CLOCK_HZ: u64 = 4_194_304;
const FIBONACCI_REGISTERS: (u8, u8, u8, u8, u8, u8) = (3, 5, 8, 13, 21, 34);
const SERIAL_EXCERPT_LIMIT: usize = 1024;
const DEFAULT_CASE_TIMEOUT_SECONDS: u64 = 60;
const RELEASE_RUNNER_ENV: &str = "GB_ROMTEST_RELEASE_RUNNER";

fn main() -> Result<(), Box<dyn Error>> {
    ensure_release_runner()?;

    let options = Options::parse(env::args().skip(1))?;
    let report = run(&options)?;

    println!(
        "wrote report: {} result(s), {} passed, {} failed, {} skipped, {} unsupported",
        report.results.len(),
        report.count(ResultStatus::Passed),
        report.count(ResultStatus::Failed),
        report.count(ResultStatus::Skipped),
        report.count(ResultStatus::Unsupported)
    );

    Ok(())
}

fn ensure_release_runner() -> Result<(), Box<dyn Error>> {
    if !cfg!(debug_assertions) || env::var_os(RELEASE_RUNNER_ENV).is_some() {
        return Ok(());
    }

    println!("building gb-romtest release runner...");
    let build_status = Command::new("cargo")
        .args(["build", "-p", "gb-romtest", "--release"])
        .status()?;
    if !build_status.success() {
        return Err(format!("release build failed with status {build_status}").into());
    }

    let release_runner = release_runner_path();
    println!("running tests with {}", release_runner.display());
    let status = Command::new(&release_runner)
        .args(env::args().skip(1))
        .env(RELEASE_RUNNER_ENV, "1")
        .status()?;

    std::process::exit(status.code().unwrap_or(1));
}

fn release_runner_path() -> PathBuf {
    let target_dir =
        env::var_os("CARGO_TARGET_DIR").map_or_else(|| PathBuf::from("target"), PathBuf::from);
    target_dir
        .join("release")
        .join(format!("gb-romtest{}", env::consts::EXE_SUFFIX))
}

fn run(options: &Options) -> Result<Report, Box<dyn Error>> {
    fs::create_dir_all(&options.out_dir)?;

    let mut cases = discover_cases(&options.rom_root)?;
    cases.retain(|case| options.matches(case));

    let results = run_cases_parallel(cases, options);

    let report = Report::new(options.profile, options.target, results);
    match options.output_format {
        OutputFormat::Json => write_json_report(&report, &options.out_dir)?,
        OutputFormat::Markdown => write_markdown_report(&report, &options.out_dir)?,
        OutputFormat::Both => {
            write_json_report(&report, &options.out_dir)?;
            write_markdown_report(&report, &options.out_dir)?;
        }
    }

    Ok(report)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    profile: Profile,
    suite: Option<String>,
    rom: Option<PathBuf>,
    rom_root: PathBuf,
    out_dir: PathBuf,
    include_audio: bool,
    target: TargetModel,
    output_format: OutputFormat,
    jobs: usize,
    case_timeout: Duration,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, Box<dyn Error>> {
        let mut args = args.into_iter();
        let Some(command) = args.next() else {
            return Err(usage().into());
        };
        if command != "run" {
            return Err(format!("unknown command {command:?}\n{}", usage()).into());
        }

        let mut options = Self {
            profile: Profile::NoAudio,
            suite: None,
            rom: None,
            rom_root: PathBuf::from(DEFAULT_ROM_ROOT),
            out_dir: PathBuf::from(DEFAULT_REPORT_OUT),
            include_audio: false,
            target: TargetModel::Dmg,
            output_format: OutputFormat::Both,
            jobs: default_worker_count(),
            case_timeout: Duration::from_secs(DEFAULT_CASE_TIMEOUT_SECONDS),
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--profile" => {
                    let Some(value) = args.next() else {
                        return Err("--profile requires a value".into());
                    };
                    options.profile = value.parse()?;
                }
                "--suite" => {
                    let Some(value) = args.next() else {
                        return Err("--suite requires a suite name".into());
                    };
                    options.suite = Some(value);
                }
                "--rom" => {
                    let Some(value) = args.next() else {
                        return Err("--rom requires a ROM path".into());
                    };
                    options.rom = Some(PathBuf::from(value));
                }
                "--rom-root" => {
                    let Some(value) = args.next() else {
                        return Err("--rom-root requires a path".into());
                    };
                    options.rom_root = PathBuf::from(value);
                }
                "--out" => {
                    let Some(value) = args.next() else {
                        return Err("--out requires a directory".into());
                    };
                    options.out_dir = PathBuf::from(value);
                }
                "--include-audio" => options.include_audio = true,
                "--target" => {
                    let Some(value) = args.next() else {
                        return Err("--target requires dmg or cgb".into());
                    };
                    options.target = value.parse()?;
                }
                "--format" => {
                    let Some(value) = args.next() else {
                        return Err("--format requires json, markdown, or both".into());
                    };
                    options.output_format = value.parse()?;
                }
                "--jobs" => {
                    let Some(value) = args.next() else {
                        return Err("--jobs requires a worker count".into());
                    };
                    options.jobs = parse_nonzero_usize(&value, "--jobs")?;
                }
                "--case-timeout-seconds" => {
                    let Some(value) = args.next() else {
                        return Err(
                            "--case-timeout-seconds requires a positive second count".into()
                        );
                    };
                    options.case_timeout =
                        Duration::from_secs(parse_nonzero_u64(&value, "--case-timeout-seconds")?);
                }
                _ => return Err(format!("unknown option: {arg}\n{}", usage()).into()),
            }
        }

        if options.profile == Profile::Audio {
            options.include_audio = true;
        }

        Ok(options)
    }

    fn matches(&self, case: &RomCase) -> bool {
        if let Some(rom) = &self.rom {
            return paths_match(&case.path, rom);
        }
        if let Some(suite) = &self.suite {
            return case.suite == *suite;
        }

        match self.profile {
            Profile::Smoke => case.in_smoke_profile(),
            Profile::Dmg | Profile::NoAudio => {
                case.target != TargetRequirement::CgbOnly
                    && case.target != TargetRequirement::SgbOnly
                    && !case.is_audio
                    && !case.is_manual
                    && !case.is_loose_root_rom
            }
            Profile::Exhaustive => true,
            Profile::Audio => case.is_audio,
        }
    }
}

fn usage() -> &'static str {
    "Usage: gb-romtest run [--profile smoke|dmg|exhaustive|audio|no-audio] [--suite NAME] [--rom PATH] [--rom-root DIR] [--out DIR] [--include-audio] [--target dmg|cgb] [--format json|markdown|both] [--jobs N] [--case-timeout-seconds N]"
}

fn default_worker_count() -> usize {
    thread::available_parallelism().map_or(1, usize::from)
}

fn parse_nonzero_usize(value: &str, option: &str) -> Result<usize, Box<dyn Error>> {
    let parsed = value.parse::<usize>()?;
    if parsed == 0 {
        return Err(format!("{option} must be greater than zero").into());
    }
    Ok(parsed)
}

fn parse_nonzero_u64(value: &str, option: &str) -> Result<u64, Box<dyn Error>> {
    let parsed = value.parse::<u64>()?;
    if parsed == 0 {
        return Err(format!("{option} must be greater than zero").into());
    }
    Ok(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Profile {
    Smoke,
    Dmg,
    Exhaustive,
    Audio,
    NoAudio,
}

impl std::str::FromStr for Profile {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "smoke" => Ok(Self::Smoke),
            "dmg" => Ok(Self::Dmg),
            "exhaustive" => Ok(Self::Exhaustive),
            "audio" => Ok(Self::Audio),
            "no-audio" => Ok(Self::NoAudio),
            _ => Err(format!("unknown profile {value:?}")),
        }
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Smoke => "smoke",
            Self::Dmg => "dmg",
            Self::Exhaustive => "exhaustive",
            Self::Audio => "audio",
            Self::NoAudio => "no-audio",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum TargetModel {
    Dmg,
    Cgb,
}

impl std::str::FromStr for TargetModel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "dmg" => Ok(Self::Dmg),
            "cgb" => Ok(Self::Cgb),
            _ => Err(format!("unknown target {value:?}")),
        }
    }
}

impl fmt::Display for TargetModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Dmg => "dmg",
            Self::Cgb => "cgb",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Json,
    Markdown,
    Both,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "json" => Ok(Self::Json),
            "markdown" => Ok(Self::Markdown),
            "both" => Ok(Self::Both),
            _ => Err(format!("unknown output format {value:?}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RomCase {
    suite: String,
    path: PathBuf,
    relative_path: PathBuf,
    rule: ResultRule,
    target: TargetRequirement,
    is_audio: bool,
    is_manual: bool,
    is_loose_root_rom: bool,
    golden_path: Option<PathBuf>,
    run_budget: RunBudget,
    input_schedule: Vec<InputEvent>,
    breakpoint_opcode: u8,
}

impl RomCase {
    fn in_smoke_profile(&self) -> bool {
        let path = normalize_path(&self.relative_path);
        matches!(
            path.as_str(),
            "blargg/cpu_instrs/cpu_instrs.gb"
                | "blargg/cpu_instrs/individual/01-special.gb"
                | "mooneye-test-suite/acceptance/bits/mem_oam.gb"
                | "mooneye-test-suite/acceptance/instr/daa.gb"
                | "gbmicrotest/000-oam_lock.gb"
                | "gbmicrotest/cpu_bus_1.gb"
                | "dmg-acid2/dmg-acid2.gb"
        )
    }
}

fn run_cases_parallel(cases: Vec<RomCase>, options: &Options) -> Vec<RomResult> {
    if cases.is_empty() {
        return Vec::new();
    }

    let total = cases.len();
    let worker_count = options.jobs.min(total).max(1);
    println!(
        "running {total} ROM(s) with {worker_count} worker(s), per-case wall timeout {}s",
        options.case_timeout.as_secs()
    );

    let queue = Arc::new(Mutex::new(
        cases
            .into_iter()
            .enumerate()
            .collect::<VecDeque<(usize, RomCase)>>(),
    ));
    let (result_sender, result_receiver) = mpsc::channel::<IndexedResult>();

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let sender = result_sender.clone();
            scope.spawn(move || loop {
                let next = {
                    let mut queue = queue
                        .lock()
                        .expect("work queue mutex should not be poisoned");
                    queue.pop_front()
                };
                let Some((index, case)) = next else {
                    break;
                };

                let result = run_case(&case, options);
                if sender.send(IndexedResult { index, result }).is_err() {
                    break;
                }
            });
        }
        drop(result_sender);

        let mut indexed_results = Vec::with_capacity(total);
        for completed in 1..=total {
            let indexed = result_receiver
                .recv()
                .expect("workers should send one result per queued case");
            print_progress(completed, total, &indexed.result);
            indexed_results.push(indexed);
        }

        indexed_results.sort_by_key(|indexed| indexed.index);
        indexed_results
            .into_iter()
            .map(|indexed| indexed.result)
            .collect()
    })
}

#[derive(Debug)]
struct IndexedResult {
    index: usize,
    result: RomResult,
}

#[derive(Debug, Clone, Copy)]
struct EvaluationContext {
    profile: Profile,
    target_model: TargetModel,
}

fn print_progress(completed: usize, total: usize, result: &RomResult) {
    println!(
        "[{completed}/{total}] {} {} ({})",
        result.status, result.rom_path, result.result_rule
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ResultRule {
    BreakpointRegisters,
    BreakpointScreenshot,
    SerialText,
    RamSignature,
    Screenshot,
    Audio,
    Unsupported,
}

impl fmt::Display for ResultRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BreakpointRegisters => "breakpoint-registers",
            Self::BreakpointScreenshot => "breakpoint-screenshot",
            Self::SerialText => "serial-text",
            Self::RamSignature => "ram-signature",
            Self::Screenshot => "screenshot",
            Self::Audio => "audio",
            Self::Unsupported => "unsupported",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetRequirement {
    Dmg,
    CgbOnly,
    SgbOnly,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RunBudget {
    max_tcycles: u64,
    stop: StopCondition,
}

impl RunBudget {
    fn frames(frames: u64) -> Self {
        Self {
            max_tcycles: frames * DMG_TCYCLES_PER_FRAME,
            stop: StopCondition::Frames(frames),
        }
    }

    fn seconds(seconds: u64) -> Self {
        Self {
            max_tcycles: seconds * DMG_CPU_CLOCK_HZ,
            stop: StopCondition::TCycles(seconds * DMG_CPU_CLOCK_HZ),
        }
    }

    fn milliseconds(milliseconds: u64) -> Self {
        Self {
            max_tcycles: milliseconds * DMG_CPU_CLOCK_HZ / 1000,
            stop: StopCondition::TCycles(milliseconds * DMG_CPU_CLOCK_HZ / 1000),
        }
    }

    fn breakpoint(seconds: u64) -> Self {
        Self {
            max_tcycles: seconds * DMG_CPU_CLOCK_HZ,
            stop: StopCondition::Breakpoint,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopCondition {
    Breakpoint,
    Frames(u64),
    TCycles(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InputEvent {
    at_tcycles: u64,
    button: Button,
    pressed: bool,
}

#[allow(clippy::too_many_lines)]
fn classify_case(root: &Path, path: PathBuf) -> Option<RomCase> {
    let relative_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
    let normalized = normalize_path(&relative_path);
    let mut parts = normalized.split('/');
    let suite = parts.next()?.to_string();

    if suite == "gambatte" || !is_rom_path(&path) {
        return None;
    }

    let is_audio = is_audio_case(&normalized);
    let is_manual = normalized.contains("/manual-only/")
        || normalized.contains("/fairylake/")
        || normalized.contains("/winpos/")
        || normalized.ends_with("statcount/statcount.gb");
    let is_loose_root_rom = !normalized.contains('/');
    let target = target_requirement(&normalized);
    let golden_path = find_golden_for_case(&path, target);
    let input_schedule = input_schedule_for(&normalized);
    let run_budget = run_budget_for(&suite, &normalized);
    let breakpoint_opcode = breakpoint_opcode_for(&suite);

    let rule = if is_audio {
        ResultRule::Audio
    } else if suite == "gbmicrotest" {
        ResultRule::RamSignature
    } else if suite == "blargg" {
        ResultRule::SerialText
    } else if normalized == "mealybug-tearoom-tests/mbc/mbc3_rtc.gb" {
        ResultRule::BreakpointRegisters
    } else if matches!(
        suite.as_str(),
        "age-test-roms" | "mooneye-test-suite" | "mooneye-test-suite-wilbertpol" | "same-suite"
    ) {
        if golden_path.is_some() || normalized.contains("/manual-only/") {
            ResultRule::BreakpointScreenshot
        } else {
            ResultRule::BreakpointRegisters
        }
    } else if matches!(
        suite.as_str(),
        "cgb-acid-hell" | "cgb-acid2" | "dmg-acid2" | "mealybug-tearoom-tests"
    ) {
        ResultRule::BreakpointScreenshot
    } else if golden_path.is_some()
        || matches!(
            suite.as_str(),
            "bully"
                | "little-things-gb"
                | "mbc3-tester"
                | "rtc3test"
                | "scribbltests"
                | "strikethrough"
                | "turtle-tests"
        )
    {
        ResultRule::Screenshot
    } else {
        ResultRule::Unsupported
    };

    Some(RomCase {
        suite,
        path,
        relative_path,
        rule,
        target,
        is_audio,
        is_manual,
        is_loose_root_rom,
        golden_path,
        run_budget,
        input_schedule,
        breakpoint_opcode,
    })
}

fn discover_cases(root: &Path) -> Result<Vec<RomCase>, Box<dyn Error>> {
    let mut paths = Vec::new();
    collect_rom_paths(root, &mut paths)?;
    paths.sort();

    Ok(paths
        .into_iter()
        .filter_map(|path| classify_case(root, path))
        .flat_map(expand_case)
        .collect())
}

fn collect_rom_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    if !directory.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rom_paths(&path, paths)?;
        } else if is_rom_path(&path) {
            paths.push(path);
        }
    }

    Ok(())
}

fn expand_case(case: RomCase) -> Vec<RomCase> {
    if case.suite == "rtc3test"
        && case
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "rtc3test.gb")
    {
        return rtc3test_cases(&case);
    }

    vec![case]
}

fn rtc3test_cases(base: &RomCase) -> Vec<RomCase> {
    [
        (
            "basic-tests",
            RunBudget::seconds(13),
            button_tap_sequence(&[Button::A], DMG_CPU_CLOCK_HZ / 3),
        ),
        (
            "range-tests",
            RunBudget::seconds(8),
            button_tap_sequence(&[Button::Down, Button::A], DMG_CPU_CLOCK_HZ / 3),
        ),
        (
            "sub-second-writes",
            RunBudget::seconds(26),
            button_tap_sequence(
                &[Button::Down, Button::Down, Button::A],
                DMG_CPU_CLOCK_HZ / 3,
            ),
        ),
    ]
    .into_iter()
    .map(|(name, run_budget, input_schedule)| {
        let mut case = base.clone();
        case.relative_path = PathBuf::from(format!("rtc3test/rtc3test.gb#{name}"));
        case.golden_path = find_rtc3test_golden(&base.path, name, base.target);
        case.run_budget = run_budget;
        case.input_schedule = input_schedule;
        case
    })
    .collect()
}

fn is_rom_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "gb" | "gbc"))
}

fn is_audio_case(normalized: &str) -> bool {
    normalized.contains("/dmg_sound/")
        || normalized.contains("/cgb_sound/")
        || normalized.contains("/same-suite/apu/")
        || normalized.contains("_outaudio")
        || normalized.contains("/apu/")
}

fn target_requirement(normalized: &str) -> TargetRequirement {
    let file_name = normalized.rsplit('/').next().unwrap_or_default();
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let extension = Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str());
    if normalized.contains("cgb-acid")
        || extension.is_some_and(|extension| extension.eq_ignore_ascii_case("gbc"))
    {
        TargetRequirement::CgbOnly
    } else if normalized.split('/').any(|part| part == "sgb") || has_model_marker(stem, "sgb") {
        TargetRequirement::SgbOnly
    } else if has_model_marker(stem, "dmg") || normalized.contains("dmg-acid2") {
        TargetRequirement::Dmg
    } else if has_model_marker(stem, "cgb")
        || has_model_marker(stem, "gbc")
        || has_model_marker(stem, "ncm")
        || normalized.contains("/hdma")
        || normalized.contains("/gdma")
        || normalized.contains("speed-switch")
    {
        TargetRequirement::CgbOnly
    } else {
        TargetRequirement::Any
    }
}

fn breakpoint_opcode_for(suite: &str) -> u8 {
    if suite == "mooneye-test-suite-wilbertpol" {
        0xED
    } else {
        0x40
    }
}

fn breakpoint_stop_reason(opcode: u8) -> String {
    match opcode {
        0x40 => "ld-b-b-breakpoint".to_string(),
        0xED => "undefined-ed-breakpoint".to_string(),
        _ => format!("opcode-{opcode:02X}-breakpoint"),
    }
}

fn run_budget_for(suite: &str, normalized: &str) -> RunBudget {
    match suite {
        "age-test-roms" | "mooneye-test-suite" | "mooneye-test-suite-wilbertpol" | "same-suite" => {
            RunBudget::breakpoint(120)
        }
        "dmg-acid2" | "cgb-acid2" | "cgb-acid-hell" | "mealybug-tearoom-tests" => {
            RunBudget::breakpoint(30)
        }
        "gbmicrotest" if normalized.ends_with("is_if_set_during_ime0.gb") => {
            RunBudget::milliseconds(380)
        }
        "gbmicrotest" => RunBudget::frames(2),
        "blargg" if normalized.contains("/cpu_instrs/") => RunBudget::seconds(55),
        "blargg" if normalized.contains("/oam_bug/") || normalized.ends_with("oam_bug.gb") => {
            RunBudget::seconds(21)
        }
        "blargg" if normalized.contains("/dmg_sound/") || normalized.contains("/cgb_sound/") => {
            RunBudget::seconds(37)
        }
        "blargg" if normalized.contains("/mem_timing-2/") => RunBudget::seconds(4),
        "blargg" if normalized.contains("/mem_timing/") => RunBudget::seconds(3),
        "blargg" => RunBudget::seconds(2),
        "mbc3-tester" => RunBudget::frames(40),
        "scribbltests" if normalized.contains("statcount-auto") => RunBudget::frames(270),
        "scribbltests" => RunBudget::frames(10),
        "rtc3test" if normalized.contains("sub-second") => RunBudget::seconds(26),
        "rtc3test" if normalized.contains("range") => RunBudget::seconds(8),
        "rtc3test" => RunBudget::seconds(13),
        "little-things-gb" if normalized.contains("telling") => RunBudget::seconds(8),
        "little-things-gb" | "bully" | "strikethrough" | "turtle-tests" => {
            RunBudget::milliseconds(500)
        }
        _ => RunBudget::seconds(5),
    }
}

fn input_schedule_for(normalized: &str) -> Vec<InputEvent> {
    if normalized.contains("little-things-gb") && normalized.contains("telling") {
        return button_tap_sequence(
            &[
                Button::A,
                Button::B,
                Button::Start,
                Button::Select,
                Button::Up,
                Button::Down,
                Button::Left,
                Button::Right,
            ],
            DMG_CPU_CLOCK_HZ / 4,
        );
    }

    if normalized.contains("rtc3test") {
        if normalized.contains("range") {
            return button_tap_sequence(&[Button::Down, Button::A], DMG_CPU_CLOCK_HZ / 3);
        }
        if normalized.contains("sub-second") {
            return button_tap_sequence(
                &[Button::Down, Button::Down, Button::A],
                DMG_CPU_CLOCK_HZ / 3,
            );
        }
        return button_tap_sequence(&[Button::A], DMG_CPU_CLOCK_HZ / 3);
    }

    Vec::new()
}

fn button_tap_sequence(buttons: &[Button], start_at: u64) -> Vec<InputEvent> {
    let mut events = Vec::with_capacity(buttons.len() * 2);
    let mut at_tcycles = start_at;
    for button in buttons {
        events.push(InputEvent {
            at_tcycles,
            button: *button,
            pressed: true,
        });
        events.push(InputEvent {
            at_tcycles: at_tcycles + DMG_CPU_CLOCK_HZ / 20,
            button: *button,
            pressed: false,
        });
        at_tcycles += DMG_CPU_CLOCK_HZ / 5;
    }
    events
}

fn find_golden_for_case(rom_path: &Path, target: TargetRequirement) -> Option<PathBuf> {
    let directory = rom_path.parent()?;
    let stem = rom_path.file_stem()?.to_string_lossy().to_ascii_lowercase();
    let normalized_stem = normalize_name_key(&stem);
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
                .is_some_and(|name| {
                    let name = name.to_ascii_lowercase();
                    let normalized_name = normalize_name_key(&name);
                    name.starts_with(&stem)
                        || normalized_name.starts_with(&normalized_stem)
                        || name.contains("expected")
                        || name.contains("pass")
                })
        })
        .collect::<Vec<_>>();

    candidates.sort_by_key(|path| golden_sort_key(path, target, &stem));
    candidates.into_iter().next()
}

fn find_rtc3test_golden(
    rom_path: &Path,
    subtest: &str,
    target: TargetRequirement,
) -> Option<PathBuf> {
    let directory = rom_path.parent()?;
    let target_label = match target {
        TargetRequirement::CgbOnly => "cgb",
        TargetRequirement::Dmg | TargetRequirement::Any | TargetRequirement::SgbOnly => "dmg",
    };
    fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        })
        .find(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| {
                    let stem = stem.to_ascii_lowercase();
                    stem.contains(subtest) && has_name_label(&stem, target_label)
                })
        })
}

fn golden_sort_key(
    path: &Path,
    target: TargetRequirement,
    stem: &str,
) -> (bool, bool, bool, bool, usize) {
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let variant = name
        .strip_prefix(stem)
        .unwrap_or(&name)
        .trim_start_matches(|character: char| !character.is_ascii_alphanumeric());
    let has_dmg_label = has_name_label(variant, "dmg");
    let has_cgb_label = has_name_label(variant, "cgb");
    let target_mismatch = match target {
        TargetRequirement::Dmg | TargetRequirement::Any => has_cgb_label && !has_dmg_label,
        TargetRequirement::CgbOnly => has_dmg_label && !has_cgb_label,
        TargetRequirement::SgbOnly => false,
    };
    let lacks_target_label = match target {
        TargetRequirement::Dmg | TargetRequirement::Any => !has_dmg_label,
        TargetRequirement::CgbOnly => !has_cgb_label,
        TargetRequirement::SgbOnly => false,
    };

    (
        target_mismatch,
        lacks_target_label,
        !name.starts_with(stem),
        !(has_dmg_label || name.contains("expected") || name.contains("pass")),
        name.len(),
    )
}

fn has_name_label(name: &str, label: &str) -> bool {
    name.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|part| part == label)
}

fn has_model_marker(name: &str, marker: &str) -> bool {
    name.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|part| part == marker || part.starts_with(marker))
}

fn normalize_name_key(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

fn missing_golden_is_skip(case: &RomCase) -> bool {
    normalize_path(&case.relative_path) == "mealybug-tearoom-tests/ppu/win_without_bg.gb"
}

fn run_case(case: &RomCase, options: &Options) -> RomResult {
    let relative_path = normalize_path(&case.relative_path);
    if case.target == TargetRequirement::SgbOnly {
        return skipped(case, "SGB-only ROM; rustboy does not target Super Game Boy");
    }
    if case.target == TargetRequirement::CgbOnly && options.target == TargetModel::Dmg {
        return skipped(case, "CGB-only ROM while runner target is DMG");
    }
    if case.is_manual {
        return skipped(case, "manual-only or interactive visual test");
    }
    if case.is_audio && !options.include_audio {
        return skipped(case, "audio test excluded by profile/options");
    }
    if case.rule == ResultRule::Audio {
        return unsupported(
            case,
            "audio tests need deterministic APU output before automation",
        );
    }
    if case.rule == ResultRule::Unsupported {
        return unsupported(case, "no automated result rule registered for this ROM");
    }
    if matches!(
        case.rule,
        ResultRule::Screenshot | ResultRule::BreakpointScreenshot | ResultRule::SerialText
    ) && case.golden_path.is_none()
        && case.rule != ResultRule::SerialText
    {
        if missing_golden_is_skip(case) {
            return skipped(
                case,
                "source-defined screenshot test has no local golden PNG",
            );
        }
        return unsupported(case, "screenshot rule has no colocated golden PNG");
    }

    let rom = match fs::read(&case.path) {
        Ok(rom) => rom,
        Err(error) => return emulator_error(case, format!("failed to read ROM: {error}")),
    };
    let cartridge = match Cartridge::from_bytes(rom) {
        Ok(cartridge) => cartridge,
        Err(error) => return emulator_error(case, format!("failed to load cartridge: {error}")),
    };
    let mut game_boy = GameBoy::new(cartridge);
    let run = run_emulation(&mut game_boy, case, options.case_timeout);
    let registers = RegisterSnapshot::from(game_boy.registers());
    let serial = serial_excerpt(game_boy.serial_output());

    match run {
        Ok(stop) => evaluate_case(
            case,
            EvaluationContext {
                profile: options.profile,
                target_model: options.target,
            },
            game_boy.framebuffer(),
            &game_boy,
            stop,
            registers,
            serial,
        ),
        Err(error) => RomResult {
            suite: case.suite.clone(),
            rom_path: relative_path,
            target_model: options.target,
            profile: options.profile,
            status: ResultStatus::EmulatorError,
            result_rule: case.rule,
            stop_reason: Some("emulator-error".to_string()),
            frames: error.frames,
            tcycles: error.tcycles,
            serial_excerpt: serial,
            registers: Some(registers),
            memory_checks: None,
            golden_path: case
                .golden_path
                .as_ref()
                .map(|path| path.display().to_string()),
            pixel_diff: None,
            error: Some(error.message),
            agent_notes: "Emulation stopped before the result rule could be evaluated.".to_string(),
        },
    }
}

fn run_emulation(
    game_boy: &mut GameBoy,
    case: &RomCase,
    case_timeout: Duration,
) -> Result<RunStop, RunFailure> {
    let mut tcycles = 0_u64;
    let mut frames = 0_u64;
    let mut next_input = 0_usize;
    let started_at = Instant::now();

    loop {
        if started_at.elapsed() >= case_timeout {
            return Ok(RunStop {
                reason: format!("wall-timeout-{}s", case_timeout.as_secs()),
                frames,
                tcycles,
                timed_out: true,
            });
        }

        while let Some(event) = case.input_schedule.get(next_input) {
            if event.at_tcycles > tcycles {
                break;
            }
            game_boy.set_button(event.button, event.pressed);
            next_input += 1;
        }

        if matches!(case.run_budget.stop, StopCondition::Breakpoint)
            && game_boy.debug_read8(game_boy.registers().pc) == case.breakpoint_opcode
        {
            return Ok(RunStop {
                reason: breakpoint_stop_reason(case.breakpoint_opcode),
                frames,
                tcycles,
                timed_out: false,
            });
        }

        if case.rule == ResultRule::RamSignature
            && matches!(game_boy.debug_read8(0xFF82), 0x01 | 0xFF)
        {
            return Ok(RunStop {
                reason: "gbmicrotest-status".to_string(),
                frames,
                tcycles,
                timed_out: false,
            });
        }

        if tcycles >= case.run_budget.max_tcycles {
            return Ok(RunStop {
                reason: "timeout".to_string(),
                frames,
                tcycles,
                timed_out: true,
            });
        }

        match game_boy.step() {
            Ok(cycles) => {
                tcycles += u64::from(cycles.0);
                frames = tcycles / DMG_TCYCLES_PER_FRAME;
                match case.run_budget.stop {
                    StopCondition::Frames(frame_limit) if frames >= frame_limit => {
                        return Ok(RunStop {
                            reason: format!("{frame_limit}-frame-budget"),
                            frames,
                            tcycles,
                            timed_out: false,
                        });
                    }
                    StopCondition::TCycles(limit) if tcycles >= limit => {
                        return Ok(RunStop {
                            reason: format!("{limit}-tcycle-budget"),
                            frames,
                            tcycles,
                            timed_out: false,
                        });
                    }
                    _ => {}
                }
            }
            Err(error) => {
                return Err(RunFailure {
                    message: error.to_string(),
                    frames,
                    tcycles,
                });
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunStop {
    reason: String,
    frames: u64,
    tcycles: u64,
    timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunFailure {
    message: String,
    frames: u64,
    tcycles: u64,
}

fn evaluate_case(
    case: &RomCase,
    context: EvaluationContext,
    framebuffer: &[u32],
    game_boy: &GameBoy,
    stop: RunStop,
    registers: RegisterSnapshot,
    serial: String,
) -> RomResult {
    let (status, error, memory_checks, pixel_diff, notes) = match case.rule {
        ResultRule::BreakpointRegisters => {
            if stop.timed_out {
                (
                    ResultStatus::Timeout,
                    Some(format!(
                        "breakpoint opcode 0x{:02X} was not reached",
                        case.breakpoint_opcode
                    )),
                    None,
                    None,
                    "Breakpoint register signature could not be evaluated before timeout."
                        .to_string(),
                )
            } else {
                evaluate_fibonacci(registers)
            }
        }
        ResultRule::BreakpointScreenshot => {
            if stop.timed_out {
                (
                    ResultStatus::Timeout,
                    Some("LD B,B breakpoint was not reached".to_string()),
                    None,
                    None,
                    "Breakpoint screenshot could not be captured before timeout.".to_string(),
                )
            } else {
                evaluate_screenshot(case, framebuffer)
            }
        }
        ResultRule::SerialText => {
            let serial_eval = evaluate_serial_text(&serial);
            if serial_eval.0 == ResultStatus::Timeout {
                if let Some((status, error, _, pixel_diff, notes)) = case
                    .golden_path
                    .as_ref()
                    .map(|_| evaluate_screenshot(case, framebuffer))
                {
                    (status, error, None, pixel_diff, notes)
                } else {
                    serial_eval
                }
            } else {
                serial_eval
            }
        }
        ResultRule::RamSignature => evaluate_ram_signature(
            game_boy.debug_read8(0xFF80),
            game_boy.debug_read8(0xFF81),
            game_boy.debug_read8(0xFF82),
        ),
        ResultRule::Screenshot => evaluate_screenshot(case, framebuffer),
        ResultRule::Audio | ResultRule::Unsupported => (
            ResultStatus::Unsupported,
            Some("unsupported rule reached evaluator".to_string()),
            None,
            None,
            "This rule should be filtered before emulation.".to_string(),
        ),
    };

    RomResult {
        suite: case.suite.clone(),
        rom_path: normalize_path(&case.relative_path),
        target_model: context.target_model,
        profile: context.profile,
        status,
        result_rule: case.rule,
        stop_reason: Some(stop.reason),
        frames: stop.frames,
        tcycles: stop.tcycles,
        serial_excerpt: serial,
        registers: Some(registers),
        memory_checks,
        golden_path: case
            .golden_path
            .as_ref()
            .map(|path| path.display().to_string()),
        pixel_diff,
        error,
        agent_notes: notes,
    }
}

fn evaluate_fibonacci(
    registers: RegisterSnapshot,
) -> (
    ResultStatus,
    Option<String>,
    Option<Vec<MemoryCheck>>,
    Option<u64>,
    String,
) {
    let actual = (
        registers.b,
        registers.c,
        registers.d,
        registers.e,
        registers.h,
        registers.l,
    );
    if actual == FIBONACCI_REGISTERS {
        (
            ResultStatus::Passed,
            None,
            None,
            None,
            "Fibonacci register signature matched.".to_string(),
        )
    } else {
        (
            ResultStatus::Failed,
            Some(format!(
                "Fibonacci register mismatch: got B={} C={} D={} E={} H={} L={}",
                registers.b, registers.c, registers.d, registers.e, registers.h, registers.l
            )),
            None,
            None,
            "The ROM reached its software breakpoint but reported failure registers.".to_string(),
        )
    }
}

fn evaluate_ram_signature(
    actual_byte: u8,
    expected_byte: u8,
    status_byte: u8,
) -> (
    ResultStatus,
    Option<String>,
    Option<Vec<MemoryCheck>>,
    Option<u64>,
    String,
) {
    let checks = vec![
        MemoryCheck {
            address: "0xFF80".to_string(),
            expected: "test result byte".to_string(),
            actual: format!("0x{actual_byte:02X}"),
        },
        MemoryCheck {
            address: "0xFF81".to_string(),
            expected: "expected result byte".to_string(),
            actual: format!("0x{expected_byte:02X}"),
        },
        MemoryCheck {
            address: "0xFF82".to_string(),
            expected: "0x01 pass or 0xFF fail".to_string(),
            actual: format!("0x{status_byte:02X}"),
        },
    ];
    match status_byte {
        0x01 => (
            ResultStatus::Passed,
            None,
            Some(checks),
            None,
            "GBMicrotest status byte reported pass.".to_string(),
        ),
        0xFF => (
            ResultStatus::Failed,
            Some("GBMicrotest status byte reported failure".to_string()),
            Some(checks),
            None,
            "GBMicrotest wrote its failure sentinel to HRAM.".to_string(),
        ),
        _ => (
            ResultStatus::Timeout,
            Some("GBMicrotest status byte did not reach pass/fail sentinel".to_string()),
            Some(checks),
            None,
            "The test may need more time, or emulation stopped before the ROM completed."
                .to_string(),
        ),
    }
}

fn evaluate_serial_text(
    serial: &str,
) -> (
    ResultStatus,
    Option<String>,
    Option<Vec<MemoryCheck>>,
    Option<u64>,
    String,
) {
    let lower = serial.to_ascii_lowercase();
    if lower.contains("passed") || lower.contains("pass") {
        (
            ResultStatus::Passed,
            None,
            None,
            None,
            "Serial output contained a pass marker.".to_string(),
        )
    } else if lower.contains("failed") || lower.contains("fail") {
        (
            ResultStatus::Failed,
            Some("serial output contained a fail marker".to_string()),
            None,
            None,
            "Serial output reported failure.".to_string(),
        )
    } else {
        (
            ResultStatus::Timeout,
            Some("serial output did not contain pass/fail text".to_string()),
            None,
            None,
            "No serial pass/fail marker was observed before the run budget expired.".to_string(),
        )
    }
}

fn evaluate_screenshot(
    case: &RomCase,
    framebuffer: &[u32],
) -> (
    ResultStatus,
    Option<String>,
    Option<Vec<MemoryCheck>>,
    Option<u64>,
    String,
) {
    let Some(golden_path) = case.golden_path.as_ref() else {
        return (
            ResultStatus::Unsupported,
            Some("no golden PNG found".to_string()),
            None,
            None,
            "Screenshot comparison needs a colocated expected PNG.".to_string(),
        );
    };

    match pixel_diff(framebuffer, golden_path) {
        Ok(0) => (
            ResultStatus::Passed,
            None,
            None,
            Some(0),
            "Framebuffer exactly matched the golden PNG.".to_string(),
        ),
        Ok(diff) => (
            ResultStatus::Failed,
            Some(format!("framebuffer differs from golden by {diff} pixels")),
            None,
            Some(diff),
            "Exact screenshot comparison failed; inspect PPU rendering or timing first."
                .to_string(),
        ),
        Err(error) => (
            ResultStatus::EmulatorError,
            Some(format!("failed to compare golden PNG: {error}")),
            None,
            None,
            "The ROM ran, but report generation could not compare the image.".to_string(),
        ),
    }
}

fn pixel_diff(framebuffer: &[u32], golden_path: &Path) -> Result<u64, Box<dyn Error>> {
    let image = image::ImageReader::open(golden_path)?.decode()?.to_rgba8();
    let (width, height) = image.dimensions();
    if width != u32::try_from(SCREEN_WIDTH)? || height != u32::try_from(SCREEN_HEIGHT)? {
        return Ok(u64::try_from(SCREEN_WIDTH * SCREEN_HEIGHT)?);
    }

    let mut diff = 0_u64;
    for (pixel, expected) in framebuffer.iter().zip(image.pixels()) {
        let [red, green, blue, alpha] = expected.0;
        let expected = u32::from_be_bytes([alpha, red, green, blue]);
        if *pixel != expected {
            diff += 1;
        }
    }

    Ok(diff)
}

#[derive(Debug, Clone, Serialize)]
struct Report {
    generated_at: String,
    profile: Profile,
    target_model: TargetModel,
    results: Vec<RomResult>,
}

impl Report {
    fn new(profile: Profile, target_model: TargetModel, mut results: Vec<RomResult>) -> Self {
        for result in &mut results {
            result.profile = profile;
            result.target_model = target_model;
        }

        Self {
            generated_at: format!("{:?}", SystemTime::now()),
            profile,
            target_model,
            results,
        }
    }

    fn count(&self, status: ResultStatus) -> usize {
        self.results
            .iter()
            .filter(|result| result.status == status)
            .count()
    }
}

#[derive(Debug, Clone, Serialize)]
struct RomResult {
    suite: String,
    rom_path: String,
    target_model: TargetModel,
    profile: Profile,
    status: ResultStatus,
    result_rule: ResultRule,
    stop_reason: Option<String>,
    frames: u64,
    tcycles: u64,
    serial_excerpt: String,
    registers: Option<RegisterSnapshot>,
    memory_checks: Option<Vec<MemoryCheck>>,
    golden_path: Option<String>,
    pixel_diff: Option<u64>,
    error: Option<String>,
    agent_notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ResultStatus {
    Passed,
    Failed,
    Timeout,
    Unsupported,
    Skipped,
    EmulatorError,
}

impl fmt::Display for ResultStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Passed => "Passed",
            Self::Failed => "Failed",
            Self::Timeout => "Timeout",
            Self::Unsupported => "Unsupported",
            Self::Skipped => "Skipped",
            Self::EmulatorError => "EmulatorError",
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
struct RegisterSnapshot {
    a: u8,
    f: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    h: u8,
    l: u8,
    sp: u16,
    pc: u16,
}

impl From<&CpuRegisters> for RegisterSnapshot {
    fn from(registers: &CpuRegisters) -> Self {
        Self {
            a: registers.a,
            f: registers.f.raw(),
            b: registers.b,
            c: registers.c,
            d: registers.d,
            e: registers.e,
            h: registers.h,
            l: registers.l,
            sp: registers.sp,
            pc: registers.pc,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct MemoryCheck {
    address: String,
    expected: String,
    actual: String,
}

fn skipped(case: &RomCase, reason: &str) -> RomResult {
    static_result(case, ResultStatus::Skipped, reason)
}

fn unsupported(case: &RomCase, reason: &str) -> RomResult {
    static_result(case, ResultStatus::Unsupported, reason)
}

fn emulator_error(case: &RomCase, error: String) -> RomResult {
    let mut result = static_result(case, ResultStatus::EmulatorError, "setup failed");
    result.error = Some(error);
    result
}

fn static_result(case: &RomCase, status: ResultStatus, reason: &str) -> RomResult {
    RomResult {
        suite: case.suite.clone(),
        rom_path: normalize_path(&case.relative_path),
        target_model: TargetModel::Dmg,
        profile: Profile::NoAudio,
        status,
        result_rule: case.rule,
        stop_reason: Some(reason.to_string()),
        frames: 0,
        tcycles: 0,
        serial_excerpt: String::new(),
        registers: None,
        memory_checks: None,
        golden_path: case
            .golden_path
            .as_ref()
            .map(|path| path.display().to_string()),
        pixel_diff: None,
        error: if status == ResultStatus::Skipped {
            None
        } else {
            Some(reason.to_string())
        },
        agent_notes: reason.to_string(),
    }
}

fn serial_excerpt(bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).to_string();
    if text.len() > SERIAL_EXCERPT_LIMIT {
        text.truncate(SERIAL_EXCERPT_LIMIT);
        text.push_str("...");
    }
    text
}

fn write_json_report(report: &Report, out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let path = out_dir.join("report.json");
    let json = serde_json::to_string_pretty(report)?;
    fs::write(path, json)?;
    Ok(())
}

fn write_markdown_report(report: &Report, out_dir: &Path) -> Result<(), Box<dyn Error>> {
    fs::write(out_dir.join("report.md"), markdown_report(report))?;
    Ok(())
}

fn markdown_report(report: &Report) -> String {
    let mut output = String::new();
    output.push_str("# Test ROM Report\n\n");
    let _ = write!(
        output,
        "- Profile: `{}`\n- Target: `{}`\n- Results: `{}`\n\n",
        report.profile,
        report.target_model,
        report.results.len()
    );

    output.push_str("## Summary\n\n");
    output.push_str("| Status | Count |\n|---|---:|\n");
    for status in [
        ResultStatus::Passed,
        ResultStatus::Failed,
        ResultStatus::Timeout,
        ResultStatus::EmulatorError,
        ResultStatus::Unsupported,
        ResultStatus::Skipped,
    ] {
        let _ = writeln!(output, "| {status} | {} |", report.count(status));
    }

    output.push_str("\n## Suites\n\n");
    output.push_str("| Suite | Passed | Failed | Timeout | Error | Unsupported | Skipped |\n");
    output.push_str("|---|---:|---:|---:|---:|---:|---:|\n");
    for (suite, counts) in suite_counts(&report.results) {
        let _ = writeln!(
            output,
            "| `{suite}` | {} | {} | {} | {} | {} | {} |",
            counts.get(ResultStatus::Passed),
            counts.get(ResultStatus::Failed),
            counts.get(ResultStatus::Timeout),
            counts.get(ResultStatus::EmulatorError),
            counts.get(ResultStatus::Unsupported),
            counts.get(ResultStatus::Skipped)
        );
    }

    output.push_str("\n## Actionable Failures\n\n");
    let failures = report
        .results
        .iter()
        .filter(|result| {
            matches!(
                result.status,
                ResultStatus::Failed | ResultStatus::Timeout | ResultStatus::EmulatorError
            )
        })
        .take(20)
        .collect::<Vec<_>>();
    if failures.is_empty() {
        output.push_str("No failed, timed out, or errored ROM results in this run.\n");
    } else {
        output.push_str("| ROM | Status | Rule | Reason |\n|---|---|---|---|\n");
        for result in failures {
            let reason = result
                .error
                .as_deref()
                .or(result.stop_reason.as_deref())
                .unwrap_or("no reason recorded");
            let _ = writeln!(
                output,
                "| `{}` | {} | `{}` | {} |",
                result.rom_path,
                result.status,
                result.result_rule,
                markdown_escape(reason)
            );
        }
    }

    output.push_str("\n## Suggested Focus\n\n");
    output.push_str(&suggested_focus(report));
    output.push('\n');

    output
}

#[derive(Default)]
struct StatusCounts {
    counts: BTreeMap<ResultStatus, usize>,
}

impl StatusCounts {
    fn add(&mut self, status: ResultStatus) {
        *self.counts.entry(status).or_default() += 1;
    }

    fn get(&self, status: ResultStatus) -> usize {
        self.counts.get(&status).copied().unwrap_or(0)
    }
}

fn suite_counts(results: &[RomResult]) -> BTreeMap<String, StatusCounts> {
    let mut counts = BTreeMap::<String, StatusCounts>::new();
    for result in results {
        counts
            .entry(result.suite.clone())
            .or_default()
            .add(result.status);
    }
    counts
}

fn suggested_focus(report: &Report) -> String {
    let ppu_failures = report
        .results
        .iter()
        .filter(|result| {
            matches!(
                result.result_rule,
                ResultRule::Screenshot | ResultRule::BreakpointScreenshot
            ) && matches!(result.status, ResultStatus::Failed | ResultStatus::Timeout)
        })
        .count();
    let cpu_failures = report
        .results
        .iter()
        .filter(|result| {
            result.result_rule == ResultRule::BreakpointRegisters
                && matches!(result.status, ResultStatus::Failed | ResultStatus::Timeout)
        })
        .count();
    let serial_failures = report
        .results
        .iter()
        .filter(|result| {
            result.result_rule == ResultRule::SerialText
                && matches!(result.status, ResultStatus::Failed | ResultStatus::Timeout)
        })
        .count();

    if cpu_failures >= ppu_failures && cpu_failures >= serial_failures && cpu_failures > 0 {
        "CPU/control-flow correctness is the strongest signal in this run.".to_string()
    } else if ppu_failures > 0 {
        "PPU rendering/timing is the strongest signal in this run.".to_string()
    } else if serial_failures > 0 {
        "Serial-output ROM completion is the strongest signal in this run.".to_string()
    } else {
        "No dominant failure cluster detected.".to_string()
    }
}

fn markdown_escape(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn paths_match(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    normalize_path(left).ends_with(&normalize_path(right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gb_core::cpu::CpuFlags;

    fn case_for(path: &str) -> RomCase {
        classify_case(
            Path::new("test-roms"),
            PathBuf::from("test-roms").join(path),
        )
        .expect("path should classify")
    }

    fn minimal_rom() -> Vec<u8> {
        let mut rom = vec![0; 0x8000];
        rom[0x0147] = 0x00;
        rom[0x0148] = 0x00;
        rom[0x0149] = 0x00;
        rom[0x014D] = header_checksum(&rom);
        rom
    }

    fn header_checksum(rom: &[u8]) -> u8 {
        let mut checksum = 0_u8;

        for byte in &rom[0x0134..=0x014C] {
            checksum = checksum.wrapping_sub(*byte).wrapping_sub(1);
        }

        checksum
    }

    #[test]
    fn classification_skips_gambatte() {
        assert!(classify_case(
            Path::new("test-roms"),
            PathBuf::from("test-roms/gambatte/cgb04c_out1.gbc")
        )
        .is_none());
    }

    #[test]
    fn options_parse_worker_controls() {
        let options = Options::parse([
            "run".to_string(),
            "--jobs".to_string(),
            "3".to_string(),
            "--case-timeout-seconds".to_string(),
            "7".to_string(),
        ])
        .expect("worker controls should parse");

        assert_eq!(options.jobs, 3);
        assert_eq!(options.case_timeout, Duration::from_secs(7));
    }

    #[test]
    fn options_reject_zero_workers() {
        let error = Options::parse(["run".to_string(), "--jobs".to_string(), "0".to_string()])
            .expect_err("zero workers should be rejected");

        assert!(
            error.to_string().contains("--jobs"),
            "error should name the invalid option"
        );
    }

    #[test]
    fn classification_assigns_core_result_rules() {
        assert_eq!(
            case_for("mooneye-test-suite/acceptance/instr/daa.gb").rule,
            ResultRule::BreakpointRegisters
        );
        assert_eq!(
            case_for("gbmicrotest/cpu/add_sp_e_timing.gb").rule,
            ResultRule::RamSignature
        );
        assert_eq!(
            case_for("blargg/cpu_instrs/cpu_instrs.gb").rule,
            ResultRule::SerialText
        );
        assert_eq!(
            case_for("same-suite/apu/channel_1/channel_1_align.gb").rule,
            ResultRule::Audio
        );
        assert_eq!(
            case_for("mealybug-tearoom-tests/mbc/mbc3_rtc.gb").rule,
            ResultRule::BreakpointRegisters
        );
    }

    #[test]
    fn target_classification_marks_cgb_and_sgb_cases() {
        assert_eq!(
            case_for("cgb-acid2/cgb-acid2.gbc").target,
            TargetRequirement::CgbOnly
        );
        assert_eq!(
            case_for("mooneye-test-suite/acceptance/boot_regs-cgb.gb").target,
            TargetRequirement::CgbOnly
        );
        assert_eq!(
            case_for("age-test-roms/lcd-align-ly/lcd-align-ly-cgbBC.gb").target,
            TargetRequirement::CgbOnly
        );
        assert_eq!(
            case_for("age-test-roms/halt/halt-prefetch-dmgC-cgbBCE.gb").target,
            TargetRequirement::Dmg
        );
        assert_eq!(
            case_for("same-suite/sgb/command_mlt_req.gb").target,
            TargetRequirement::SgbOnly
        );
        assert_eq!(
            case_for("mooneye-test-suite/acceptance/boot_regs-sgb.gb").target,
            TargetRequirement::SgbOnly
        );
    }

    #[test]
    fn dmg_profile_excludes_loose_root_roms() {
        let options = Options::parse([
            "run".to_string(),
            "--profile".to_string(),
            "dmg".to_string(),
        ])
        .expect("DMG profile should parse");

        assert!(!options.matches(&case_for("tetris.gb")));
        assert!(Options::parse([
            "run".to_string(),
            "--profile".to_string(),
            "dmg".to_string(),
            "--rom".to_string(),
            "test-roms/tetris.gb".to_string(),
        ])
        .expect("explicit ROM should parse")
        .matches(&case_for("tetris.gb")));
    }

    #[test]
    fn wilbertpol_uses_legacy_undefined_opcode_breakpoint() {
        assert_eq!(
            case_for("mooneye-test-suite-wilbertpol/acceptance/bits/mem_oam.gb").breakpoint_opcode,
            0xED
        );
        assert_eq!(
            case_for("mooneye-test-suite/acceptance/bits/mem_oam.gb").breakpoint_opcode,
            0x40
        );
    }

    #[test]
    fn rtc3test_expands_to_three_scripted_subtests() {
        let base = case_for("rtc3test/rtc3test.gb");
        let cases = expand_case(base);
        let paths = cases
            .iter()
            .map(|case| normalize_path(&case.relative_path))
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![
                "rtc3test/rtc3test.gb#basic-tests",
                "rtc3test/rtc3test.gb#range-tests",
                "rtc3test/rtc3test.gb#sub-second-writes",
            ]
        );
        assert_eq!(cases[0].input_schedule.len(), 2);
        assert_eq!(cases[1].input_schedule.len(), 4);
        assert_eq!(cases[2].input_schedule.len(), 6);
    }

    #[test]
    fn golden_sort_prefers_dmg_label_for_dmg_target() {
        let cgb = Path::new("test-roms/dmg-acid2/dmg-acid2-cgb.png");
        let dmg = Path::new("test-roms/dmg-acid2/dmg-acid2-dmg.png");

        assert!(
            golden_sort_key(dmg, TargetRequirement::Dmg, "dmg-acid2")
                < golden_sort_key(cgb, TargetRequirement::Dmg, "dmg-acid2"),
            "DMG runs should prefer the DMG-labelled acid2 golden over the CGB-labelled one"
        );
    }

    #[test]
    fn golden_lookup_tolerates_filename_separator_differences() {
        let temp_dir = env::temp_dir().join("rustboy-gb-romtest-flexible-golden");
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let rom_path = temp_dir.join("statcount-auto.gb");
        let golden_path = temp_dir.join("statcount_auto-cgb-dmg.png");
        fs::write(&rom_path, []).expect("ROM placeholder should save");
        fs::write(&golden_path, []).expect("golden placeholder should save");

        assert_eq!(
            find_golden_for_case(&rom_path, TargetRequirement::Dmg),
            Some(golden_path)
        );
    }

    #[test]
    fn known_source_screenshot_without_golden_is_skipped() {
        assert!(missing_golden_is_skip(&case_for(
            "mealybug-tearoom-tests/ppu/win_without_bg.gb"
        )));
        assert!(!missing_golden_is_skip(&case_for(
            "mealybug-tearoom-tests/ppu/m3_bgp_change.gb"
        )));
    }

    #[test]
    fn fibonacci_evaluator_passes_only_expected_signature() {
        let passing = RegisterSnapshot {
            a: 0,
            f: 0,
            b: 3,
            c: 5,
            d: 8,
            e: 13,
            h: 21,
            l: 34,
            sp: 0,
            pc: 0,
        };
        assert_eq!(evaluate_fibonacci(passing).0, ResultStatus::Passed);

        let mut failing = passing;
        failing.l = 35;
        assert_eq!(evaluate_fibonacci(failing).0, ResultStatus::Failed);
    }

    #[test]
    fn breakpoint_register_timeout_does_not_evaluate_fibonacci_signature() {
        let case = case_for("mooneye-test-suite/acceptance/instr/daa.gb");
        let registers = RegisterSnapshot {
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            sp: 0,
            pc: 0x1234,
        };
        let framebuffer = vec![0; SCREEN_WIDTH * SCREEN_HEIGHT];
        let result = evaluate_case(
            &case,
            EvaluationContext {
                profile: Profile::Dmg,
                target_model: TargetModel::Dmg,
            },
            &framebuffer,
            &GameBoy::new(Cartridge::from_bytes(minimal_rom()).expect("ROM should load")),
            RunStop {
                reason: "timeout".to_string(),
                frames: 0,
                tcycles: 0,
                timed_out: true,
            },
            registers,
            String::new(),
        );

        assert_eq!(result.status, ResultStatus::Timeout);
        assert!(result
            .error
            .expect("timeout should explain missing breakpoint")
            .contains("0x40"));
    }

    #[test]
    fn ram_signature_evaluator_uses_ff82_sentinel() {
        assert_eq!(
            evaluate_ram_signature(0x12, 0x12, 0x01).0,
            ResultStatus::Passed
        );
        assert_eq!(
            evaluate_ram_signature(0x12, 0x34, 0xFF).0,
            ResultStatus::Failed
        );
        assert_eq!(
            evaluate_ram_signature(0x00, 0x00, 0x00).0,
            ResultStatus::Timeout
        );
    }

    #[test]
    fn ram_signature_evaluator_reports_all_documented_bytes() {
        let checks = evaluate_ram_signature(0x12, 0x34, 0xFF)
            .2
            .expect("RAM signature should report memory checks");

        assert_eq!(checks.len(), 3);
        assert_eq!(checks[0].address, "0xFF80");
        assert_eq!(checks[0].actual, "0x12");
        assert_eq!(checks[1].address, "0xFF81");
        assert_eq!(checks[1].actual, "0x34");
        assert_eq!(checks[2].address, "0xFF82");
        assert_eq!(checks[2].actual, "0xFF");
    }

    #[test]
    fn serial_evaluator_detects_pass_and_fail_text() {
        assert_eq!(
            evaluate_serial_text("cpu_instrs\nPassed all tests").0,
            ResultStatus::Passed
        );
        assert_eq!(evaluate_serial_text("Failed #3").0, ResultStatus::Failed);
        assert_eq!(evaluate_serial_text("").0, ResultStatus::Timeout);
    }

    #[test]
    fn scripted_input_sequence_taps_buttons() {
        let events = button_tap_sequence(&[Button::A, Button::Start], 10);

        assert_eq!(events.len(), 4);
        assert_eq!(events[0].button, Button::A);
        assert!(events[0].pressed);
        assert_eq!(events[1].button, Button::A);
        assert!(!events[1].pressed);
        assert_eq!(events[2].button, Button::Start);
    }

    #[test]
    fn screenshot_diff_counts_changed_pixels() {
        let temp_dir = env::temp_dir().join("rustboy-gb-romtest-pixel-diff");
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let path = temp_dir.join("expected.png");
        let mut image = image::RgbaImage::new(
            u32::try_from(SCREEN_WIDTH).expect("width fits"),
            u32::try_from(SCREEN_HEIGHT).expect("height fits"),
        );
        for pixel in image.pixels_mut() {
            *pixel = image::Rgba([255, 255, 255, 255]);
        }
        image.save(&path).expect("test image should save");

        let mut framebuffer = vec![0xFFFF_FFFF; SCREEN_WIDTH * SCREEN_HEIGHT];
        assert_eq!(
            pixel_diff(&framebuffer, &path).expect("matching image should compare"),
            0
        );

        framebuffer[0] = 0xFF00_0000;
        assert_eq!(
            pixel_diff(&framebuffer, &path).expect("changed image should compare"),
            1
        );
    }

    #[test]
    fn register_snapshot_reads_cpu_registers() {
        let registers = CpuRegisters {
            a: 1,
            f: CpuFlags::from_raw(0xB0),
            b: 3,
            c: 5,
            d: 8,
            e: 13,
            h: 21,
            l: 34,
            sp: 0xFFFE,
            pc: 0x0100,
        };

        let snapshot = RegisterSnapshot::from(&registers);

        assert_eq!(snapshot.f, 0xB0);
        assert_eq!(snapshot.pc, 0x0100);
    }
}
