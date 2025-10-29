use tracing::{debug, info};
use crate::mappers::mapper::{Mapper};
use crate::mappers::mapper_config::MapperConfig;
use crate::rom::Rom;

pub struct Mapper19 {
    chr_rom: Vec<u8>,
    prg_rom: Vec<u8>,
    prg_banks: [u8; 4],

    chr_banks: [u8; 12],
    ram_enable: u8,
    ciram: [u8; 0x800],  // 2 internal nametables, 0x400 each
    write_protect: u8,
    // wram: [u8; 0x8000],

    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
}

/// Mapper 19
impl Mapper19 {
    pub fn new(rom: &Rom, config: &mut MapperConfig) -> Self {
        config.set_prg_bank_size(0x2000);
        config.set_chr_bank_size(0x400);
        config.set_prg_bank(3, config.get_prg_bank_count() - 1);
        // config.set_is_custom_prg(true);
        config.set_is_custom_chr(true);
        config.set_is_custom_nametable(true);
        info!("PRG BANK COUNT: {}", config.get_prg_bank_count());

        Self {
            chr_rom: rom.chr_rom.clone(),
            prg_rom: rom.prg_rom.clone(),
            ciram: [0; 0x800],
            prg_banks: [0, 0, 0, config.get_prg_bank_count() as u8 - 1],
            chr_banks: [0; 12],
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            ram_enable: 0,
            write_protect: 0,
            // wram: [0; 0x8000],

        }
    }
}

impl Mapper19 {
    fn write_chr_19(&mut self, address: u16, value: u8) {
        let address = (address as usize) & 0x3fff;
        let bank_index = address / 0x400;
        let offset = address % 0x400;
        let bank_value = self.chr_banks[bank_index];

        // Handle $E0-$FF range (CIRAM is always writable)
        if bank_value >= 0xE0 {
            if bank_index >= 8 {  // Nametables
                let ciram_page = bank_value as usize & 0x01;
                self.ciram[(ciram_page * 1024) + offset] = value;
            } else {  // Pattern tables using CIRAM
                let use_ciram = if bank_index <= 3 { (self.ram_enable & 0x40) == 0 }
                    else { (self.ram_enable & 0x80) == 0 };
                if use_ciram {
                    let ciram_page = bank_value as usize & 0x01;
                    self.ciram[(ciram_page * 1024) + offset] = value;
                }
            }
            return;
        }
    }

    fn read_chr_19(&self, address: u16) -> u8 {
        let address = (address as usize) & 0x3fff;
        let bank_index = address / 0x400;
        let offset = address % 0x400;
        let bank_value = self.chr_banks[bank_index];
        if bank_value >= 0xe0 {
            return self.read_special_bank(bank_index, bank_value, offset);
        }

        let chr_address = (bank_value as usize) * 0x400 + offset;
        // if ram present
        //     return self.chr_ram[chr_address];

        self.chr_rom[chr_address]
    }

    fn read_special_bank(&self, bank_index: usize, bank_value: u8, offset: usize) -> u8 {
        if bank_index >= 8 {
            // Banks 8-11 (nametables): Always use CIRAM for $E0-$FF
            let ciram_page = bank_value as usize & 1;
            return self.ciram[ciram_page * 0x400 + offset];
        }

        // Banks 0-3 (pattern table 0): Check bit 6 of $E800
        if bank_index <= 3 {
            let use_ciram = (self.ram_enable & 0x40) == 0;
            if use_ciram {
                let ciram_page = bank_value as usize & 0x01;
                return self.ciram[(ciram_page * 1024) + offset];
            }
            // Use last banks of CHR-ROM
            let chr_address = (bank_value as usize * 1024) + offset;
            return self.chr_rom[chr_address % self.chr_rom.len()];
        }

        // Banks 4-7 (pattern table 1): Check bit 7 of $E800
        if bank_index <= 7 {
            let use_ciram = (self.ram_enable & 0x80) == 0;
            if use_ciram {
                let ciram_page = bank_value as usize & 0x01;
                return self.ciram[(ciram_page * 1024) + offset];
            }
            // Use last banks of CHR-ROM
            let chr_address = (bank_value as usize * 1024) + offset;
            return self.chr_rom[chr_address % self.chr_rom.len()];
        }

        panic!("Should never reach here");

    }
}

impl Mapper for Mapper19 {
    fn write_prg(&mut self, address: u16, data: u8, config: &mut MapperConfig) {
        let mask = address & 0xf800;
        match mask {
            0x4800 => {
                // Audio chip
            }
            0x5000 => {
                // Low 8 bits of the IRQ counter
                self.irq_counter = (self.irq_counter & 0xff00) | data as u16;
                self.irq_pending = false;
                // debug!(target: "mapper", "Setting low 8 bits of IRQ counter:{:02X} new counter:{:04X}",
                //     data as u16, self.irq_counter);
            }
            0x5800 => {
                // High 7 bits of the IRQ counter
                let value = (data as u16 & 0x7f) << 8;
                self.irq_counter = (self.irq_counter & 0xff) | value;
                self.irq_pending = false;
                self.irq_enabled = (data & 0x80) != 0;
                // debug!(target: "mapper", "Setting high 7 bits of IRQ counter, new counter:{:04X} enabled:{}",
                //     self.irq_counter, self.irq_enabled);
                // println!();
            }
            0x8000 | 0x8800 | 0x9000 | 0x9800 | 0xa000 | 0xa800 | 0xb000 | 0xb800 => {
                // Pattern tables
                let bank_index = (address as usize >> 11) & 7;  // 0-7
                self.chr_banks[bank_index] = data;
                // debug!(target: "mapper", "pattern chr_banks[${bank_index:02X}]=${data:02X}");
            }
            0xc000 | 0xc800 | 0xd000 | 0xd800 => {
                // Nametables
                let bank_index = 8 + ((address as usize >> 11) & 3);  // 0-2
                debug!(target: "mapper", "nametable chr_banks[${bank_index:02X}]=${data:02X}");
                self.chr_banks[bank_index] = data;
            }
            0xe000 | 0xe800 | 0xf000 => {
                let bank = (address as usize - 0xe000) / 0x800;
                let data2 = data & config.get_prg_bank_count() as u8 - 1;
                self.prg_banks[bank] = data2;
                config.set_prg_bank(bank, data as usize);
                if mask == 0xe800 {
                    // Write E800 register
                    self.ram_enable = data;
                }
                debug!(target: "mapper", "M19: prg_banks[0]=${:02X} [{:02X} {:02X} {:02X}]",
                data2, self.prg_banks[0], self.prg_banks[1], self.prg_banks[2]);
            }
            0xf800 => {
                self.write_protect = data;
            }
            _ => {
                // self.wram[address as usize] = data;
            }
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.read_chr_19(addr)
    }

    fn write_chr(&mut self, addr: u16, data: u8) {
        self.write_chr_19(addr, data);
    }

    fn read_nametable(&self, address: usize) -> u8 {
        self.read_chr_19(address as u16)
    }

    fn write_nametable(&mut self, address: usize, value: u8) {
        self.write_chr_19(address as u16, value);
    }

    fn on_cpu_cycle(&mut self) -> bool {
        if self.irq_enabled {
            if self.irq_counter < 0x7fff {
                self.irq_counter += 1;
                // info!("New IRQ counter: {:04X}", self.irq_counter);
                return false;
            } else {
                // Reached 0x7fff, trigger an IRQ, unless we've already just triggered one
                if ! self.irq_pending {
                    self.irq_pending = true;
                    // info!("IRQ triggered");
                    return true
                }
            }
        }

        false
    }
}
