use crate::apu::Apu;
use crate::constants::{RomInfo, CAP_FPS, CPU_TYPE_NEW, DEBUG_ASM, DEBUG_MESEN, HEIGHT, WIDTH, WINDOW_TITLE};
use crate::joypad::{Button, Joypad};
use crate::mappers::mapper_base::MapperBase;
use crate::mesen_logger::{MesenLogger, LOG_CYCLE, LOG_SCANLINE};
use crate::nes_memory::NesMemory;
use crate::ppu2::{PpuResult, CURRENT_CYCLE, CURRENT_SCANLINE};
use crate::rom::Rom;
use crate::Args;
use cpu::config::{Config, System};
use cpu::cpu::Cpu;
use cpu::cpu2::Cpu2;
use cpu::external_logger::{DefaultLogger, IExternalLogger};
use cpu::labels::Labels;
use cpu::memory::Memory;
use enum_dispatch::enum_dispatch;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use gilrs::{Axis, Button as GilButton, EventType, Gilrs};
use tokio::sync::broadcast::{Receiver, Sender};
use tracing::{debug, info, warn};
use crate::bk2::Bk2Movie;
use crate::config_file::EmulatorConfig;
use crate::fm2::Fm2Movie;
use crate::ppu2::Ppu2;

const TICK_BATCH_CYCLES: usize = 2_000;

pub struct FrameStat {
    _duration_ms: u16,
}

pub static FRAME: Lazy<RwLock<[u8; WIDTH * HEIGHT]>> =
    Lazy::new(|| RwLock::new([0; WIDTH * HEIGHT]));

/// Trait that abstracts over `Cpu<NesMemory>` and `Cpu2<NesMemory>`.
#[allow(dead_code)]
#[enum_dispatch]
pub trait CpuInterface {
    fn set_pc(&mut self, pc: u16);
    fn pc(&self) -> u16;
    fn set_s(&mut self, v: u8);
    fn nmi(&mut self);
    fn irq(&mut self);
    fn get_cycles(&self) -> u128;
    fn is_get_phase(&self) -> bool;
    fn add_cycles(&mut self, cycles: u128);
    fn one_cycle(&mut self, config: &mut Config, breakpoints: &HashSet<u16>) -> (bool, u8);
    fn set_get_phase(&mut self, get: bool);
    fn memory(&mut self) -> &mut NesMemory;
}

impl CpuInterface for Cpu<NesMemory> {
    fn set_pc(&mut self, pc: u16) { Cpu::set_pc(self, pc); }
    fn pc(&self) -> u16 { Cpu::pc(self) }
    fn set_s(&mut self, v: u8) { self.s = v; }
    fn nmi(&mut self) { Cpu::nmi(self); }
    fn irq(&mut self) { Cpu::irq(self); }
    fn get_cycles(&self) -> u128 { self.cycles }
    fn is_get_phase(&self) -> bool { false }
    fn add_cycles(&mut self, cycles: u128) { self.cycles += cycles; }
    fn one_cycle(&mut self, config: &mut Config, breakpoints: &HashSet<u16>) -> (bool, u8) {
        Cpu::one_cycle(self, config, breakpoints)
    }
    fn set_get_phase(&mut self, _get: bool) {}
    fn memory(&mut self) -> &mut NesMemory { &mut self.memory }
}

impl CpuInterface for Cpu2<NesMemory> {
    fn set_pc(&mut self, pc: u16) { Cpu2::set_pc(self, pc); }
    fn pc(&self) -> u16 { Cpu2::pc(self) }
    fn set_s(&mut self, v: u8) { self.s = v; }
    fn nmi(&mut self) { Cpu2::nmi(self); }
    fn irq(&mut self) { Cpu2::irq(self); }
    fn get_cycles(&self) -> u128 { self.cycles }
    fn is_get_phase(&self) -> bool { self.is_get_phase }
    fn add_cycles(&mut self, cycles: u128) { self.cycles += cycles; }
    fn one_cycle(&mut self, config: &mut Config, breakpoints: &HashSet<u16>) -> (bool, u8) {
        Cpu2::one_cycle(self, config, breakpoints)
    }
    fn set_get_phase(&mut self, get: bool) { self.is_get_phase = get; }
    fn memory(&mut self) -> &mut NesMemory { &mut self.memory }
}

