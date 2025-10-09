use crate::is_set;
use crate::mappers::mapper::{Mapper};
use crate::mappers::mapper_config::MapperConfig;
use crate::rom::{Rom};
use crate::rom::Mirroring::{ScreenA, ScreenB};

pub struct MapperAxRom;

/// AxRom, Mapper 7, https://www.nesdev.org/wiki/AxROM
impl MapperAxRom {
    pub fn new(_rom: &Rom, config: &mut MapperConfig) -> Self {
        // Banks are 32k
        config.set_prg_bank_size(0x8000);
        config.set_mirroring(ScreenA);
        Self {}
    }
}

impl Mapper for MapperAxRom {
    fn write_prg(&mut self, _addr: u16, data: u8, config: &mut MapperConfig) {
        config.set_prg_bank(0, data as usize);
        config.set_mirroring(if ! is_set!(data, 4) { ScreenA } else { ScreenB })
    }
}