use tracing::info;
use cpu::memory::Memory;
use crate::nes_memory::NesMemory;

///
/// DMC
/// https://www.nesdev.org/wiki/APU_DMC
///
#[derive(Clone, Default)]
pub struct Dmc {
    // $4010
    pub irq_enabled: bool,
    pub loop_enabled: bool,
    pub rate_index: usize,
    pub timer: u16,
    pub current_timer: u16,

    // $4011
    pub output_level: u8, // 7-bit

    // $4012
    pub sample_address: u16,

    // $4013
    pub sample_length: u16,

    // Internal state
    pub current_address: u16,
    pub current_length: u16,

    pub sample_buffer: Option<u8>,
    pub shift_register: u8,
    pub bits_remaining: u8, // 0-8
    pub silence_flag: bool,

    pub irq_flag: bool,
}

impl Dmc {
    pub fn set(&mut self, address: u16, val: u8) {
        match address & 0x03 {
            0 => {
                // $4010 - IL--.RRRR
                self.irq_enabled = (val & 0x80) != 0;
                if !self.irq_enabled {
                    self.irq_flag = false;
                }
                self.loop_enabled = (val & 0x40) != 0;
                self.rate_index = (val & 0x0F) as usize;
                self.timer = RATES[self.rate_index];
                // Note: The timer is NOT reset immediately by $4010
            }
            1 => {
                // $4011 - -DDD.DDDD
                self.output_level = val & 0x7F;
            }
            2 => {
                // $4012 - AAAA.AAAA
                // Sample address = %11AAAAAA.AA000000 = $C000 + (A * 64)
                self.sample_address = 0xC000 | ((val as u16) << 6);
            }
            3 => {
                // $4013 - LLLL.LLLL
                // Sample length = %LLLL.LLLL0001 = (L * 16) + 1 bytes
                self.sample_length = ((val as u16) << 4) | 1;
            }
            _ => {}
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.current_length = 0;
        } else if self.current_length == 0 {
            self.current_address = self.sample_address;
            self.current_length = self.sample_length;
        }
        self.irq_flag = false;
    }

    fn fill_sample_buffer(&mut self, memory: &mut NesMemory) {
        if self.sample_buffer.is_none() && self.current_length > 0 {
            // TODO: CPU should stall for 4 cycles
            self.sample_buffer = Some(memory.get(self.current_address));

            if self.current_address == 0xFFFF {
                self.current_address = 0x8000;
            } else {
                self.current_address += 1;
            }

            self.current_length -= 1;
            if self.current_length == 0 {
                if self.loop_enabled {
                    self.current_address = self.sample_address;
                    self.current_length = self.sample_length;
                } else if self.irq_enabled {
                    self.irq_flag = true;
                }
            }
        }
    }

    pub fn step(&mut self, memory: &mut NesMemory) {
        // 1. Memory Reader
        self.fill_sample_buffer(memory);

        // 2. Timer
        if self.current_timer > 0 {
            self.current_timer -= 1;
        } else {
            self.current_timer = self.timer;

            // 3. Output Unit
            if !self.silence_flag {
                let bit = self.shift_register & 0x01;
                if bit == 1 {
                    if self.output_level <= 125 {
                        self.output_level += 2;
                    }
                } else {
                    if self.output_level >= 2 {
                        self.output_level -= 2;
                    }
                }
            }

            self.shift_register >>= 1;
            if self.bits_remaining > 0 {
                self.bits_remaining -= 1;
            }

            if self.bits_remaining == 0 {
                self.bits_remaining = 8;
                if let Some(val) = self.sample_buffer.take() {
                    self.shift_register = val;
                    self.silence_flag = false;
                } else {
                    self.silence_flag = true;
                }
            }
        }
    }

    pub fn output(&self) -> u8 {
        self.output_level
    }

    pub fn is_active(&self) -> bool {
        self.current_length > 0
    }
}

// NTSC DMC rates (cycles between steps)
const RATES: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214,
    190, 160, 142, 128, 106, 84, 72, 54
];