#[enum_dispatch(CpuInterface)]
pub(crate) enum CpuType {
    Old(Cpu<NesMemory>),
    New(Cpu2<NesMemory>),
}


pub struct Emulator {
    // New
    // pub cpu: Cpu2<NesMemory>,
    // pub cpu: Cpu<NesMemory>,
    pub(crate) cpu: CpuType,

    pub(crate) ppu: Arc<RwLock<Ppu2>>,

    pub(crate) apu: Arc<RwLock<Apu>>,
    pub _rom: Option<Rom>,
    pub config: Config,
    // pub frame: Frame,
    frame_start: Instant,
    // Used to measure and display the FPS
    pub frame_stats: Vec<FrameStat>,
    _frame_stats_last: Instant,
    // Used to count the FPS to pace it
    pub frame_count: Vec<FrameStat>,
    pub frame_count_last: Instant,
    _fps: u16,
    _joypad: Joypad,
    _shared_state: Arc<RwLock<SharedState>>,
    /// Bounded queue
    pub sound_samples: Vec<f32>,
}

impl Emulator {
    pub fn new(rom_info: RomInfo,
        shared_state: Arc<RwLock<SharedState>>, joypad: Arc<RwLock<Joypad>>, args: Args,
        existing_apu: Option<Arc<RwLock<Apu>>>)
        -> Self
    {
        shared_state.write().unwrap().rom_name = rom_info.name();
        let rom = Rom::read_nes_file(&rom_info.file_name()).unwrap();
        let home_dir = std::env::home_dir().unwrap();
        let home_dir = home_dir.to_str().unwrap();
        // let labels =
        //     Labels::from_file(&format!("{home_dir}\\rust\\sixty.rs\\nes\\AccuracyCoin.fns"))
        //         .unwrap();
        let labels = Labels::default();
        // [
        //     (0x2000, "PpuControl_2000"),
        //     (0x2001, "PpuMask_2001"),
        //     (0x2002, "PpuStatus_2002"),
        //     (0x2003, "OamAddr_2003"),
        //     (0x2004, "OamData_2004"),
        //     (0x2005, "PpuScroll_2005"),
        //     (0x2006, "PpuAddr_2006"),
        //     (0x2007, "PpuData_2007"),
        //     (0x2008, "PpuAddr_2008"),
        //     (0x4000, "Sq0Duty_4000"),
        //     (0x4010, "DmcFreq_4010"),
        //     (0x4014, "SpriteDma_4014"),
        //     (0x4015, "ApuStatus_4015"),
        //     (0x4016, "Ctrl1_4016"),
        //     (0x4017, "Ctrl2_FrameCtr_4017"),
        //     (0xf916, "WaitForVBlank"),
        // ].iter().for_each(|(k, v)| {
        //     let _ = labels.insert(*k as u16, (*v).into());
        // });
        let config = Config {
            emulator_speed_hz: 16_000_000,
            debug_asm: DEBUG_ASM,
            pc_max: None,
            trace_to_file: None,
            asynchronous_logging: cpu::cpu::LOG_ASYNC,
            trace_file_asm: format!("{home_dir}\\t\\trace.txt"),
            labels,
            system: System::Nes,
            ..Default::default()
        };
        let mut mapper = MapperBase::new(&rom);
        let ppu = Arc::new(RwLock::new(Ppu2::new(&mut mapper)));
        // Reuse the existing APU (and its audio device) when rebooting so we never
        // tear down / recreate the rodio stream, which would cause a click.
        let apu = if let Some(existing) = existing_apu {
            existing.write().unwrap().reset_state();
            existing
        } else {
            Arc::new(RwLock::new(Apu::new()))
        };
        let len = rom.prg_rom.len();
        debug!(target: "rom", "prg_rom length: {len:04X}");
        let irq = ((rom.prg_rom[len - 1] as u16) << 8) | rom.prg_rom[len - 2] as u16;
        let pc = if let Some(pc) = &args.pc {
            u16::from_str_radix(pc, 16).expect("Failed to parse PC as hexadecimal")
        } else {
            ((rom.prg_rom[len - 3] as u16) << 8) | rom.prg_rom[len - 4] as u16
        };
        let nmi = ((rom.prg_rom[len - 5] as u16) << 8) | rom.prg_rom[len - 6] as u16;
        debug!(target: "rom", "IRQ:{irq:04X} RESET:{pc:04X} NMI:{nmi:04X}");
        let mut nes_memory = NesMemory::new(mapper, joypad.clone(), ppu.clone(), apu.clone());
        nes_memory.init = false;
        let logger: Option<Box<dyn IExternalLogger>> = if DEBUG_MESEN {
            Some(Box::new(MesenLogger::default()))
        } else {
            Some(Box::new(DefaultLogger::default()))
        };
        // Old
        let mut cpu = if CPU_TYPE_NEW { CpuType::New(Cpu2::new(nes_memory, &config, logger)) }
            else { CpuType::Old(Cpu::new(nes_memory, None, &config, logger))};
        // let mut cpu = Cpu::new(nes_memory, None, &config, logger);
        // let mut cpu = Cpu2::new(nes_memory); // , None, &config, logger);
        // New
        cpu.set_pc(pc);
        cpu.set_s(0xfd);

        let ppu2 = ppu.clone();
        let apu2 = apu.clone();
        Self {
            cpu,
            ppu: ppu2,

            apu: apu2,
            _rom: Some(rom),
            config,
            // frame: Frame::default(),
            frame_start: Instant::now(),
            frame_stats: Vec::new(),
            _frame_stats_last: Instant::now(),
            frame_count: Vec::new(),
            frame_count_last: Instant::now(),
            _fps: 0,
            _joypad: Joypad::new(),
            _shared_state: shared_state,
            sound_samples: Vec::new(),
        }
    }

