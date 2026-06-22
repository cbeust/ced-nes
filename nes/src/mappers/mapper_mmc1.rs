use crate::constants::{CPU_TYPE_NEW};
use crate::mappers::mapper::Mapper;
use crate::mappers::mapper_config::MapperConfig;
use crate::nes_memory::NesMemory;
use crate::rom::{Mirroring, Rom, PRG_ROM_SIZE};
use tracing::{debug};

/// MMC1, mapper 1
pub struct MapperMMC1 {
    prg_rom: Vec<u8>,
    prg_rom_bank_count: usize,
    chr_rom: Vec<u8>,

    shift_reg: u8,
    shift_count: u8,
    control: u8,
    chr_bank0: u8,
    chr_bank1: u8,
    prg_bank: u8,
    // 0-3
    nametable_arrangement: u8,
}

impl MapperMMC1 {
    pub fn new(rom: &Rom, config: &mut MapperConfig) -> Self {
        config.set_is_custom_prg(true);
        config.set_is_custom_chr(true);
        let prg_rom_bank_count = rom.prg_rom.len() / PRG_ROM_SIZE;
        Self {
            prg_rom: rom.prg_rom.clone(),
            prg_rom_bank_count,
            chr_rom: rom.chr_rom.clone(),
            shift_reg: 0x10,
            shift_count: 0,
            control: 0xc,
            chr_bank0: 0,
            chr_bank1: 0,
            prg_bank: 0,
            nametable_arrangement: 0,
        }
    }
}

impl MapperMMC1 {
    /// Reset triggered by a write with bit 7 set.
    /// NESdev: clear shift register and force PRG mode bits high (|= 0x0C).
    fn reset(&mut self) {
        self.shift_reg = 0x10;
        self.shift_count = 0;
        self.control |= 0x0C;
    }

    /// Clear the 5-bit shift register after latching a value.
    fn clear_shift(&mut self) {
        self.shift_reg = 0x10;
        self.shift_count = 0;
    }
}

impl Mapper for MapperMMC1 {
    fn read_prg(&self, address: u16) -> u8 {
        let prg_mode = (self.control >> 2) & 0b11;
        let bank_count = self.prg_rom_bank_count.max(1);
        let selected_bank = (self.prg_bank & 0x0F) as usize;

        let bank = match prg_mode {
            0 | 1 => {
                // 32 KB mode: ignore low bit, map even bank at $8000 and odd at $C000.
                let bank_lo = (selected_bank & !1) % bank_count;
                if address < 0xC000 {
                    bank_lo
                } else {
                    (bank_lo + 1) % bank_count
                }
            }
            2 => {
                // Fix first bank at $8000, switch bank at $C000.
                if address < 0xC000 {
                    0
                } else {
                    selected_bank % bank_count
                }
            }
            3 => {
                // Switch bank at $8000, fix last bank at $C000.
                if address < 0xC000 {
                    selected_bank % bank_count
                } else {
                    bank_count - 1
                }
            }
            _ => unreachable!(),
        };

        let actual_address = (bank * PRG_ROM_SIZE) + ((address & 0x3fff) as usize);
        self.prg_rom[actual_address]
    }

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
                    let chr_mode = (self.shift_reg >> 4) & 0b1;
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

            self.clear_shift();
        }
    }

    fn read_chr(&mut self, address: u16) -> u8 {
        self.chr_rom[self.chr_index(address)]
    }

    fn write_chr(&mut self, address: u16, value: u8) {
        let a = self.chr_index(address);
        self.chr_rom[a] = value;
    }
}

impl MapperMMC1 {
    fn chr_index(&self, address: u16) -> usize {
        let addr = NesMemory::ppu_mirrorring(address);
        let bank_mode = (self.control >> 4) & 1; // 0 = 8KB, 1 = 4KB
        let chr_4k_bank_count = (self.chr_rom.len() / 0x1000).max(1);

        if bank_mode == 0 {
            // 8 KB mode
            // Only CHR bank 0 is used and bit 0 is ignored.
            let bank = ((self.chr_bank0 as usize) & !1) % chr_4k_bank_count;
            let offset = (addr as usize) & 0x1FFF;       // 0–8191
            (bank * 0x1000) + offset
        } else {
            let bank = if addr < 0x1000 {
                // 4 KB mode
                // PPU $0000–0FFF → CHR bank 0
                (self.chr_bank0 as usize % chr_4k_bank_count) * 0x1000
            } else {
                // PPU $1000–1FFF → CHR bank 1
                (self.chr_bank1 as usize % chr_4k_bank_count) * 0x1000
            };
            let offset = (addr as usize) & 0x0FFF;
            bank + offset
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_mapper(prg_16k_banks: usize) -> MapperMMC1 {
        let mut rom = Rom::default();
        rom.prg_rom = vec![0; PRG_ROM_SIZE * prg_16k_banks];
        for bank in 0..prg_16k_banks {
            let marker = 0xA0 + bank as u8;
            let start = bank * PRG_ROM_SIZE;
            for i in start..start + PRG_ROM_SIZE {
                rom.prg_rom[i] = marker;
            }
        }
        let mut config = MapperConfig::new(&rom);
        MapperMMC1::new(&rom, &mut config)
    }

    #[test]
    fn mmc1_mode2_maps_fixed_lower_and_switchable_upper() {
        let mut mapper = make_test_mapper(4);
        mapper.control = 0b01000;
        mapper.prg_bank = 2;

        assert_eq!(mapper.read_prg(0x8000), 0xA0);
        assert_eq!(mapper.read_prg(0xC000), 0xA2);
    }

    #[test]
    fn mmc1_mode3_maps_switchable_lower_and_fixed_last_upper() {
        let mut mapper = make_test_mapper(4);
        mapper.control = 0b01100;
        mapper.prg_bank = 1;

        assert_eq!(mapper.read_prg(0x8000), 0xA1);
        assert_eq!(mapper.read_prg(0xC000), 0xA3);
    }

    #[test]
    fn mmc1_mode0_uses_two_consecutive_16k_banks() {
        let mut mapper = make_test_mapper(4);
        mapper.control = 0b00000;
        mapper.prg_bank = 3; // low bit ignored => banks 2 and 3

        assert_eq!(mapper.read_prg(0x8000), 0xA2);
        assert_eq!(mapper.read_prg(0xC000), 0xA3);
    }

    #[test]
    fn mmc1_successful_latch_does_not_force_mode3() {
        let mut mapper = make_test_mapper(4);
        let mut config = MapperConfig::new(&Rom::default());

        for &bit in &[0u8, 0, 0, 1, 0] {
            mapper.write_prg(0x8000, bit, &mut config);
        }
        assert_eq!((mapper.control >> 2) & 0b11, 2);

        for _ in 0..5 {
            mapper.write_prg(0xE000, 0, &mut config);
        }
        assert_eq!((mapper.control >> 2) & 0b11, 2);
    }

    #[test]
    fn mmc1_reset_write_forces_prg_mode_to_3() {
        let mut mapper = make_test_mapper(2);
        mapper.control = 0;
        let mut config = MapperConfig::new(&Rom::default());
        mapper.write_prg(0x8000, 0x80, &mut config);
        assert_eq!((mapper.control >> 2) & 0b11, 3);
        assert_eq!(mapper.shift_reg, 0x10);
        assert_eq!(mapper.shift_count, 0);
    }
}
