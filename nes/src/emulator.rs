use std::collections::HashSet;
use std::ops::Add;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use enum_dispatch::enum_dispatch;
use lazy_static::lazy_static;
use tracing::{debug, info};
use cpu::config::Config;
use cpu::cpu2::Cpu2;
use cpu::cpu::{Cpu};
use cpu::external_logger::{DefaultLogger, IExternalLogger};
use cpu::labels::Labels;
use cpu::memory::Memory;
use crate::app::SharedState;
use crate::apu::Apu;
use crate::Args;
use crate::constants::{RomInfo, CPU_TYPE_NEW, DEBUG_ASM, DEBUG_MESEN, HEIGHT, WIDTH};
use crate::rom::Rom;
use crate::joypad::Joypad;
use crate::mappers::mapper_base::MapperBase;
use crate::mesen_logger::{MesenLogger, LOG_CYCLE, LOG_SCANLINE};
use crate::nes_memory::NesMemory;
use crate::ppu::{Ppu, PpuResult, CURRENT_CYCLE, CURRENT_SCANLINE};

pub struct FrameStat {
    duration_ms: u16,
}

pub static mut FRAME: [u8; WIDTH * HEIGHT] = [0; WIDTH * HEIGHT];

lazy_static! {
    pub static ref CYCLES: Arc<RwLock<u128>> = Arc::new(RwLock::new(0));
}

enum CpuType {
    Old(Cpu<NesMemory>),
    New(Cpu2<NesMemory>),
}

impl CpuType {
    pub fn set_pc(&mut self, pc: u16) {
        match self {
            CpuType::Old(cpu) => cpu.set_pc(pc),
            CpuType::New(cpu) => cpu.set_pc(pc),
        }
    }

    pub fn set_s(&mut self, v: u8) {
        match self {
            CpuType::Old(cpu) => { cpu.s = v; }
            CpuType::New(cpu) => { cpu.s = v; }
        }
    }

    pub(crate) fn nmi(&mut self) {
        match self {
            CpuType::Old(cpu) => cpu.nmi(),
            CpuType::New(cpu) => cpu.nmi(),
        }
    }

    pub(crate) fn irq(&mut self) {
        match self {
            CpuType::Old(cpu) => cpu.irq(),
            CpuType::New(cpu) => cpu.irq(),
        }
    }

    pub(crate) fn get_cycles(&mut self) -> u128 {
        match self {
            CpuType::Old(cpu) => cpu.cycles,
            CpuType::New(cpu) => cpu.cycles,
        }
    }

    pub(crate) fn add_cycles(&mut self, cycles: u128) {
        match self {
            CpuType::Old(cpu) => { cpu.cycles += cycles }
            CpuType::New(cpu) => { cpu.cycles += cycles }
        }
    }

    pub fn one_cycle(&mut self, config: &mut Config,
                     breakpoints: &HashSet<u16>) -> (bool, u8) {
        match self {
            CpuType::Old(cpu) => cpu.one_cycle(config, breakpoints),
            CpuType::New(cpu) => cpu.one_cycle(config, breakpoints),
        }
    }

    pub fn memory(&mut self) -> &mut NesMemory {
        match self {
            CpuType::Old(c) => { &mut c.memory }
            CpuType::New(c) => { &mut c.memory }
        }
    }
}

pub struct Emulator {
    // New
    // pub cpu: Cpu2<NesMemory>,
    // pub cpu: Cpu<NesMemory>,
    pub cpu: CpuType,
    pub(crate) ppu: Arc<RwLock<Ppu>>,
    pub(crate) apu: Arc<RwLock<Apu>>,
    pub rom: Option<Rom>,
    pub config: Config,
    // pub frame: Frame,
    frame_start: Instant,
    // Used to measure and display the FPS
    pub frame_stats: Vec<FrameStat>,
    frame_stats_last: Instant,
    // Used to count the FPS to pace it
    pub frame_count: Vec<FrameStat>,
    pub frame_count_last: Instant,
    fps: u16,
    joypad: Joypad,
    shared_state: Arc<RwLock<SharedState>>
}