    /// (cycles, frame_completed)
    pub fn tick(&mut self) -> (u128, bool) {
        let mut cycles = 0;
        let mut frame_completed = false;
        let mut i = 0;
        while i < TICK_BATCH_CYCLES && ! frame_completed {
            let (_, c, frame_done) = self.tick_one();
            cycles +=c;
            frame_completed |= frame_done;
            i += 1;
        }
        (cycles, frame_completed)
    }

    // pub fn _tick_one(&mut self) -> u128 {
    //     let mut result = 0;
    //     while self.cpu.wait_cycles != 0 {
    //         // println!("WAIT: {}", self.cpu.wait_cycles);
    //         self.tick_one_cycle();
    //         result += 1;
    //     }
    //     self._tick_one();
    //     result
    // }

    fn is_rendering_enabled(&mut self) -> bool {
        let mask = self.cpu.memory().ppu_mask;
        mask.sprite_rendering() && mask.background_rendering()
    }

    /// (has_advanced, cycle_count, frame_completed)
    pub fn tick_one(&mut self) -> (bool, u128, bool) {

        // New
        // let cycles = self.cpu.cycles;
        // Old
        // let cycles = self.cpu.run_status.cycles();
        // let new_cycles = CYCLES.read().unwrap().add(cycles as u128);
        // *CYCLES.write().unwrap() = new_cycles;

        let mut frame_completed = false;

        //
        // Tick the PPU three times
        for _ in 1..=3 {
            // Read rendering flags fresh each dot so that a ppu_mask.tick() inside
            // ppu.tick() that flips the effective state is visible for the very next dot.
            let sprite_rendering = self.cpu.memory().ppu_mask.sprite_rendering();
            let background_rendering = self.cpu.memory().ppu_mask.background_rendering();
            // info!("PPU TICK");
            let PpuResult { vbl, frame_start, frame_end, irq_requested } =
                self.ppu.write().unwrap().tick(sprite_rendering, background_rendering,
                    &mut self.cpu.memory());
            if vbl && self.cpu.memory().is_vbl_enabled() {
                // DEBUG TEXT
                // self.display_vram();
                self.cpu.nmi();
            }
            if irq_requested {
                self.cpu.irq();
            }
            if frame_start {
                self.frame_start = Instant::now();
            }
            if frame_end {
                frame_completed = true;
                self.frame_stats.push(FrameStat {
                    _duration_ms: self.frame_start.elapsed().as_millis() as u16
                });
                self.frame_count.push(FrameStat {
                    _duration_ms: self.frame_start.elapsed().as_millis() as u16
                });
                self.sound_samples.append(
                    &mut self.apu.write().unwrap().flush_samples()
                );
            }
        }

        // Old
        // (has_advanced, self.cpu.run_status.cycles())
        // New

        //
        // Tick the APU once
        //
        let is_get = self.cpu.is_get_phase();
        let irq_requested = self.apu.write().unwrap().step(self.cpu.memory(), is_get);
        if irq_requested {
            self.cpu.irq();
        }

        //
        // Tick the CPU once
        //
        // Old
        // let has_advanced = self.cpu.one_cycle(&mut self.config, &HashSet::new());
        // New
        let (has_advanced, _cycles) = self.cpu.one_cycle(&mut self.config, &HashSet::new());
        if has_advanced {
            *LOG_SCANLINE.write().unwrap() = *CURRENT_SCANLINE.read().unwrap();
            *LOG_CYCLE.write().unwrap() = *CURRENT_CYCLE.read().unwrap();
        }
        let rendering_enabled = self.is_rendering_enabled();
        if self.cpu.memory().pause_cpu_for_dma {
            let cycles = if (self.cpu.get_cycles() % 2) == 0 { 514 * 3 } else { 513 * 3};
            let mut ppu = self.ppu.write().unwrap();

            for _ in 0..cycles {
                ppu.update_beam(rendering_enabled);
            }
            self.cpu.add_cycles(cycles as u128);
            self.cpu.memory().pause_cpu_for_dma = false;
            // DMA always ends on a PUT
            debug!(target: "asm", "Forcing CPU to PUT");
            self.cpu.set_get_phase(true);
        }

        (has_advanced, 1, frame_completed) // self.cpu.cycles)
    }

