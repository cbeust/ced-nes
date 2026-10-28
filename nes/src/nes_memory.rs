use crate::apu::Apu;
pub(crate) use crate::internal_registers::IR;
use crate::joypad::Joypad;
use crate::mappers::mapper_base::{MapperBase, VramType};
use cpu::memory::Memory;
use std::sync::{Arc, RwLock};
use tracing::{debug};

pub const MEMORY_SIZE: usize = 65_536;

use crate::ppu2::BIT_VBL;
use crate::ppu2::Ppu2;
use crate::ppu_ctrl::PpuCtrl;
use crate::ppu_mask::PpuMask;
use crate::rom::Mirroring;
use crate::set_bit_with_mask;

/// Cycle-accurate OAM DMA ($4014) state — runs interleaved with DMC DMA.
#[derive(Clone, Debug)]
pub struct OamDma {
    /// Page << 8 base address.
    pub page: u16,
    /// Next OAM address to write (starts at OAMADDR).
    pub oam_addr: u8,
    /// Bytes transferred so far (0..256).
    pub bytes_done: u16,
    /// Latched CPU byte waiting for put, if any.
    pub latched: Option<u8>,
    /// True after the halt cycle has completed.
    pub halted: bool,
    /// After halt on a get cycle, spend one put as alignment before first get.
    pub need_align: bool,
    /// After a DMC sample get steals a get cycle, spend one put as OAM alignment.
    pub align_after_dmc: bool,
}

impl OamDma {
    pub fn new(page: u8, oam_addr: u8) -> Self {
        Self {
            page: (page as u16) << 8,
            oam_addr,
            bytes_done: 0,
            latched: None,
            halted: false,
            need_align: false,
            align_after_dmc: false,
        }
    }

    pub fn done(&self) -> bool {
        self.bytes_done >= 256 && self.latched.is_none()
    }
}

pub struct NesMemory{
    pub ir: IR,
    pub init: bool,
    // Holds the non mapped CPU memory: 0x2000..0x7FFF
    memory: Vec<u8>,
    pub ppu_ctrl: PpuCtrl,
    pub ppu_mask: PpuMask,
    pub joypad: Arc<RwLock<Joypad>>,
    /// Controller 2 ($4017 reads). Strobe is shared with controller 1 via $4016 writes.
    pub joypad2: Arc<RwLock<Joypad>>,

    ppu: Arc<RwLock<Ppu2>>,

    apu: Arc<RwLock<Apu>>,
    pub(crate) internal_buffer: u8,
    pub mapper: MapperBase,
    /// Legacy bulk-OAM flag (unused when `oam_dma` is Some).
    pub(crate) pause_cpu_for_dma: bool,
    /// Active cycle-stepped OAM DMA, if any.
    pub(crate) oam_dma: Option<OamDma>,
    /// True while OAM DMA is performing a get (sprite fetch). When set and the
    /// 6502 bus is outside $4000–$401F, APU registers are inactive (open bus).
    pub(crate) oam_dma_fetching: bool,
    /// Approximate 6502 core address while halted (for APU register activation).
    pub(crate) cpu_bus_address: u16,
    /// Last value driven on the CPU data bus (CPU open bus).
    /// Updated on every bus read/write except $4015 (does not drive the bus).
    /// DMC DMA sample fetches update it too.
    pub(crate) cpu_data_bus: u8,
    /// Last value on the 2A03 internal data bus (drives bit 5 of $4015 reads).
    pub(crate) cpu_internal_bus: u8,
    pub nmi_requested: bool,
    pub nmi_cancel_requested: bool,
    prev_nmi_line_low: bool,
    pub total_ppu_dots: u64,
    pub nmi_low_start_dot: u64,
    pub nmi_delay: u8,
}

impl NesMemory{
    pub fn new(mapper: MapperBase,
        joypad: Arc<RwLock<Joypad>>,

        ppu: Arc<RwLock<Ppu2>>,

        apu: Arc<RwLock<Apu>>,
    ) -> Self
    {
        let mut memory = Vec::<u8>::new();
        for _i in 0..MEMORY_SIZE {
            memory.push(0);
            // if i < 0x4800 { memory.push(0); }
            // else { memory.push((i / 256) as u8); }
        }
        memory[0x2001] = 0x08;
        memory[0x4010] = 0x40;
        Self {
            ir: IR::default(),
            init: true,
            ppu_ctrl: PpuCtrl::new(0),
            ppu_mask: PpuMask::new(0),
            memory,
            joypad,
            joypad2: Arc::new(RwLock::new(Joypad::new())),
            ppu,
            apu,
            internal_buffer: 0,
            mapper,
            pause_cpu_for_dma: false,
            oam_dma: None,
            oam_dma_fetching: false,
            cpu_bus_address: 0,
            cpu_data_bus: 0,
            cpu_internal_bus: 0,
            nmi_requested: false,
            nmi_cancel_requested: false,
            prev_nmi_line_low: false,
            total_ppu_dots: 0,
            nmi_low_start_dot: 0,
            nmi_delay: 0,
        }
    }

