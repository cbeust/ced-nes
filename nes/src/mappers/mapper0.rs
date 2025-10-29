use crate::constants::DONKEY_KONG;
use crate::mappers::mapper::Mapper;
use crate::mappers::mapper_config::MapperConfig;
use crate::rom::Rom;

pub struct Mapper0;

impl Default for Mapper0 {
    fn default() -> Self {
        Self::new(&Rom::read_nes_file(DONKEY_KONG).unwrap(), &mut MapperConfig::default())
    }
}

impl Mapper0 {
    pub fn new(rom: &Rom, config: &mut MapperConfig) -> Self {
        if rom.prg_rom.len() == 0x4000 {
            // 16KB, mirror 0x8000 - 0xbfff and 0xc000 - 0xffff
            config.set_prg_bank_size(0x4000);
            config.set_prg_bank(0, 0);
            config.set_prg_bank(1, 0);
        } else {
            // 32KB, just one bank 0x8000 - 0xffff
            config.set_prg_bank_size(0x8000);
        }
        Self {}
    }
}

impl Mapper for Mapper0 {}