    // pub fn display_chr(&mut self, rom: Rom) -> Frame {
    //     let mut result = Frame::default();
    //     let mut y_base = 0;
    //     let mut x_base = 0;
    //     for i in 0..256 {
    //         if (i % 16) == 0 && i > 0 {
    //             y_base += 8;
    //             x_base = 0;
    //         }
    //         // let character = crate::cartridge::pattern_table(&buffer[chr_rom_offset..chr_rom_offset + 0x2000], offset);
    //         for y in 0..8 {
    //             for x in 0..8 {
    //                 let color = rom.get_background_pattern(false, i, x, y);
    //                 let color = to_color_rgb(color);
    //                 let xx = x + x_base;
    //                 let yy = y + y_base;
    //                 // info!("Setting pixel {xx},{yy}");
    //                 result.set_pixel(xx, yy, color);
    //                 // frame.set_pixel(xx, yy, color); // display_chr
    //             }
    //         }
    //         x_base += 8;
    //         // crate::cartridge::display_character(&character);
    //     }
    //
    //     result
    // }

    pub fn _set_rom(&mut self, rom: Option<Rom>) {
        self._rom = rom;
    }

    fn displayable_character(byte: u8) -> String {
        let c = byte as char;
        if c == ' ' || c.is_ascii_alphanumeric() || c.is_ascii_punctuation() {
            c
        } else {
            '.'
        }.into()
    }

    fn line(&mut self, address: u16, values: &[u8]) -> String {
        let mut line: String = "".into();
        line.push_str(&format!("{address:04X}: "));
        for i in 0..16 {
            line.push_str(&format!("{:02X} ", values[i]));
        }
        line.push_str("  ");
        for i in 0..16 {
            line.push_str(&format!("{}",
                Self::displayable_character(values[i])));
        }
        line.push_str("\n");
        line
    }

    pub(crate) fn debug(&mut self) {
        let mut line: String = "".into();

        {
            let mut i: u32 = 0;
            line.push_str("\nCPU Memory\n");
            line.push_str("==========\n");
            while i <= 0xffff {
                let mut values = Vec::new();
                for a in 0..16 {
                    values.push(self.cpu.memory().get(i as u16 + a as u16));
                }
                line.push_str(&self.line(i as u16, &values));
                i += 16;
            }
        }

        {
            let mut i: u32 = 0;
            line.push_str("\nPPU Memory\n");
            line.push_str("==========\n");
            while i <= 0x3fff {
                let mut values = Vec::new();
                for a in 0..16 {
                    values.push(self.ppu.read().unwrap()
                        .get_vram((i as u16 + a as u16) as usize, &mut self.cpu.memory().mapper));
                }

                line.push_str(&self.line(i as u16, &values));
                i += 16;
            }
        }

        {
            let mut i: u32 = 0;
            line.push_str("\nOAM\n");
            line.push_str("===\n");
            while i <= 0xff {
                let mut values = Vec::new();
                for a in 0..16 {
                    values.push(self.ppu.read().unwrap().oam[a + i as usize]);
                }
                line.push_str(&self.line(i as u16, &values));
                i += 16;
            }
        }

        let home_dir = std::env::home_dir().unwrap();
        let home_dir = home_dir.to_str().unwrap();
        let file = &format!("{home_dir}\\t\\debug.txt");
        let _ = std::fs::write(file, line);
        info!("Wrote {file}");
    }
}

