use crate::constants::{CPU_TYPE_NEW};
use crate::mappers::mapper::Mapper;
use crate::mappers::mapper_config::MapperConfig;
use crate::nes_memory::NesMemory;
use crate::rom::{Mirroring, Rom, PRG_ROM_SIZE};
use tracing::{debug};

/// MMC1, mapper 1
pub struct MapperMMC1 {
    prg_rom: Vec<u8>,
    prg_rom_bank_count: u8,
    chr_rom: Vec<u8>,

    shift_reg: u8,
    shift_count: u8,
    control: u8,
    chr_bank0: u8,
    chr_bank1: u8,
    prg_bank: u8,
    // 0-3
    nametable_arrangement: u8,
    last_cycle_write: u128,
}

impl MapperMMC1 {
    pub fn new(rom: &Rom, config: &mut MapperConfig) -> Self {
        config.set_is_custom_prg(true);
        config.set_is_custom_chr(true);
        let prg_rom_bank_count = (rom.prg_rom.len() / PRG_ROM_SIZE) as u8;
        Self {
            prg_rom: rom.prg_rom.clone(),
            prg_rom_bank_count,
            chr_rom: rom.chr_rom.clone(),
            shift_reg: 0,
            shift_count: 0,
            control: 0xc,
            chr_bank0: 0,
            chr_bank1: 0,
            prg_bank: 0,
            nametable_arrangement: 0,
            last_cycle_write: 0,
        }
    }
}

impl MapperMMC1 {
    fn reset(&mut self) {
        self.shift_reg = 0x10; // (10000b)
        self.shift_count = 0;
        self.control |= 0x0C; // force 16 KB mode as default
    }
}

impl Mapper for MapperMMC1 {
    fn write_prg(&mut self, address: u16, value: u8, config: &mut MapperConfig) {
        if address < 0x8000 { return; }
        if ! CPU_TYPE_NEW {
            // Ignore consecutive writes (e.g. INC)
            // let cycles = *CYCLES.read().unwrap();
            // if self.last_cycle_write == cycles {
            //    return;
            // }
            // self.last_cycle_write = cycles;
        }

        // Reset if bit 7 set
        if value & 0x80 != 0 {
            self.reset();

            debug!(target: "mapper", "M1: Write with 7th bit on, resetting [{:04X}]={:02X}",
                address, value);

            return;
        }

        // if self.shift_count == 0 {
        //     self.current_address = address as usize;
        // } else {
        //     if address as usize != self.current_address {
        //         warn!("Suspicious write to mapper1: {address:04X} != {:04X}",
        //             self.current_address);
        //         println!();
        //         return;
        //     }
        // }
        // Load bit into shift register

        debug!(target: "mapper", "M1: write_prg() #{} Write {value:02X} to {address:04X}",
                self.shift_count);

        let bit = value & 1;
        self.shift_reg = (self.shift_reg >> 1) | (bit << 4);
        // self.shift_reg = (self.shift_reg << 1) | bit;
        self.shift_count += 1;

        // Once 5 writes done
        if self.shift_count == 5 {
            let reg = (address >> 13) & 0b11; // which register
            debug!(target: "mapper", "M1:   5 writes reg:{reg} shift_reg:{:02X}",
                    self.shift_reg);
            match reg {
                0 => {
                    // 4bit0
                    // -----
                    // CPPMM
                    // |||||
                    // |||++- Nametable arrangement: (0: one-screen, lower bank; 1: one-screen, upper bank;
                    // |||               2: horizontal arrangement ("vertical mirroring", PPU A10);
                    // |||               3: vertical arrangement ("horizontal mirroring", PPU A11) )
                    // |++--- PRG-ROM bank mode (0, 1: switch 32 KB at $8000, ignoring low bit of bank number;
                    // |                         2: fix first bank at $8000 and switch 16 KB bank at $C000;
                    // |                         3: fix last bank at $C000 and switch 16 KB bank at $8000)
                    // +----- CHR-ROM bank mode (0: switch 8 KB at a time; 1: switch two separate 4 KB banks)
                    self.control = self.shift_reg;
                    let nametable_arrangement = self.shift_reg & 0b11;
                    self.nametable_arrangement = nametable_arrangement;
                    match nametable_arrangement {
                        0 => config.set_mirroring(Mirroring::ScreenA),
                        1 => config.set_mirroring(Mirroring::ScreenB),
                        2 => config.set_mirroring(Mirroring::Vertical),
                        3 => config.set_mirroring(Mirroring::Horizontal),
                        // 0 | 1 is using 1KB of VRAM for all four screens, so mirroring 4 times in
                        // the address space. 0 is for the lower half, 1 is for the upper half.
                        _ => {}
                    };

                    let prg_mode = (self.shift_reg >> 2) & 0b11;
                    let chr_mode = (self.shift_reg >> 4) & 0b11;
                    debug!(target: "mapper", "M1:  write_prg() New control: ${:02X} nametable:{} \
                    prg_mode:{} chr_mode:{}",
                        self.shift_reg, self.nametable_arrangement, prg_mode, chr_mode);
                }
                1 => {
                    self.chr_bank0 = self.shift_reg;
                    debug!(target: "mapper", "M1:  write_prg() New chr0_bank: {}", self.shift_reg);
                }
                2 => {
                    self.chr_bank1 = self.shift_reg;
                    debug!(target: "mapper", "M1:  write_prg() New chr1_bank: {}", self.shift_reg);
                }
                3 => {
                    self.prg_bank = self.shift_reg;
                    debug!(target: "mapper", "M1:  write_prg() New prg_bank: {}", self.shift_reg);
                }
                _ => {}
            }

            self.reset();
        }
    }

