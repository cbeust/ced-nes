use crate::mappers::mapper::Mapper;
use crate::mappers::mapper_config::MapperConfig;
use crate::rom::{Mirroring, Rom};
use std::path::PathBuf;
use tracing::{debug};

/// MMC3 (#4)
/// https://www.nesdev.org/wiki/MMC3
pub struct MapperMMC3 {
    chr_mode: u8,
    prg_mode: u8,
    /// 0..7 (R0..R7)
    register: usize,
    registers: [usize; 8],
    irq_load_value: u8,
    irq_counter: u8,
    irq_reload: bool,
    irq_enabled: bool,
}

impl MapperMMC3 {
    pub fn new(_rom: &Rom, config: &mut MapperConfig) -> Self {
        config.set_prg_bank_size(0x2000);
        config.set_chr_bank_size(0x400);
        config.set_prg_bank(3, config.get_prg_bank_count() - 1);

        Self {
            chr_mode: 0,
            prg_mode: 0,
            register: 0,
            registers: [0; 8],
            irq_load_value: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: false,
        }
    }
}

impl Mapper for MapperMMC3 {
    fn write_prg(&mut self, addr: u16, data: u8, config: &mut MapperConfig) {
        debug!(target: "mapper", "M3: write_prg [${addr:04X}] = {data:02X}");
        let even = (addr & 1) == 0;
        match addr {
            0x8000..=0x9fff => {
                if even {
                    self.register = data as usize & 0b111;
                    self.prg_mode = (data & 0b0100_0000) >> 6;
                    self.chr_mode = (data & 0b1000_0000) >> 7;
                    debug!(target: "mapper", "    write_prg register:{} prg_mode:{} chr_mode:{}",
                        self.register, self.prg_mode, self.chr_mode);
                } else {
                    // let data = data & 0x1f;
                    let data = data as usize;
                    self.registers[self.register] =
                        if self.register <= 1 {
                            // Ignore bottom bit for R0 and R1
                            data & 0xfe
                        } else if self.register >= 6 {
                            // Ignore top two bits for registers 6 and 7
                            data & 0b0011_1111
                        } else {
                            data
                        };
                    debug!(target: "mapper", "    set register[{}]={}", self.register,
                        self.registers[self.register]);
                }
                self.update_banks(config);
            }
            0xa000..=0xbfff => {
                debug!(target: "mapper", "    (TBD) write_prg nametable/PRG RAM protect");
                if even {
                    let mirroring = if (data & 1) == 1 {
                        Mirroring::Horizontal
                    } else {
                        Mirroring::Vertical
                    };
                    config.set_mirroring(mirroring);
                } else {
                    // warn!("PRG RAM PROTECT NOT IMPLEMENTED");
                }
            }
            0xc000..=0xdfff => {
                if even {
                    debug!(target: "mapper", "    IRQ load_value: {data:02X}");
                    self.irq_load_value = if data > 0 { data - 1 } else { data }
                } else {
                    debug!(target: "mapper", "    IRQ counter: 0, reload: true");
                    self.irq_counter = 0;
                    self.irq_reload = true;
                }
            }
            0xe000..=0xffff => {
                self.irq_enabled = ! even;
                debug!(target: "mapper", "    (TBD) IRQ enabled:{}", self.irq_enabled);
            }
            _ => {
                // info!("Writing to unhandled address:{addr:04X}");
                // println!();
            }
        }
    }

    fn on_scanline(&mut self) -> bool {
        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_load_value
        } else {
            self.irq_counter -= 1;
        }

        self.irq_reload = false;

        if self.irq_counter == 0 && self.irq_enabled {
            debug!(target: "mapper", "Triggering IRQ from mapper");
            true
        } else {
            false
        }
    }

}

impl MapperMMC3 {
    fn update_banks(&mut self, config: &mut MapperConfig) {
        {
            let even = self.chr_mode == 0;
            config.set_chr_bank(0, if even { self.registers[0] & 0xfe } else { self.registers[2] });
            config.set_chr_bank(1, if even { self.registers[0] | 1 } else { self.registers[3] });
            config.set_chr_bank(2, if even { self.registers[1] & 0xfe } else { self.registers[4] });
            config.set_chr_bank(3, if even { self.registers[1] | 1 } else { self.registers[5] });
            config.set_chr_bank(4, if even { self.registers[2] } else { self.registers[0] & 0xfe });
            config.set_chr_bank(5, if even { self.registers[3] } else { self.registers[0] | 1 });
            config.set_chr_bank(6, if even { self.registers[4] } else { self.registers[1] & 0xfe });
            config.set_chr_bank(7, if even { self.registers[5] } else { self.registers[1] | 1 });
        }

        {
            let even = self.prg_mode == 0;
            config.set_prg_bank(0,
                if even { self.registers[6] } else { config.get_prg_bank_count() - 2 });
            config.set_prg_bank(1, self.registers[7]);
            config.set_prg_bank(2,
                if even { config.get_prg_bank_count() - 2 } else { self.registers[6] });
            config.set_prg_bank(3, config.get_prg_bank_count() - 1);
        }
    }
}

pub fn _test_mapper3() {
    let file_name = [dirs::home_dir().unwrap().to_str().unwrap(),
        "rust", "sixty.rs", "nes", "Donkey Kong - Original Edition (U) (VC) [!].nes"
    ]
        .iter().fold(PathBuf::new(), |mut path, segment| {
            path.push(segment);
            path
        });
    let rom = Rom::read_nes_file(file_name.to_str().unwrap()).unwrap();
    let mapper = MapperMMC3::new(&rom, &mut MapperConfig::default());
    assert_eq!(mapper.read_prg(0xc79e), 0x78);
    assert_eq!(mapper.read_prg(0xf092), 0x85);
}