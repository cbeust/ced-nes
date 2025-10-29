use crate::is_set;
use crate::mappers::mapper::{Mapper};
use crate::mappers::mapper_config::MapperConfig;
use crate::rom::{Rom};
use crate::rom::Mirroring::{ScreenA, ScreenB};

pub struct MapperGxRom;

/// GxRom, Mapper 66, https://www.nesdev.org/wiki/GxROM
impl MapperGxRom {
    pub fn new(_rom: &Rom, config: &mut MapperConfig) -> Self {
        // PRG banks are 32k
        config.set_prg_bank_size(0x8000);
        Self {}
    }
}

impl Mapper for MapperGxRom {
    fn write_prg(&mut self, _addr: u16, data: u8, config: &mut MapperConfig) {
        config.set_prg_bank(0, (data as usize & 0x30) >> 4);
        config.set_chr_bank(0, data as usize & 0x3);
    }
}