pub struct SharedState {
    pub title: String,
    pub _joypad1: String,
    pub rom_name: String,
    pub paused_cycle_count: u128,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            title: String::from(WINDOW_TITLE),
            _joypad1: String::from("Joypad 1"),
            rom_name: "".into(),
            paused_cycle_count: 0,
        }
    }
}

pub fn launch_emulator(args: Args, mut rom_info: RomInfo,
                       sender: Sender<ToUiMessage>, mut receiver: Receiver<ToEmulatorMessage>) ->
                       (Arc<RwLock<SharedState>>, Arc<RwLock<Joypad>>)
{
    let shared_state = Arc::new(RwLock::new(SharedState::default()));
    let shared_state2 = shared_state.clone();
    let fm2_movie = if let Some(fm2_file) = &args.fm2 {
        let movie = Fm2Movie::parse_file(fm2_file).unwrap();
        let rom = Rom::read_nes_file(&rom_info.file_name()).unwrap();
        if let Some(false) = movie.rom_checksum_matches(&rom) {
            warn!(
                target: "fm2",
                rom = %rom_info.file_name(),
                rom_checksum = %rom.checksum,
                movie_checksum = %movie.rom_checksum().unwrap_or_default(),
                "FM2 movie ROM checksum does not match the loaded ROM"
            );
        }
        Some(movie)
    } else {
        None
    };

    println!("Current directory: {}", std::env::current_dir().unwrap().display());
    let mut bk2_movie = if let Some(bk2_file) = &args.bk2 {
        let movie = Bk2Movie::parse_file(bk2_file).expect(&format!("File {bk2_file} should exist"));
        Some(movie)
    } else {
        None
    };

    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let joypad2 = joypad.clone();
    let _ = thread::Builder::new()
        .name("NES emulator thread".to_string())
        .spawn(move|| {
            let mut reboot = false;
            let mut paused = false;
            let mut gilrs = Gilrs::new().ok();
            // Keep the APU alive across reboots so we never recreate the audio device.
            let mut carry_apu: Option<Arc<RwLock<crate::apu::Apu>>> = None;
            loop {
                let mut emulator = Emulator::new(rom_info.clone(),
                                                 shared_state.clone(), joypad2.clone(), args.clone(), carry_apu.take());

                if let (Some(movie), Some(rom)) = (fm2_movie.as_ref(), emulator._rom.as_ref()) {
                    if let Some(false) = movie.rom_checksum_matches(rom) {
                        warn!(
                        target: "fm2",
                        rom = %rom_info.file_name(),
                        rom_checksum = %rom.checksum,
                        movie_checksum = %movie.rom_checksum().unwrap_or_default(),
                        "FM2 movie ROM checksum does not match the loaded ROM"
                    );
                    }
                }

                // Apply persisted channel toggles for each fresh emulator instance (including reboots).
                if let Ok(cfg) = EmulatorConfig::read_or_create() {
                    let mut apu = emulator.apu.write().unwrap();
                    apu.set_sound_enabled(cfg.sound_all_enabled);
                    apu.set_pulse1_enabled(cfg.sound_pulse1_enabled);
                    apu.set_pulse2_enabled(cfg.sound_pulse2_enabled);
                    apu.set_triangle_enabled(cfg.sound_triangle_enabled);
                    apu.set_noise_enabled(cfg.sound_noise_enabled);
                    apu.set_dmc_enabled(cfg.sound_dmc_enabled);
                }

                let mut one_second_start = Instant::now();
                let mut sound_flush_start = Instant::now();
                let mut one_second_cycles = 0;

                while ! reboot {
                    while let Ok(m) = receiver.try_recv() {
                        match m {
                            ToEmulatorMessage::Reboot(ri) => {
                                info!("Emulator rebooting with {ri:#?})");
                                reboot = true;
                                paused = false;
                                rom_info = ri;
                            }
                            ToEmulatorMessage::_SaveState => {
                                info!("Save state requested");
                            }
                            ToEmulatorMessage::_RestoreState => {
                                info!("Restore state requested");
                            }
                            ToEmulatorMessage::Pause(value) => {
                                if paused != value {
                                    paused = value;
                                    one_second_cycles = 0;
                                    one_second_start = Instant::now();
                                    sound_flush_start = Instant::now();
                                    emulator.frame_stats.clear();
                                    emulator.frame_count.clear();
                                    emulator.frame_count_last = Instant::now();

                                    if let Ok(mut state) = shared_state.write() {
                                        if paused {
                                            state.paused_cycle_count = emulator.cpu.get_cycles();
                                            state.title = format!(
                                                "{} - Paused - Cycle {} - {}",
                                                WINDOW_TITLE,
                                                state.paused_cycle_count,
                                                state.rom_name
                                            );
                                        } else {
                                            state.title = format!("{} - {}", WINDOW_TITLE, state.rom_name);
                                        }
                                    }
                                }
                            }
                            ToEmulatorMessage::_Debug => {
                                emulator.debug();
                            }
                            ToEmulatorMessage::SoundAll(enabled) => {
                                emulator.apu.write().unwrap().set_sound_enabled(enabled);
                            }
                            ToEmulatorMessage::SoundPulse1(enabled) => {
                                emulator.apu.write().unwrap().set_pulse1_enabled(enabled);
                            }
                            ToEmulatorMessage::SoundPulse2(enabled) => {
                                emulator.apu.write().unwrap().set_pulse2_enabled(enabled);
                            }
                            ToEmulatorMessage::SoundTriangle(enabled) => {
                                emulator.apu.write().unwrap().set_triangle_enabled(enabled);
                            }
                            ToEmulatorMessage::SoundNoise(enabled) => {
                                emulator.apu.write().unwrap().set_noise_enabled(enabled);
                            }
                            ToEmulatorMessage::SoundDmc(enabled) => {
                                emulator.apu.write().unwrap().set_dmc_enabled(enabled);
                            }
                        }
                    }

                    if let Some(g) = gilrs.as_mut() {
                        while let Some(event) = g.next_event() {
                            match event.event {
                                EventType::ButtonPressed(button, _) => {
                                    if let Some(mapped) = map_gilrs_button(button) {
                                        joypad2.write().unwrap().set_button_status(mapped, true);
                                    }
                                }
                                EventType::ButtonReleased(button, _) => {
                                    if let Some(mapped) = map_gilrs_button(button) {
                                        joypad2.write().unwrap().set_button_status(mapped, false);
                                    }
                                }
                                EventType::AxisChanged(Axis::LeftStickX, value, _) => {
                                    let mut joypad = joypad2.write().unwrap();
                                    apply_stick_x(&mut joypad, value);
                                }
                                EventType::AxisChanged(Axis::LeftStickY, value, _) => {
                                    let mut joypad = joypad2.write().unwrap();
                                    apply_stick_y(&mut joypad, value);
                                }
                                _ => {}
                            }
                        }
                    }

                    if reboot {
                        continue;
                    }

                    if paused {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }

                    let (cycles, frame_completed) = emulator.tick();
                    one_second_cycles += cycles;

                    // fm2
                    if let Some(movie) = &mut bk2_movie {
                        if frame_completed {
                            if let Some(frame) = movie.next_state() {
                                // Apply full controller state every frame so releases are honored.
                                // Using direct button mapping avoids stick helper deadzone/inversion logic.
                                let mut joypad = joypad2.write().unwrap();
                                joypad.set_button_status(Button::Up, frame.up);
                                joypad.set_button_status(Button::Down, frame.down);
                                joypad.set_button_status(Button::Left, frame.left);
                                joypad.set_button_status(Button::Right, frame.right);
                                joypad.set_button_status(Button::A, frame.a);
                                joypad.set_button_status(Button::B, frame.b);
                                joypad.set_button_status(Button::Start, frame.start);
                                joypad.set_button_status(Button::Select, frame.select);

                                if !frame.is_empty() {
                                    info!("Event: {}", frame);
                                }
                            }
                        }
                    }

                    let elapsed = one_second_start.elapsed().as_millis();
                    let sound_elapsed = sound_flush_start.elapsed().as_millis();

                    if sound_elapsed > 100 && !emulator.sound_samples.is_empty() {
                        let samples = std::mem::take(&mut emulator.sound_samples);
                        let _ = sender.send(ToUiMessage::SoundSamples(samples));
                        sound_flush_start = Instant::now();
                    }

                    if elapsed > 1000 {
                        // Refresh the frequency display every second
                        let frames = emulator.frame_stats.len();
                        let frequency = one_second_cycles as f32 / (elapsed as f32 * 1000.0);
                        let _ = sender.send(ToUiMessage::Update(frequency, frames as u16));
                        emulator.frame_stats.clear();
                        one_second_cycles = 0;
                        one_second_start = Instant::now();
                    }

                    if let Some(cap) = CAP_FPS {
                        // If CAP_FPS is set to 60 and the divider is 10, we want
                        // to run 6 (FPS / divider) frames every 100 (1000 / 10) milliseconds
                        // The higher the divider, the smoother the scrolling, up to a point
                        // (if the divider is too high, it makes the emulator uncapped)
                        let divider = 30_u128;
                        // Divider = 10, caps = 40 fps, need to run 4 frames every 100ms
                        let time_wait_ms = 1000 / divider;
                        let frame_cap_divided = cap as u128 / divider;
                        let frame_count = emulator.frame_count.len();
                        // let frame_count_divided = frame_count / divider as usize;
                        // info!("Frame count:{frame_count} time_wait:{time_wait_ms}");
                        if frame_count as u128 >= frame_cap_divided {
                            let elapsed = emulator.frame_count_last.elapsed().as_millis();
                            if elapsed >= time_wait_ms {
                                emulator.frame_count.drain(0..frame_cap_divided as usize);
                                emulator.frame_count_last = Instant::now();
                            } else {
                                let can_sleep_for_video_throttle = emulator.apu.read().unwrap()
                                    .can_sleep_for_video_throttle();

                                if can_sleep_for_video_throttle {
                                    let remaining_ms = (time_wait_ms - elapsed) as u64;
                                    if remaining_ms > 1 {
                                        thread::sleep(Duration::from_millis(1));
                                    } else {
                                        thread::yield_now();
                                    }
                                }
                            }
                        }
                    }

                }
                // Save the APU Arc so the audio device survives the reboot.
                carry_apu = Some(emulator.apu.clone());
                reboot = false;
            }
        });

    (shared_state2, joypad)
}

