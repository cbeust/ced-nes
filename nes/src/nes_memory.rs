use std::sync::{Arc, RwLock};
use tracing::{debug, info};
use cpu::memory::Memory;
use crate::mappers::mapper::*;
pub(crate) use crate::internal_registers::IR;
use crate::joypad::Joypad;
use crate::mappers::mapper0::Mapper0;
use crate::mappers::mapper_base::{MapperBase, VramType};
use crate::mappers::mapper_base::VramType::Vram;

pub const MEMORY_SIZE: usize = 65_536;

use crate::ppu::{Ppu, SPRITE_OVERFLOW};
use crate::ppu_ctrl::PpuCtrl;
use crate::ppu_mask::PpuMask;
use crate::rom::Mirroring;
use crate::set_bit_with_mask;

pub struct NesMemory{
    pub ir: IR,
    pub init: bool,
    // Holds the non mapped CPU memory: 0x2000..0x7FFF
    memory: Vec<u8>,
    pub ppu_ctrl: PpuCtrl,
    pub ppu_mask: PpuMask,
    pub joypad: Arc<RwLock<Joypad>>,
    ppu: Arc<RwLock<Ppu>>,
    internal_buffer: u8,
    pub mapper: MapperBase,
}

impl  NesMemory{
    pub fn new(mapper: MapperBase,
        joypad: Arc<RwLock<Joypad>>,
        ppu: Arc<RwLock<Ppu>>) -> Self
    {
        let mut memory = Vec::<u8>::new();
        for _ in 0..MEMORY_SIZE {
            memory.push(0);
        }
        Self {
            ir: IR::default(),
            init: true,
            ppu_ctrl: PpuCtrl::new(0),
            ppu_mask: PpuMask::new(0),
            memory,
            joypad,
            ppu,
            internal_buffer: 0,
            mapper,
        }
    }

    pub fn new_for_testing() -> Self {
        let mut result =
            NesMemory::new(MapperBase::default(),
                Arc::new(RwLock::new(Joypad::new())), Arc::new(RwLock::new(Ppu::default())));
        result.init = false;
        result
    }

    pub fn is_vbl_enabled(&self) -> bool {
        (self.memory[0x2000] & 0b1000_0000) != 0
    }

    pub fn cpu_mirrorring(address: u16) -> u16 {
        match address {
            // 0-800 is mirrorred until 2000
            0..0x2000 => {
                address & 0b00000111_11111111
            }
            // 2000-2007 is mirrorred until 4000
            0x2000..0x4000 => {
                address & 0b00100000_00000111
            }
            _ => {
                address
            }
        }
    }

    pub fn ppu_mirrorring(a: u16) -> u16 {
        let address = a & 0x3fff;
        match address {
            0x3000..0x3f00 => {
                // 0x3000-3eff mirrors 2000
                address - 0x1000
            }
            0x3f00..0x4000 => {
                // 3f00-3f1f is mirrorred until 4000
                let index = address & 0b0001_1111;
                0x3f00 + index
            }
            _ => {
                address
            }
        }
    }

    pub fn nametable_mirroring(mirroring: Mirroring, address: usize) -> VramType {
        use VramType::*;
        if address < 0x2000 || address >= 0x3000 {
            return Vram;
        }
        match mirroring {
            Mirroring::Vertical => {
                if (0x2000..0x2400).contains(&address) || (0x2800..0x2c00).contains(&address) {
                    Vram_A
                } else {
                    Vram_B
                }
            }
            Mirroring::Horizontal => {
                if (0x2000..0x2800).contains(&address) {
                    Vram_A
                } else {
                     Vram_B
                 }
            }
            Mirroring::ScreenA => {
                Vram_A
            }
            Mirroring::ScreenB => {
                Vram_B
            }
            _ => {
                panic!("Unknown mirroring: {mirroring:?}");
            }
        }
    }

    fn palette_mirrorring(address: usize) -> Option<usize> {
        match address {
            0x3f00 => Some(0x3f10),
            0x3f04 => Some(0x3f14),
            0x3f08 => Some(0x3f18),
            0x3f0c => Some(0x3f1c),
            0x3f10 => Some(0x3f00),
            0x3f14 => Some(0x3f04),
            0x3f18 => Some(0x3f08),
            0x3f1c => Some(0x3f0c),
            _ => None
        }
    }

    pub fn set_bit(&mut self, address: u16, bit: u8) {
        let current = self.get_force(address);
        let new_value = current | (1 << bit);
        self.set_force(address, new_value);
    }

    pub fn get_force(&mut self, address: u16) -> u8 { self.memory[address as usize] }

    pub fn clear_bit(&mut self, address: u16, bit: u8) {
        let current = self.get_force(address);
        self.set_force(address, current & !(1 << bit));
    }