    fn write_chr(&mut self, address: u16, value: u8) {
        let a = self.chr_index(address);
        self.chr_rom[a] = value;
    }

    fn read_chr(&mut self, address: u16) -> u8 {
        self.chr_rom[self.chr_index(address)]
    }

    fn read_prg(&self, address: u16) -> u8 {
        let control = (self.control >> 2) & 0b11;
        let mut bank = match control {
            0 | 1 => (self.prg_bank & 7) as usize,       // 32 KB mode
            2 => if address < 0xc000 { self.prg_bank as usize } else { 0 }, // switch at $8000
            3 => if address < 0xC000 {
                (self.prg_bank & 0xf) as usize
                } else {
                    self.prg_rom_bank_count as usize - 1
                },
            _ => { panic!("Should never happen");}
        };

        bank = bank & (self.prg_rom_bank_count as usize - 1);
        let actual_address = (bank * PRG_ROM_SIZE) + (address & 0x3fff) as usize;

        // if actual_address >= self.prg_rom.len() {
        //     debug!(target: "mapper", "M1: read_prg() {address:04X} control:{} prg_bank:{} bank:{}",
        //         self.control, self.prg_bank, bank);
        //     println!();
        // }

        let result = self.prg_rom[actual_address];
        result
    }
}

impl MapperMMC1 {
    fn chr_index(&self, address: u16) -> usize {
        let addr = NesMemory::ppu_mirrorring(address);
        let bank_mode = (self.control >> 4) & 1; // 0 = 8KB, 1 = 4KB

        if bank_mode == 0 {
            // 8 KB mode
            // Only CHR bank 0 is used, and it selects an 8 KB bank
            let bank = (self.chr_bank0 as usize) & 0x1E; // mask to even, since it's 8 KB
            let offset = (addr as usize) & 0x1FFF;       // 0–8191
            (bank * 0x1000) + offset
        } else {
            // if self.chr_bank0 != 0 || self.chr_bank1 != 0 {
            //     println!("BANK is $1C");
            // }
            let bank = if addr < 0x1000 {
                // 4 KB mode
                // PPU $0000–0FFF → CHR bank 0
                self.chr_bank0 as usize * 0x1000
            } else {
                // PPU $1000–1FFF → CHR bank 1
                self.chr_bank1 as usize * 0x1000
            };
            let offset = (addr as usize) & 0x0FFF;
            bank + offset
        }
    }
}
