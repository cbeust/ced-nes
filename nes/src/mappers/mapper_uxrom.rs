use crate::mappers::mapper::{Mapper};
use crate::mappers::mapper_config::MapperConfig;
use crate::rom::{Rom};

/// UxRom, Mapper 2
/// https://www.nesdev.org/wiki/UxROM
pub struct MapperUxROM;

impl MapperUxROM {
    pub fn new(_rom: &Rom, config: &mut MapperConfig) -> Self {
        config.set_prg_bank_size(0x4000);
        config.set_prg_bank(0, 0);
        config.set_prg_bank(1, config.get_prg_bank_count() - 1);
        Self {}
    }
}

impl Mapper for MapperUxROM {
    fn write_prg(&mut self, _addr: u16, data: u8, config: &mut MapperConfig) {
        config.set_prg_bank(0, data as usize);
    }
}