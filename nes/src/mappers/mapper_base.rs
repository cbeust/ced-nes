use tracing::info;
use crate::mappers::mapper::{Mapper};
use crate::mappers::mapper0::Mapper0;
use crate::mappers::mapper_mmc1::MapperMMC1;
use crate::mappers::mapper_uxrom::MapperUxROM;
use crate::mappers::mapper_mmc3::MapperMMC3;
use crate::mappers::mapper_axrom::MapperAxRom;
use crate::mappers::mapper_cnrom::MapperCNRom;
use crate::mappers::mapper_config::MapperConfig;
use crate::nes_memory::NesMemory;
use crate::rom::{Mirroring, Rom, CHR_ROM_SIZE};

/// enum_dispatch fails here because as soon as there is more than one variant,
/// the performances tank. So I have to innline all the mappers manually and then
/// the performance shoots up.
// #[enum_dispatch(Mapper)]
pub struct MapperBase {
    mapper: Box<dyn Mapper>,
    config: MapperConfig,
    chr_ram: Vec<u8>,  // 8KB CHR RAM
    prg_rom: Vec<u8>,
    prg_bank_mask: usize,
    chr_bank_mask: usize,
    // Using these pointers to dispatch memory accesses to either the mapper
    // or the generic MapperBase implementation. Removing branches against the
    // boolean config.is_custom_*** accelerates performances by 3x...
    read_chr_pointer: Box<dyn Fn(&MapperBase, u16) -> u8>,
    read_prg_pointer: Box<dyn Fn(&MapperBase, u16) -> u8>,
}

impl Default for MapperBase {
    fn default() -> Self {
        Self::new(&Rom::default())
    }
}

impl MapperBase {
    fn mapper_number_to_mapper(rom: &Rom, config: &mut MapperConfig) -> Box<dyn Mapper> {
        match rom.mapper {
            0 => { Box::new(Mapper0::new(&rom, config)) }
            1 => { Box::new(MapperMMC1::new(&rom, config)) }
            2 => { Box::new(MapperUxROM::new(&rom, config)) }
            3 => { Box::new(MapperCNRom::new(&rom, config)) }
            4 => { Box::new(MapperMMC3::new(&rom, config)) }
            7 => { Box::new(MapperAxRom::new(&rom, config)) }
            _ => { panic!("Mapper not implemented: {}", rom.mapper) }
        }
    }

    pub fn new(rom: &Rom) -> Self {
        let mut config = MapperConfig::new(rom);

        info!("{} CHR banks size ${:X}, {} PRG banks size ${:X}",
            config.get_chr_bank_count(),
            config.chr_bank_size,
            config.get_prg_bank_count(),
            config.prg_bank_size,
        );
        // info!("rom_len:{} bank_size:{}", rom.prg_rom.len(), config.prg_bank_size);
        let prg_bank_mask = config.prg_bank_size - 1;
        let chr_bank_mask = config.chr_bank_size - 1;
        let mapper = Self::mapper_number_to_mapper(rom, &mut config);

        let read_chr_pointer: Box<dyn Fn(&MapperBase, u16) -> u8> =
            if config.is_custom_chr {
                Box::new(MapperBase::read_chr_mapper)
            } else {
                Box::new(MapperBase::read_chr_direct)
            };

        let read_prg_pointer: Box<dyn Fn(&MapperBase, u16) -> u8> =
            if config.is_custom_prg {
                Box::new(MapperBase::read_prg_mapper)
            } else {
                Box::new(MapperBase::read_prg_direct)
            };

        Self {
            mapper,
            config,
            prg_rom: rom.prg_rom.clone(),
            chr_ram: rom.chr_rom.clone(),
            prg_bank_mask, chr_bank_mask,
            read_chr_pointer,
            // write_chr_pointer,
            read_prg_pointer,
        }
   }
}

impl MapperBase {
    pub fn read_chr(&self, address: u16) -> u8 {
        (self.read_chr_pointer)(self, address)
    }

    pub fn read_chr_direct(&self, address: u16) -> u8 {
        let index = Self::memory_index(CHR_ROM_SIZE - 1, address,
            self.config.chr_bank_size_mask,
            self.config.chr_bank_size_bit,
            &self.config.chr_banks[0..]);

        self.chr_ram[index]
    }

    pub fn read_chr_mapper(&self, address: u16) -> u8 {
        self.mapper.read_chr(address)
    }

    pub fn write_prg(&mut self, addr: u16, data: u8) {
        self.mapper.write_prg(addr, data, &mut self.config);
    }

    pub fn read_prg(&self, address: u16) -> u8 {
        (self.read_prg_pointer)(self, address)
    }

    pub fn read_prg_direct(&self, address: u16) -> u8 {
        let index = Self::memory_index(0x7fff, address,
            self.config.prg_bank_size_mask,
            self.config.prg_bank_size_bit,
            &self.config.prg_banks[0..]);
        self.prg_rom[index]
    }

    pub fn read_prg_mapper(&self, address: u16) -> u8 {
        self.mapper.read_prg(address)
    }

    pub fn write_chr(&mut self, addr: u16, data: u8) {
        if self.config.is_custom_chr {
            self.mapper.write_chr(addr, data);
        } else {
            self.chr_ram[addr as usize] = data;
        }
    }

    pub fn memory_index(address_mask: usize, address: u16,
        mask: usize, bank_size_bit: usize, banks: &[usize]) -> usize
    {
        let masked_address = address as usize & address_mask;
        let result = (banks[masked_address >> bank_size_bit] << bank_size_bit)
            | (masked_address & mask);
        result
    }

    pub fn mirroring(&self) -> Mirroring {
        self.config.mirroring
    }

    pub fn on_scanline(&mut self) -> bool { self.mapper.on_scanline() }

    pub fn nametable_mirroring(&self, address: usize) -> VramType {
        NesMemory::nametable_mirroring(self.mirroring(), address)
    }

}

#[derive(Debug, PartialEq)]
pub enum VramType {
    Vram_A,
    Vram_B,
    Vram,
}