fn map_gilrs_button(button: GilButton) -> Option<Button> {
    match button {
        GilButton::South => Some(Button::A),
        GilButton::East => Some(Button::B),
        GilButton::Start => Some(Button::Start),
        GilButton::Select => Some(Button::Select),
        _ => None,
    }
}

const STICK_DEADZONE: f32 = 0.25;

fn apply_stick_x(joypad: &mut Joypad, value: f32) {
    if value > STICK_DEADZONE {
        joypad.set_button_status(Button::Right, true);
        joypad.set_button_status(Button::Left, false);
    } else if value < -STICK_DEADZONE {
        joypad.set_button_status(Button::Left, true);
        joypad.set_button_status(Button::Right, false);
    } else {
        joypad.set_button_status(Button::Left, false);
        joypad.set_button_status(Button::Right, false);
    }
}

fn apply_stick_y(joypad: &mut Joypad, value: f32) {
    if value < -STICK_DEADZONE {
        joypad.set_button_status(Button::Down, true);
        joypad.set_button_status(Button::Up, false);
    } else if value > STICK_DEADZONE {
        joypad.set_button_status(Button::Up, true);
        joypad.set_button_status(Button::Down, false);
    } else {
        joypad.set_button_status(Button::Up, false);
        joypad.set_button_status(Button::Down, false);
    }
}

#[derive(Clone)]
pub enum ToUiMessage {
    // Frequency, FPS
    Update(f32, u16),
    SoundSamples(Vec<f32>),
}

#[derive(Clone)]
pub enum ToEmulatorMessage {
    Reboot(RomInfo),
    _SaveState,
    _RestoreState,
    Pause(bool),
    SoundAll(bool),
    SoundPulse1(bool),
    SoundPulse2(bool),
    SoundTriangle(bool),
    SoundNoise(bool),
    SoundDmc(bool),
    _Debug,
}
