use std::collections::HashSet;
use std::ops::Add;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use lazy_static::lazy_static;
use tracing::{debug, info};
use cpu::config::Config;
use cpu::cpu::Cpu;
use cpu::labels::Labels;
use cpu::memory::Memory;
use crate::app::SharedState;
use crate::Args;
use crate::constants::{RomInfo, DEBUG_ASM, HEIGHT, WIDTH};
use crate::rom::Rom;
use crate::joypad::Joypad;
use crate::mappers::mapper0::Mapper0;
use crate::mappers::mapper::Mapper;
use crate::mappers::mapper_base::MapperBase;
use crate::nes_memory::NesMemory;
use crate::ppu::{Ppu, PpuResult};

pub struct FrameStat {
    duration_ms: u16,
}

pub static mut FRAME: [u8; WIDTH * HEIGHT] = [0; WIDTH * HEIGHT];

lazy_static! {
    pub static ref CYCLES: Arc<RwLock<u128>> = Arc::new(RwLock::new(0));
}

pub struct Emulator {
    pub cpu: Cpu<NesMemory>,
    pub(crate) ppu: Arc<RwLock<Ppu>>,
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
        let labels =
            Labels::from_file(&format!("{home_dir}\\rust\\sixty.rs\\nes\\AccuracyCoin.fns"))
                .unwrap();
        let labels = Labels::default();
        let config = Config {
            emulator_speed_hz: 16_000_000,
            debug_asm: DEBUG_ASM,
            pc_max: None,
            trace_to_file: None,
            asynchronous_logging: false,
            labels,
            ..Default::default()
        };
        let mut mapper = MapperBase::new(&rom);
        let ppu = Arc::new(RwLock::new(Ppu::new(&mut mapper)));
        let len = rom.prg_rom.len();
        debug!(target: "rom", "prg_rom length: {len:04X}");
        let pc = ((rom.prg_rom[len - 3] as u16) << 8) | rom.prg_rom[len - 4] as u16;
        let mut nes_memory = NesMemory::new(mapper, joypad.clone(), ppu.clone());
        nes_memory.init = false;
        let mut cpu = Cpu::new(nes_memory, None, config.clone());
        cpu.pc = pc;

        let ppu2 = ppu.clone();
        Self {
            cpu,
            ppu: ppu2,
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
        for _ in 1..1000 {
            cycles += self.tick_one().1;
        }
        // let value: String = format!("{:02X}", self.cpu.memory.joypad.read().unwrap().read_status());
        // (*self.shared_state.write().unwrap()).joypad1 = value;
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

    pub fn tick_one(&mut self) -> (bool, u128) {
        //
        // Tick the CPU once
        //
        // self.cpu.step(&self.config, &HashSet::new());
        let has_advanced = self.cpu.one_cycle(&self.config, &HashSet::new());
        {
            let mut ppu = self.ppu.write().unwrap();
            ppu.ppu_ctrl = self.cpu.memory.ppu_ctrl;
        }

        let cycles = self.cpu.run_status.cycles();
        // let cycles = 1;
        let new_cycles = CYCLES.read().unwrap().add(cycles);
        *CYCLES.write().unwrap() = new_cycles;

        //
        // Tick the PPU three times
        let sprite_rendering = self.cpu.memory.ppu_mask.sprite_rendering();
        let background_rendering = self.cpu.memory.ppu_mask.background_rendering();
        for _ in 1..=cycles * 3 {
            // self.cpu.memory.ppu_mask.update_rendering_counts();
            // info!("PPU TICK");
            let PpuResult { vbl, frame_start, frame_end, irq_requested } =
                self.ppu.write().unwrap().tick(sprite_rendering, background_rendering,
                    &mut self.cpu.memory);
            if vbl && self.cpu.memory.is_vbl_enabled() {
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
            }
        }

        (has_advanced, self.cpu.run_status.cycles())
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
                    values.push(self.cpu.memory.get(i as u16 + a as u16));
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
                        .get_vram((i as u16 + a as u16) as usize, &self.cpu.memory.mapper));
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