    fn get2(&mut self, original_address: u16) -> u8 {
        // if original_address == 0x2002 {
        //     info!("GETTING $2002:{:04X}", self.memory[original_address as usize]);
        // }
        let address = Self::cpu_mirrorring(original_address);
        let mut result = self.memory[address as usize];
        if ! self.init && Self::is_register(address) {
            match address {
                0x2002 => {
                    // w:                  <- 0
                    self.ir.w = false;
                    self.clear_bit(0x2002, SPRITE_OVERFLOW);
                    // $2002: bits 0 through 4 are open bus
                    result = (result & 0b1110_0000) | (self.ppu.read().unwrap().get_open_bus() & 0x1f);
                }
                0x2004 => {
                    let oam_address = self.ppu.read().unwrap().oam_address;
                    result = self.ppu.read().unwrap().oam[oam_address as usize];
                    // Third sprite byte need to be masked with 0xe3 because bits 2,3,4 are unused
                    if (oam_address % 4) % 2 == 2 {
                        result = result & 0xe3;
                    }
                    debug!(target: "oam", "Reading OAMDATA $2004 [{:04X}]:{:02X}",
                        oam_address, result);
                    // Note: reads don't increment oam_address, only writes
                }
                0x2007 => {
                    // https://www.nesdev.org/wiki/PPU_registers#The_PPUDATA_read_buffer
                    let a = self.ir.v as usize & 0x3fff;

                    // let is_palette = a >= 0x3f00 && a <= 0x3fff;
                    let is_palette = (a & 0x3f00) == 0x3f00;

                    result = if is_palette {
                        // If palette address, return the direct result and not the internal buffer
                        // but set the internal buffer to the 0x2700 address
                        let a2 = 0x3f00 + (a & 0x1f);
                        let result = self.ppu.read().unwrap().get_vram(a2, &self.mapper);
                        let ts = 0x2700 + (a & 0xff);
                        self.internal_buffer = self.ppu.read().unwrap().get_vram(ts, &self.mapper);
                        debug!(target: "vram", "Reading VRAM palette [{a:04X}/{a2:02X}]:{:02X} and \
                        set internal_buffer to {:02X}",
                            result, self.internal_buffer);
                        result
                    } else {
                        // Regular VRAM read, return the internal buffer then update the internal
                        // buffer to the actual VRAM value
                        let result = self.internal_buffer;
                        self.internal_buffer = self.ppu.read().unwrap().get_vram(a, &self.mapper);
                        debug!(target: "vram","Reading VRAM [{a:04X}] Old internal_buffer:{:02X} \
                        new internal_buffer and value returned:{:02X}",
                            self.internal_buffer, result);
                        result
                    };

                    self.ir.increment_v(self.ppu_ctrl.vram_increment);

                    debug!(target: "ir", "IR:{}: $2007 read {result:02X}", self.ir);
                    // TODO:  During rendering (on the pre-render line and the visible lines 0-239,
                    // provided either background or sprite rendering is enabled), it will update
                    // v in an odd way, triggering a coarse X increment and a Y increment
                    // simultaneously (with normal wrapping behavior)

                }
                // Joypad
                0x4016 => {
                    // TODO: Enabling this makes the cursor move down
                    result = self.joypad.write().unwrap().read();
                }
                _ => {
                    result = self.ppu.read().unwrap().get_open_bus()
                }
            }
        }
        result
    }

    fn is_register(address: u16) -> bool {
        (0x4014 <= address && address <= 0x4016) || (0x2000 <= address && address <= 0x2008)
    }