    pub fn new_for_testing() -> Self {
        let ppu = Arc::new(RwLock::new(Ppu2::default()));
        let mut result =
            NesMemory::new(MapperBase::default(),
                Arc::new(RwLock::new(Joypad::new())),
                ppu,
                Arc::new(RwLock::new(Apu::new()))
            );
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
                    VramA
                } else {
                    VramB
                }
            }
            Mirroring::Horizontal => {
                if (0x2000..0x2800).contains(&address) {
                    VramA
                } else {
                    VramB
                 }
            }
            Mirroring::FourScreen => {
                // Four-screen boards provide distinct backing for all 4 nametables.
                Vram
            }
            Mirroring::ScreenA => {
                VramA
            }
            Mirroring::ScreenB => {
                VramB
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

    pub fn update_nmi_line(&mut self) {
        let vbl = (self.memory[0x2002] & (1 << BIT_VBL)) != 0;
        let nmi_enable = self.is_vbl_enabled();
        let line_low = vbl && nmi_enable;
        if line_low && !self.prev_nmi_line_low {
            self.nmi_low_start_dot = self.total_ppu_dots;
            if self.total_ppu_dots % 3 == 0 {
                self.nmi_delay = 1;
                self.nmi_requested = false;
            } else {
                self.nmi_delay = 0;
                self.nmi_requested = true;
            }
        } else if !line_low && self.prev_nmi_line_low {
            let duration = self.total_ppu_dots.saturating_sub(self.nmi_low_start_dot);
            if duration <= 1 {
                self.nmi_requested = false;
                self.nmi_delay = 0;
                self.nmi_cancel_requested = true;
            }
        }
        self.prev_nmi_line_low = line_low;
    }

    pub fn set_bit(&mut self, address: u16, bit: u8) {
        let current = self.get_force(address);
        let new_value = current | (1 << bit);
        self.set_force(address, new_value);
        if address == 0x2002 && bit == BIT_VBL {
            self.update_nmi_line();
        }
    }

    pub fn get_force(&mut self, address: u16) -> u8 { self.memory[address as usize] }

    pub fn clear_bit(&mut self, address: u16, bit: u8) {
        let current = self.get_force(address);
        self.set_force(address, current & !(1 << bit));
        if address == 0x2002 && bit == BIT_VBL {
            self.update_nmi_line();
        }
    }

    fn get2(&mut self, original_address: u16) -> u8 {
        // if original_address == 0x2002 {
        //     info!("GETTING $2002:{:04X}", self.memory[original_address as usize]);
        // }
        let address = Self::cpu_mirrorring(original_address);
        let mut result = self.memory[address as usize];
        if ! self.init && Self::is_register(address) {
            match address {
                0x2000 | 0x2001 | 0x2003 | 0x2005 | 0x2006 => {
                    // Write-only PPU registers: reads return PPU open bus.
                    // Reading write-only registers does NOT update the open bus latch.
                    return self.ppu.read().unwrap().get_open_bus();
                }
                0x2002 => {
                    // w:                  <- 0
                    self.ir.w = false;
                    let (scanline, x) = {
                        let ppu = self.ppu.read().unwrap();
                        (ppu.scanline, ppu.x)
                    };

                    if scanline == 261 && x == 0 {
                        result &= !0x60;
                    }
                    if scanline == 241 && x == 0 {
                        self.ppu.write().unwrap().vblank_suppressed = true;
                    }
                    if scanline == 241 && (x == 1 || x == 2) {
                        self.nmi_cancel_requested = true;
                        self.nmi_requested = false;
                        self.ppu.write().unwrap().nmi_suppressed = true;
                    }
                    // VSOx xxxx
                    // |||| ||||
                    // |||+-++++- (PPU open bus or 2C05 PPU identifier)
                    // ||+------- Sprite overflow flag (NOT cleared on read, only at dot 1 of pre-render line)
                    // |+-------- Sprite 0 hit flag (NOT cleared on read, only at dot 1 of pre-render line)
                    // +--------- Vblank flag, cleared on read. Unreliable; see below.
                    self.clear_bit(0x2002, BIT_VBL);
                    // $2002: bits 0 through 4 are open bus
                    result = (result & 0b1110_0000)
                        | (self.ppu.read().unwrap().get_open_bus() & 0x1f);
                    //  reading $2002 overwrites latch bits 7..5 with current status bits.
                    let mut open_bus = self.ppu.read().unwrap().get_open_bus();
                    open_bus = (result & 0b1110_0000) | (open_bus & 0b0001_1111);
                    self.ppu.write().unwrap().set_open_bus(open_bus);
                }
                0x2004 => {
                    let (scanline, oam_buffer, oam_address) = {
                        let ppu = self.ppu.read().unwrap();
                        (ppu.scanline, ppu.oam_buffer, ppu.oam_address)
                    };
                    let rendering_enabled = self.ppu_mask.is_rendering_enabled();
                    if rendering_enabled && scanline < 240 {
                        result = oam_buffer;
                    } else {
                        result = self.ppu.read().unwrap().oam[oam_address as usize];
                        if oam_address % 4 == 2 {
                            result &= 0xe3;
                        }
                    }
                    // Note: reads don't increment oam_address, only writes
                }
                0x2007 => {
                    // https://www.nesdev.org/wiki/PPU_registers#The_PPUDATA_read_buffer
                    let a = self.ir.v as usize & 0x3fff;

                    // let is_palette = a >= 0x3f00 && a <= 0x3fff;
                    let is_palette = (a & 0x3f00) == 0x3f00;
                    let is_rendering = (self.ppu_mask.background_rendering()
                        || self.ppu_mask.sprite_rendering())
                        && (self.ppu.read().unwrap().scanline < 240
                            || self.ppu.read().unwrap().scanline == 261);

                    result = if is_palette {
                        // If palette address, return the direct result and not the internal buffer
                        // The PPU also performs a normal read from PPU memory
                        // at the specified address, "underneath" the palette data,
                        let a2 = 0x3f00 + (a & 0x1f);
                        let vram = self.ppu.read().unwrap().get_vram(a2, &mut self.mapper);
                        if !is_rendering {
                            let ts = 0x2f00 + (a & 0xff);
                            self.internal_buffer = self.ppu.read().unwrap().get_vram(ts, &mut self.mapper);
                        }
                        debug!(target: "vram", "Reading VRAM palette [{a:04X}/{a2:02X}]:{:02X} and \
                        set internal_buffer to {:02X}",
                            result, self.internal_buffer);
                        // Palette values are 6 bits

                        if self.ppu_mask.greyscale() {
                            // Greyscale: Reading from Palette RAM with greyscale mode enabled
                            // always reads the lower four bits as zero ;;;
                            vram & 0b1111_0000
                            // Read $10 from palette ram! (notably, not $5A, which is the value we
                            // wrote. Not even $1A. Rather, the lower four bits are all zero.)
                        } else {
                            // Not greyscale
                            let open_bus = self.ppu.read().unwrap().get_open_bus();
                            (open_bus & 0b1100_0000) | (vram & 0b0011_1111)
                        }
                    } else {
                        // Regular VRAM read, return the internal buffer then update the internal
                        // buffer to the actual VRAM value
                        let result = self.internal_buffer;
                        if !is_rendering {
                            self.internal_buffer = self.ppu.read().unwrap().get_vram(a, &mut self.mapper);
                        }
                        debug!(target: "vram","Reading VRAM [{a:04X}] Old internal_buffer:{:02X} \
                        new internal_buffer and value returned:{:02X}",
                            self.internal_buffer, result);
                        result
                    };

                    if is_rendering {
                        self.ppu.write().unwrap().pending_2007_read_delay = 5;
                    } else {
                        self.ir.increment_v(self.ppu_ctrl.vram_increment);
                    }

                    debug!(target: "ir", "IR:{}: $2007 read {result:02X}", self.ir);
                }
                0x4015 => {
                    // APU regs active only when 6502 bus is in $4000–$401F
                    // (AccuracyCoin APU Register Activation). During OAM DMA from
                    // page $40 with CPU halted elsewhere, reads are open bus.
                    let apu_active = !self.oam_dma_fetching
                        || (self.cpu_bus_address & 0xFFE0) == 0x4000;
                    if apu_active {
                        result = (self.apu.write().unwrap().get(0x4015) & 0xDF)
                            | (self.cpu_internal_bus & 0x20);
                    } else {
                        result = self.cpu_data_bus;
                    }
                }

                // Joypad 1 — D0 is serial data; D1–D4 are 0 on a standard NES port
                // (no expansion); D5–D7 are open bus. AccuracyCoin Bus Conflicts
                // returns A=$50 which is this pattern — if D1–D7 are all open bus,
                // A becomes $FF and the menu shows the test as "skipped".
                0x4016 => {
                    let apu_active = !self.oam_dma_fetching
                        || (self.cpu_bus_address & 0xFFE0) == 0x4000;
                    if apu_active {
                        let bit0 = self.joypad.write().unwrap().read() & 1;
                        result = (self.cpu_data_bus & 0xE0) | bit0;
                    } else {
                        result = self.cpu_data_bus;
                    }
                }
                // Joypad 2 (write to $4017 is the APU frame counter — see set2)
                0x4017 => {
                    let apu_active = !self.oam_dma_fetching
                        || (self.cpu_bus_address & 0xFFE0) == 0x4000;
                    if apu_active {
                        let bit0 = self.joypad2.write().unwrap().read() & 1;
                        result = (self.cpu_data_bus & 0xE0) | bit0;
                    } else {
                        result = self.cpu_data_bus;
                    }
                }
                // Unreadable APU / write-only registers: CPU open bus.
                // (High-byte-of-address is only correct when that was the last bus
                // activity, e.g. the absolute address high byte of LDA $4000.
                // DMC DMA sample fetches also leave their value on the bus.)
                0x4000..=0x4014 => {
                    result = self.cpu_data_bus;
                }
                _ => {
                    result = self.cpu_data_bus;
                }
            }
            if address == 0x2004 || address == 0x2007 {
                self.ppu.write().unwrap().set_open_bus(result);
            }
        }

        // if address == 0x2002 || address == 0x2004 || address == 0x2007
        //         || (address >= 0x2008 && address <= 0x3fff)
        // {
        //     self.ppu.write().unwrap().set_open_bus(result);
        // }

        result
    }

    fn is_register(address: u16) -> bool {
        match address {
            0x4000..=0x4013 | 0x4014..=0x4017 => {
                true
            }
            0x2000..=0x2008 => {
                true
            }
            _ => {
                false
            }
        }
        // (0x4014 <= address && address <= 0x4016) || (0x2000 <= address && address <= 0x2008)
    }

    fn set2(&mut self, original_address: u16, value: u8) {
        let address = Self::cpu_mirrorring(original_address);
        if !self.init && Self::is_register(address) {
            match address {
                0x2000 => {
                    // PPUCTRL
                    self.memory[0x2000] = value;
                    self.ppu_ctrl = PpuCtrl::new(value);
                    self.update_nmi_line();
                    debug!(target: "2000", "PPU CTRL: {} VBL:{}", self.ppu_ctrl,
                        (value & 0b1000_0000) != 0);

                    // t: ...GH.. ........ <- d: ......GH
                    self.ir.t = set_bit_with_mask!(self.ir.t, value as u16, 0b11, 10);
                    debug!(target: "ir", "$2000 write IR:{}: value:{value:0b} t:{:0b}", self.ir,
                        self.ir.t);
                    // ;;; Test 2 [PPU Open Bus]: All PPU Registers update PPU Open Bus. ;;;
                }
                0x2001 => {

                    // PPUMASK – rendering bits take effect after a 3/4-dot delay.
                    debug!(target: "2001", "Writing PPUMASK $2001: {value:02X}");
                    let old_rendering = self.ppu_mask.is_rendering_enabled();
                    let new_rendering = (value & 0x18) != 0;
                    if old_rendering && !new_rendering {
                        let (scanline, x, oam2_eval_index) = {
                            let ppu = self.ppu.read().unwrap();
                            (ppu.scanline, ppu.x, ppu.oam2_eval_index)
                        };
                        if scanline < 240 || scanline == 261 {
                            let sec_oam_addr = if (1..=64).contains(&x) {
                                ((x + 2) / 2) as u8
                            } else if (65..=256).contains(&x) {
                                (oam2_eval_index as u8 / 4) * 4
                            } else {
                                0
                            };
                            let row = sec_oam_addr;
                            if row > 0 {
                                self.ppu.write().unwrap().pending_oam_corruption_row = Some(row);
                            }
                        }
                    }
                    self.ppu_mask.from_write(value);
                }
                0x2002 => {
                    // not writable
                }
                0x2003 => {
                    // OAMADDR
                    debug!(target: "oam", "Writing OAMADDR $2003 [{:02X}]={:02X}",
                        self.ppu.read().unwrap().oam_address, value);
                    self.ppu.write().unwrap().oam_address = value;
                }
                0x2004 => {
                    // OAMDATA
                    let (scanline, _) = {
                        let ppu = self.ppu.read().unwrap();
                        (ppu.scanline, ppu.x)
                    };
                    let rendering_enabled = self.ppu_mask.is_rendering_enabled();
                    let mut ppu = self.ppu.write().unwrap();
                    if rendering_enabled && (scanline < 240 || scanline == 261) {
                        ppu.oam_address = (ppu.oam_address.wrapping_add(4)) & 0xFC;
                    } else {
                        let a = ppu.oam_address;
                        ppu.write_oam(a, value);
                        ppu.oam_address = a.wrapping_add(1);
                    }
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
                    // Schedule cycle-accurate OAM DMA (halt on the next CPU cycle).
                    // Copy happens one get/put pair per cycle so DMC DMA can interleave
                    // (AccuracyCoin DMC DMA + OAM DMA).
                    let oam_addr = self.ppu.read().unwrap().oam_address;
                    self.oam_dma = Some(OamDma::new(value, oam_addr));
                    self.pause_cpu_for_dma = false;
                    {
                        let mut ppu = self.ppu.write().unwrap();
                        ppu.pending_oam_corruption_row = None;
                        ppu.oam_corruption_armed = false;
                    }
                    debug!(
                        target: "oam",
                        "Writing PPU $4014={value:02X}, OAM DMA scheduled from ${:04X}",
                        (value as u16) << 8
                    );
                }
                0x4000..=0x4013 => {
                    self.apu.write().unwrap().set(address, value);
                }
                0x4015 | 0x4017 => {
                    self.apu.write().unwrap().set(address, value);
                }
                // Joypad strobe (both controllers share the $4016 strobe line).
                0x4016 => {
                    let on_put = !self.apu.read().unwrap().is_get_phase();
                    self.joypad.write().unwrap().write(value, on_put);
                    self.joypad2.write().unwrap().write(value, on_put);
                }
                _ => {
                }
            }
            // PPU open bus is only driven by PPU register access ($2000–$2007),
            // not APU/IO ($4000+). STA $2002 to seed open bus for $3FFE execution
            // (AccuracyCoin APU Register Activation) must survive later $401x writes.
            if (0x2000..=0x2007).contains(&address) {
                self.ppu.write().unwrap().set_open_bus(value);
            }
        }
        if address != 0x2002 {
            self.set_force(address, value);
        }
    }
}

