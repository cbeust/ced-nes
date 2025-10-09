use tracing::info;
use crate::constants::DONKEY_KONG;
use crate::mappers::mapper::{Mapper};
use crate::mappers::mapper_config::MapperConfig;
use crate::nes_memory::NesMemory;
use crate::rom::{Mirroring, Rom, CHR_ROM_SIZE};

pub struct Mapper0 {
    prg_rom: Vec<u8>,
    prg_rom_count: u8,
    chr_rom: Vec<u8>,
    mirroring: Mirroring,
}

impl Default for Mapper0 {
    fn default() -> Self {
        Self::new(&Rom::read_nes_file(DONKEY_KONG).unwrap(), &mut MapperConfig::default())
    }
}

impl Mapper0 {
    pub fn new(rom: &Rom, config: &mut MapperConfig) -> Self {
        let prg_rom_count = (rom.prg_rom.len() / 0x4000) as u8;
        if prg_rom_count == 1 {
            // 16KB, mirror 0x8000 - 0xbfff and 0xc000 - 0xffff
            config.set_prg_bank_size(0x4000);
            config.set_prg_bank(0, 0);
            config.set_prg_bank(1, 0);
        } else {
            // 32KB, just one bank 0x8000 - 0xffff
            config.set_prg_bank_size(0x8000);
        }
        let chr_rom = if rom.chr_rom.len() == 0 {
            vec![0; CHR_ROM_SIZE]
        } else {
            rom.chr_rom.clone()
        };
        config.set_mirroring(rom.header.mirroring);
        Self {
            mirroring: rom.header.mirroring,
            prg_rom: rom.prg_rom.clone(),
            prg_rom_count,
            chr_rom,
        }
    }
}

impl Mapper for Mapper0 {
    fn write_prg(&mut self, _addr: u16, _data: u8, config: &mut MapperConfig) {
        // Don't write to ROM
    }

    fn read_prg(&self, addr: u16) -> u8 {
        let subtract =
        if self.prg_rom_count == 2 {
            0x8000
        } else {
            if addr >= 0xc000 {
                0xc000
            } else {
                0x8000
            }
        };
        let result = self.prg_rom[addr as usize - subtract];
        result
    }

    fn nametable_mirroring(&self, address: usize) -> usize {
        NesMemory::ppu_mirrorring(address as u16) as usize
    }
}
