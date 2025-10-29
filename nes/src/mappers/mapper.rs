use crate::mappers::mapper_config::MapperConfig;
use crate::rom::Mirroring;

// #[enum_dispatch]
pub trait Mapper {
    fn read_prg(&self, _addr: u16) -> u8 { 0 }
    fn write_prg(&mut self, _addr: u16, _data: u8, _config: &mut MapperConfig) {}
    fn read_chr(&mut self, _addr: u16) -> u8 { 0 }
    fn write_chr(&mut self, _addr: u16, _data: u8) {}
    fn read_nametable(&self, _address: usize) -> u8 { 0 }
    fn write_nametable(&mut self, _address: usize, _value: u8) { }
    fn mirroring(&self) -> Mirroring { Mirroring::Horizontal }
    fn nametable_mirroring(&self, address: usize) -> usize { address }
    /// Return true if IRQ should be triggered
    fn on_scanline(&mut self) -> bool { false }
    /// Return true if IRQ should be triggered
    fn on_cpu_cycle(&mut self) -> bool { false }
    fn on_read_chr(&mut self, _address: u16, _config: &mut MapperConfig) {}
}