impl Emulator {
    pub fn new(rom_info: RomInfo,
        shared_state: Arc<RwLock<SharedState>>, joypad: Arc<RwLock<Joypad>>, _args: Args)
        -> Self
    {
        shared_state.write().unwrap().rom_name = rom_info.name();
        let rom = Rom::read_nes_file(&rom_info.file_name()).unwrap();
        let home_dir = std::env::home_dir().unwrap();
        let home_dir = home_dir.to_str().unwrap();
        // let labels =
        //     Labels::from_file(&format!("{home_dir}\\rust\\sixty.rs\\nes\\AccuracyCoin.fns"))
        //         .unwrap();
        let mut labels = Labels::default();
        [
            (0x2000, "PpuControl_2000"),
            (0x2001, "PpuMask_2001"),
            (0x2002, "PpuStatus_2002"),
            (0x2003, "OamAddr_2003"),
            (0x2004, "OamData_2004"),
            (0x2005, "PpuScroll_2005"),
            (0x2006, "PpuAddr_2006"),
            (0x2007, "PpuData_2007"),
            (0x2008, "PpuAddr_2008"),
            (0x4000, "Sq0Duty_4000"),
            (0x4010, "DmcFreq_4010"),
            (0x4014, "SpriteDma_4014"),
            (0x4015, "ApuStatus_4015"),
            (0x4016, "Ctrl1_4016"),
            (0x4017, "Ctrl2_FrameCtr_4017")
        ].iter().for_each(|(k, v)| {
            let _ = labels.insert(*k as u16, (*v).into());
        });
        let config = Config {
            emulator_speed_hz: 16_000_000,
            debug_asm: DEBUG_ASM,
            pc_max: None,
            trace_to_file: None,
            asynchronous_logging: cpu::cpu::LOG_ASYNC,
            trace_file_asm: format!("{home_dir}\\t\\trace.txt"),
            labels,
            ..Default::default()
        };
        let mut mapper = MapperBase::new(&rom);
        let ppu = Arc::new(RwLock::new(Ppu::new(&mut mapper)));
        let apu = Arc::new(RwLock::new(Apu::new()));
        let len = rom.prg_rom.len();
        debug!(target: "rom", "prg_rom length: {len:04X}");
        let irq = ((rom.prg_rom[len - 1] as u16) << 8) | rom.prg_rom[len - 2] as u16;
        let pc = ((rom.prg_rom[len - 3] as u16) << 8) | rom.prg_rom[len - 4] as u16;
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
            rom: Some(rom),
            config,
            // frame: Frame::default(),
            frame_start: Instant::now(),
            frame_stats: Vec::new(),
            frame_stats_last: Instant::now(),
            frame_count: Vec::new(),
            frame_count_last: Instant::now(),
            fps: 0,
            joypad: Joypad::new(),
            shared_state,
        }
    }

    pub fn tick(&mut self) -> u128 {
        let mut cycles = 0;
        for _ in 1..10_000 {
            cycles += self.tick_one().1;
        }
        cycles
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

    pub fn tick_one(&mut self) -> (bool, u128) {

        // New
        // let cycles = self.cpu.cycles;
        // Old
        // let cycles = self.cpu.run_status.cycles();
        // let new_cycles = CYCLES.read().unwrap().add(cycles as u128);
        // *CYCLES.write().unwrap() = new_cycles;

        //
        // Tick the PPU three times
        let sprite_rendering = self.cpu.memory().ppu_mask.sprite_rendering();
        let background_rendering = self.cpu.memory().ppu_mask.background_rendering();
        for _ in 1..=3 {
            // self.cpu.memory().ppu_mask.update_rendering_counts();
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
                self.frame_stats.push(FrameStat {
                    duration_ms: self.frame_start.elapsed().as_millis() as u16
                });
                self.frame_count.push(FrameStat {
                    duration_ms: self.frame_start.elapsed().as_millis() as u16
                });
                self.apu.write().unwrap().flush_samples();
            }
        }

        // Old
        // (has_advanced, self.cpu.run_status.cycles())
        // New

        //
        // Tick the APU once
        //
        self.apu.write().unwrap().step();

        //
        // Tick the CPU once
        //
        // Old
        // let has_advanced = self.cpu.one_cycle(&mut self.config, &HashSet::new());
        // New
        let (has_advanced, cycles) = self.cpu.one_cycle(&mut self.config, &HashSet::new());
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
        }

        {
            let mut ppu = self.ppu.write().unwrap();
            ppu.ppu_ctrl = self.cpu.memory().ppu_ctrl;
        }

        (has_advanced, 1) // self.cpu.cycles)
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

    pub fn set_rom(&mut self, rom: Option<Rom>) {
        self.rom = rom;
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
