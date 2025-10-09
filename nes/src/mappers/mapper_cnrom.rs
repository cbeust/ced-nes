use tracing::debug;
use crate::mappers::mapper::{Mapper};
use crate::mappers::mapper_config::MapperConfig;
use crate::rom::Rom;

pub struct MapperCNRom;

/// CNRom, Mapper 3
impl MapperCNRom {
    pub fn new(_rom: &Rom, _config: &mut MapperConfig) -> Self {
        Self {}
    }
}

impl Mapper for MapperCNRom {
    fn write_prg(&mut self, addr: u16, data: u8, config: &mut MapperConfig) {
        debug!(target: "mapper", "MCNRom: write_prg [${addr:04X}] = {data:02X}");
        config.set_chr_bank(0, data as usize);
    }
}
