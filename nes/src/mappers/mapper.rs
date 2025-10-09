use crate::mappers::mapper_config::MapperConfig;
use crate::rom::Mirroring;

// #[enum_dispatch]
pub trait Mapper {
    fn write_prg(&mut self, addr: u16, data: u8, config: &mut MapperConfig);
    fn write_chr(&mut self, _addr: u16, _data: u8) {}
    fn read_chr(&self, _addr: u16) -> u8 { 0 }
    fn read_prg(&self, _addr: u16) -> u8 { 0 }
    fn mirroring(&self) -> Mirroring { Mirroring::Horizontal }
    fn nametable_mirroring(&self, address: usize) -> usize { address }
    /// Return true if IRQ should be triggered
    fn on_scanline(&mut self) -> bool { false }
}