const REGISTER_LOWER_BOUND: u16 = 0x4800;

#[cfg(test)]
mod open_bus_tests {
    use super::*;
    use cpu::memory::Memory;

    #[test]
    fn lda_absolute_open_bus_returns_high_address_byte() {
        let mut mem = NesMemory::new_for_testing();
        // Simulate the bus activity of LDA $4000:
        // read opcode, low, high — last value on the bus is $40.
        let _ = mem.get(0x8000); // dummy (would be opcode)
        mem.cpu_data_bus = 0x40; // absolute address high byte
        assert_eq!(mem.get(0x4000), 0x40);
    }

    #[test]
    fn dmc_dma_sample_updates_open_bus() {
        let mut mem = NesMemory::new_for_testing();
        mem.cpu_data_bus = 0x40;
        // Simulate DMC DMA reading a $00 sample byte.
        mem.cpu_data_bus = 0x00;
        assert_eq!(mem.get(0x4000), 0x00);
    }
}

impl  Memory for NesMemory{
    fn get(&mut self, address: u16) -> u8 {
        // CPU map: $0000–$401F via get2 (RAM/PPU/APU/io), $4020–$5FFF open bus
        // on NROM (AccuracyCoin open-bus pre-tests), $6000–$7FFF cart WRAM,
        // $8000–$FFFF PRG ROM.
        //
        // APU register activation (AccuracyCoin): when the 6502 address bus is
        // in $4000–$401F (e.g. CPU halted mid-instruction there during OAM),
        // APU $4015/$4016/$4017 are readable as mirrors every $20 bytes.
        // Open-bus pages ($40xx unreadable / $4020–$5FFF): pure APU/controller.
        // RAM pages: $4015 bus-conflicts (bit 5 from RAM); controllers do not
        // override RAM data (test 7), though they may still be clocked.
        let apu_active = !self.oam_dma_fetching
            || (self.cpu_bus_address & 0xFFE0) == 0x4000;
        let apu_mirror = address & 0x1F;
        let result = if self.oam_dma_fetching
            && apu_active
            && matches!(apu_mirror, 0x15 | 0x16 | 0x17)
        {
            let underlying = if address >= 0x8000 {
                self.mapper.read_prg(address)
            } else if address >= 0x6000 {
                self.mapper.read_prg(address)
            } else if address >= 0x4020 {
                self.cpu_data_bus
            } else if address >= 0x4000 {
                self.cpu_data_bus
            } else if address < 0x2000 {
                self.memory[(address & 0x7FF) as usize]
            } else {
                self.cpu_data_bus
            };
            let open_bus_region = address >= 0x4000;
            match apu_mirror {
                0x15 => {
                    // APU drives D0–D4, D6–D7; D5 is open bus → from other device.
                    let status = self.apu.write().unwrap().get(0x4015);
                    (status & 0xDF) | (underlying & 0x20)
                }
                0x16 | 0x17 if open_bus_region => {
                    self.get2(0x4000 | apu_mirror)
                }
                0x16 | 0x17 => {
                    // Still clock the port (some consoles do) but RAM wins the bus.
                    let _ = self.get2(0x4000 | apu_mirror);
                    underlying
                }
                _ => underlying,
            }
        } else if address >= 0x8000 {
            self.mapper.read_prg(address)
        } else if address >= 0x6000 {
            self.mapper.read_prg(address)
        } else if address >= 0x4018 {
            // Expansion area / disabled APU test regs: no device on NROM → CPU open bus.
            self.cpu_data_bus
        } else {
            self.get2(address)
        };
        // Reading $4015 does **not** update the external CPU data bus for a
        // normal CPU access (AccuracyCoin Internal Data Bus). OAM DMA puts the
        // fetched byte on the bus when writing OAM, so mirrors/status reads
        // during OAM *do* update open bus (APU Register Activation table).
        self.cpu_internal_bus = result;
        if address != 0x4015 || self.oam_dma_fetching {
            self.cpu_data_bus = result;
        }
        result
    }

