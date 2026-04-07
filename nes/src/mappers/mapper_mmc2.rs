use crate::mappers::mapper::Mapper;
use crate::mappers::mapper_config::MapperConfig;
use crate::rom::Mirroring::{Horizontal, Vertical};
use crate::rom::Rom;
use tracing::{debug};

pub struct MapperMMC2 {
    left_latch: usize,
    right_latch: usize,
    left_chr_page: [usize; 2],
    right_chr_page: [usize; 2],
    need_update: bool,
}

/// MMC2, Mapper 9
impl MapperMMC2 {
    pub fn new(_rom: &Rom, config: &mut MapperConfig) -> Self {
        config.set_prg_bank_size(0x2000); // 8KB
        config.set_chr_bank_size(0x1000); // 4KB
        config.set_prg_bank(1, config.get_prg_bank_count() - 3);
        config.set_prg_bank(2, config.get_prg_bank_count() - 2);
        config.set_prg_bank(3, config.get_prg_bank_count() - 1);
        config.on_read_chr_hook = true;
        Self {
            left_latch: 1,
            right_latch: 1,
            left_chr_page: [0; 2],
            right_chr_page: [0; 2],
            need_update: false,
        }
    }
}

impl Mapper for MapperMMC2 {
    fn write_prg(&mut self, addr: u16, data: u8, config: &mut MapperConfig) {
        // debug!(target: "mapper", "MMC2: write_prg [${addr:04X}] = {data:02X}");
        let data = data as usize;
        match addr >> 12 {
            0xa => {
                // debug!(target: "mapper", "MMC2: set_prg_bank(0, {:02X})", data & 0xf);
                config.set_prg_bank(0, data & 0xf);
            }
            0xb => {
                self.left_chr_page[0] = data & 0x1f;
                let bank = self.left_chr_page[self.left_latch];
                config.set_chr_bank(0, bank);
                debug!(target: "mapper", "MMC2: left_chr_page[0]:{:X} chr_bank_0: {bank:X}",
                    self.left_chr_page[0]);
            }
            0xc => {
                self.left_chr_page[1] = data & 0x1f;
                let bank = self.left_chr_page[self.left_latch];
                config.set_chr_bank(0, bank);
                debug!(target: "mapper", "MMC2: left_chr_page[0]:{:X} chr_bank_0: {bank:X}",
                    self.left_chr_page[1]);
            }
            0xd => {
                self.right_chr_page[0] = data & 0x1f;
                let bank = self.right_chr_page[self.right_latch];
                config.set_chr_bank(1, bank);
                debug!(target: "mapper", "MMC2: left_chr_page[0]:{:X} chr_bank_0: {bank:X}",
                    self.right_chr_page[0]);
            }
            0xe => {
                self.right_chr_page[1] = data & 0x1f;
                let bank = self.right_chr_page[self.right_latch];
                config.set_chr_bank(1, bank);
                debug!(target: "mapper", "MMC2: left_chr_page[0]:{:X} chr_bank_0: {bank:X}",
                    self.right_chr_page[1]);
            }
            0xf => {
                config.set_mirroring(if data & 0x01 == 0 { Vertical } else { Horizontal });
            }
            _ => {}
        }
    }

    fn on_read_chr(&mut self, addr: u16, config: &mut MapperConfig) {
        if self.need_update {
            config.set_chr_bank(0, self.left_chr_page[self.left_latch]);
            config.set_chr_bank(1, self.right_chr_page[self.right_latch]);
            self.need_update = false;
        }
        // if addr == 0xfd8 || addr == 0xfe8 || (0x1fd8..=0x1fdf).contains(&addr)
        //     ||  (0x1fe8..=0x1fef).contains(&addr) {
        //     info!("MMC2: read_chr [${addr:04X}]");
        //     println!();
        // }
        if addr == 0xfd8 {
            self.left_latch = 0;
            self.need_update = true;
        } else if addr == 0xfe8 {
            self.left_latch = 1;
            self.need_update = true;
        }
        else if (0x1fd8..=0x1fdf).contains(&addr) {
            self.right_latch = 0;
            self.need_update = true;
        }
        else if (0x1fe8..=0x1fef).contains(&addr) {
            self.right_latch = 1;
            self.need_update = true;
        }
    }
}