    fn set2(&mut self, original_address: u16, value: u8) {
        let address = Self::cpu_mirrorring(original_address);
        if !self.init && Self::is_register(address) {
            match address {
                0x2000 => {
                    // PPUCTRL
                    self.ppu_ctrl = PpuCtrl::new(value);
                    debug!(target: "2000", "PPU CTRL: {} VBL:{}", self.ppu_ctrl,
                        (value & 0b1000_0000) != 0);

                    // t: ...GH.. ........ <- d: ......GH
                    self.ir.t = set_bit_with_mask!(self.ir.t, value as u16, 0b11, 10);
                    debug!(target: "ir", "$2000 write IR:{}: value:{value:0b} t:{:0b}", self.ir, self.ir.t);
                }
                0x2001 => {
                    // PPUMASK
                    debug!(target: "2001", "Writing PPUMASK $2001: {value:02X}");
                    self.ppu_mask = PpuMask::new(value);
                }
                0x2003 => {
                    // OAMADDR
                    debug!(target: "oam", "Writing OAMADDR $2003 [{:02X}]={:02X}",
                        self.ppu.read().unwrap().oam_address, value);
                    self.ppu.write().unwrap().oam_address = value;
                }
                0x2004 => {
                    // OAMDATA
                    let mut ppu = self.ppu.write().unwrap();
                    let a = ppu.oam_address;
                    ppu.write_oam(a, value);
                    ppu.oam_address = a.wrapping_add(1);
                    debug!(target: "oam", "Writing OAMDATA $2004 [{:02X}]={:02X} new address:{:04X}",
                        a, value, a.wrapping_add(1));
                }
                0x2005 => {
                    // PPUSCROLL
                    if ! self.ir.w {
                        // t: ....... ...ABCDE <- d: ABCDE...
                        self.ir.t = set_bit_with_mask!(self.ir.t, value as u16 >> 3, 0b11111, 0);

                        // x:              FGH <- d: .....FGH
                        self.ir.x = set_bit_with_mask!(self.ir.x, value & 0b111, 0b111, 0);
                    } else {
                        // t: FGH..AB CDE..... <- d: ABCDEFGH
                        let (abcde, fgh) = ((value as u16 & 0b1111_1000) >> 3, value as u16 & 0b111);
                        self.ir.t = (self.ir.t & 0b000_11_00000_11111) | (fgh << 12) | (abcde << 5);
                    }
                    // Flip w
                    self.ir.w = ! self.ir.w;
                    debug!(target: "ir", "$2005 write {value:02X} {}", self.ir);
                }
                0x2006 => {
                    // PPUADDR
                    if ! self.ir.w {
                        // t: .CDEFGH ........ <- d: ..CDEFGH
                        // t: Z...... ........ <- 0 (bit Z is cleared)
                        self.ir.t =set_bit_with_mask!(self.ir.t, value as u16, 0b11_1111, 8)
                            &0b1011_1111_1111_1111;
                    } else {
                        // t: ....... ABCDEFGH <- d: ABCDEFGH
                        self.ir.t = (self.ir.t & 0xff00) | (value as u16);

                        // v: <...all bits...> <- t: <...all bits...>
                        self.ir.set_v_to_t();
                    }
                    self.ir.w = ! self.ir.w;
                    debug!(target: "ir", "IR:{}: $2006 write {value:02X}", self.ir);
                }
                0x2007 => {
                    // PPUDATA

                    let a = self.ir.v as usize & 0x3fff;
                    let a2 = Self::nametable_mirroring(self.mapper.mirroring(), a);

                    self.ppu.write().unwrap().set_vram(a, value, &mut self.mapper);
                    // Palette mirroring: addresses 0x3F00/0x3F10, 0x3F04/0x3F14, 0x3F08/0x3F18,
                    // 0x3F0C/0x3F1C are mirrored
                    // TOOD: restore this to pass tests again
                    if let Some(mirror_address) = Self::palette_mirrorring(a) {
                        self.ppu.write().unwrap().set_vram(mirror_address, value, &mut self.mapper);
                    };

                    debug!(target: "vram", "Writing VRAM [{a:04X}]={value:02X}");

                    self.ir.increment_v(self.ppu_ctrl.vram_increment);

                    debug!(target: "ir", "IR:{}: $2007 write {value:02X}", self.ir);
                    // TODO:  During rendering (on the pre-render line and the visible lines 0-239,
                    // provided either background or sprite rendering is enabled), it will update
                    // v in an odd way, triggering a coarse X increment and a Y increment
                    // simultaneously (with normal wrapping behavior)
                }
                0x4014 => {
                    // info!("DMA WRITE ACCESS {value:02X}");
                    let address = value as u16 * 0x100;
                    let offset = self.ppu.read().unwrap().oam_address;
                    for i in 0..=255 {
                        let i2 = (i + offset as u16) as u8;
                        let v = self.get(address + i);
                        self.ppu.write().unwrap().write_oam(i2, v);
                    }
                    debug!(target: "oam", "Writing PPU $4014={value:02X}, \
                     copied 256 bytes from cpu[{:04X}] to OAM", address + offset as u16);
                }
                // Joypad
                0x4016 => {
                    self.joypad.write().unwrap().write(value);
                }
                0x4017 => {
                    self.joypad.write().unwrap().write(value);
                }
                _ => {
                    self.ppu.write().unwrap().set_open_bus(value);
                    // info!("Unhandled PPU register write at {address:04X}={value:02X}");
                    // println!();
                }
            }
            self.ppu.write().unwrap().set_open_bus(value);
        }
        self.set_force(address, value);
    }
}

impl  Memory for NesMemory{
    fn get(&mut self, address: u16) -> u8 {
        if address >= 0x8000 {
            self.mapper.read_prg(address)
        } else {
            self.get2(address)
        }
    }

    fn set(&mut self, address: u16, value: u8) {
        if address >= 0x8000 {
            // self.set_force(address, value);
            self.mapper.write_prg(address, value);
        } else {
            self.set2(address, value);
        }
    }

    fn set_force(&mut self, address: u16, value: u8) {
        if address >= 0x8000 {
            // self.set_force(address, value);
            self.mapper.write_prg(address, value);
        } else {
            self.memory[address as usize] = value;
        }
    }

    fn get_direct(&mut self, address: u16) -> u8 {
        if address >= 0x8000 {
            self.mapper.read_prg(address)
        } else {
            self.memory[address as usize]
        }
    }

    fn main_memory(&mut self) -> Vec<u8> {
        self.memory.clone()
    }
}