    fn set(&mut self, address: u16, value: u8) {
        // Every bus write drives the CPU data bus.
        self.cpu_data_bus = value;
        self.cpu_internal_bus = value;
        if address >= 0x6000 {
            self.mapper.write_prg(address, value);
        } else if address >= 0x4018 {
            // Open bus region — writes ignored on NROM.
        } else {
            self.set2(address, value);
        }
    }

    fn set_force(&mut self, address: u16, value: u8) {
        if address >= REGISTER_LOWER_BOUND {
            // self.set_force(address, value);
            self.mapper.write_prg(address, value);
        } else {
            self.memory[address as usize] = value;
        }
    }

    fn get_direct(&mut self, address: u16) -> u8 {
        if address >= REGISTER_LOWER_BOUND {
            self.mapper.read_prg(address)
        } else {
            self.memory[address as usize]
        }
    }

    fn main_memory(&mut self) -> Vec<u8> {
        // Full CPU-visible 64K for the debugger. Cartridge space ($6000–$FFFF)
        // is not stored in `self.memory`; it must be read through the mapper
        // (same as `get`), otherwise PRG looks like zeros and disassembly is BRK.
        let mut result = vec![0u8; MEMORY_SIZE];
        for addr in 0u32..MEMORY_SIZE as u32 {
            let a = addr as u16;
            result[addr as usize] = if a >= 0x6000 {
                self.mapper.read_prg(a)
            } else if a >= 0x4020 {
                // Expansion / open-bus region on NROM
                0
            } else if a < 0x2000 {
                self.memory[(a & 0x7FF) as usize]
            } else {
                // PPU/APU/IO backing store (mirrored where applicable)
                let m = Self::cpu_mirrorring(a) as usize;
                if m < self.memory.len() {
                    self.memory[m]
                } else {
                    0
                }
            };
        }
        result
    }

    fn check_nmi(&mut self) -> bool {
        if self.nmi_delay > 0 {
            self.nmi_delay -= 1;
            if self.nmi_delay == 0 {
                self.nmi_requested = true;
            }
            false
        } else if self.nmi_requested {
            self.nmi_requested = false;
            true
        } else {
            false
        }
    }

    fn check_cancel_nmi(&mut self) -> bool {
        if self.nmi_cancel_requested {
            self.nmi_cancel_requested = false;
            true
        } else {
            false
        }
    }
}