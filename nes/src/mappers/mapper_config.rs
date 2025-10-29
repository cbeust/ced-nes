use std::arch::x86_64::_popcnt32;
use tracing::info;
use crate::rom::{Mirroring, Rom};


pub struct MapperConfig {
    pub(crate) total_prg_rom_size: usize,
    pub(crate) total_chr_rom_size: usize,
    pub(crate) chr_bank_size: usize,
    pub(crate) chr_bank_size_mask: usize,
    pub(crate) chr_bank_size_bit: usize,
    pub(crate) prg_bank_size: usize,
    pub(crate) prg_bank_size_mask: usize,
    pub(crate) prg_bank_size_bit: usize,
    pub(crate) prg_banks: Vec<usize>,
    pub(crate) chr_banks: Vec<usize>,
    pub(crate) mirroring: Mirroring,
    pub(crate) is_custom_chr: bool,
    pub(crate) is_custom_prg: bool,
    pub(crate) is_custom_nametable: bool,
    pub(crate) on_read_chr_hook: bool,
}

impl Default for MapperConfig {
    fn default() -> Self {
        Self {
            total_prg_rom_size: 0,
            total_chr_rom_size: 0,
            chr_bank_size: 0x2000,
            chr_bank_size_mask: 0x1fff,
            chr_bank_size_bit: 13,
            prg_bank_size: 0x8000,
            prg_bank_size_mask: 0x7fff,
            prg_bank_size_bit: 15,
            prg_banks: vec![0; 256],
            chr_banks: vec![0; 256],
            mirroring: Mirroring::Horizontal,
            is_custom_chr: false,
            is_custom_prg: false,
            is_custom_nametable: false,
            on_read_chr_hook: false,
        }
    }
}

impl MapperConfig {
    pub fn new(rom: &Rom) -> Self {
        let mut result = Self::default();
        result.total_prg_rom_size = rom.prg_rom.len();
        result.total_chr_rom_size = rom.chr_rom.len();
        result.mirroring = rom.header.mirroring;
        result
    }

    /// Typically 0x800 or 0x2000
    pub fn set_chr_bank_size(&mut self, size: usize) {
        self.chr_bank_size = size;
        self.chr_bank_size_bit = (size - 1).count_ones() as usize;
        self.chr_bank_size_mask = size - 1;
        // info!("bank_size:{size:4X} bit:{:4X} mask:{:4X}",
        //     self.chr_bank_size_bit, self.chr_bank_size_mask);
    }

    /// Typically 0x4000 or 0x8000
    pub fn set_prg_bank_size(&mut self, size: usize) {
        self.prg_bank_size = size;
        self.prg_bank_size_bit = (size - 1).count_ones() as usize;
        self.prg_bank_size_mask = size - 1;
    }

    pub fn set_chr_bank(&mut self, bank_index: usize, bank: usize) {
        let bank2 = bank & (self.get_chr_bank_count() - 1);
        self.chr_banks[bank_index] = bank2;
    }

    pub fn set_prg_bank(&mut self, bank_index: usize, bank: usize) {
        let bank2 = bank & (self.get_prg_bank_count()  - 1);
        self.prg_banks[bank_index] = bank2;
    }

    pub fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }

    pub fn set_is_custom_chr(&mut self, is_custom: bool) {
        self.is_custom_chr = is_custom;
    }

    pub fn set_is_custom_prg(&mut self, is_custom: bool) {
        self.is_custom_prg = is_custom;
    }

    pub fn get_chr_bank_count(&self) -> usize {
        self.total_chr_rom_size / self.chr_bank_size
    }

    pub fn get_prg_bank_count(&self) -> usize {
        self.total_prg_rom_size / self.prg_bank_size
    }

    pub fn set_is_custom_nametable(&mut self, is_custom: bool) {
        self.is_custom_nametable = is_custom;
    }
}