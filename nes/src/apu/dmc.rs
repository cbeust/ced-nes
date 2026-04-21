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
    irq_enabled: bool,
    loop_enabled: bool,
    rate_index: usize,
    rate: u16,
    current_rate: u16,

    // $4011
    output: u8, // 7-bit

    // $4012
    sample_address: u16,

    // $4013
    sample_length: u16,

    // Internal state
    current_address: u16,
    current_length: u16,

    /// The current sample being shifted
    sample_buffer: Option<u8>,
    shift_register: u8,
    bits_remaining: u8, // 0-8
    silence_flag: bool,

    // Memory reader DMA timing: emulate a fixed fetch latency.
    dma_delay: u8,

    pub irq_flag: bool,
}

impl Dmc {
    pub fn set(&mut self, address: u16, val: u8) {
        match address & 0x03 {
            0 => {
                // $4010 - IL--.RRRR
                self.irq_enabled = (val & 0x80) != 0;
                info!(target: "asm", "Writing to $4010, DMC IRQ enabled={}", self.irq_enabled);
                if !self.irq_enabled {
                    self.irq_flag = false;
                }
                self.loop_enabled = (val & 0x40) != 0;
                self.rate_index = (val & 0x0F) as usize;
                self.rate = RATES[self.rate_index];
                // Note: The timer is NOT reset immediately by $4010
            }
            1 => {
                // $4011 - -DDD.DDDD
                self.output = val & 0x7F;
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
            self.dma_delay = 0;
        } else if self.current_length == 0 {
            self.current_address = self.sample_address;
            self.current_length = self.sample_length;
        }
        self.irq_flag = false;
    }

    fn clock_memory_reader(&mut self, memory: &mut NesMemory) {
        if self.dma_delay > 0 {
            self.dma_delay -= 1;
            if self.dma_delay == 0 {
                self.complete_sample_fetch(memory);
            }
            return;
        }

        if self.sample_buffer.is_none() && self.current_length > 0 {
            // Approximate DMC DMA transfer delay before the byte becomes available.
            self.dma_delay = 3;
        }
    }

    fn complete_sample_fetch(&mut self, memory: &mut NesMemory) {
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
                // IRQ is raised when the last DMA fetch completes.
                self.irq_flag = true;
            }
        }
    }

    pub fn step(&mut self, memory: &mut NesMemory) -> bool {
        // 1. Memory Reader
        self.clock_memory_reader(memory);

        // 2. Timer
        if self.current_rate > 0 {
            self.current_rate -= 1;
        } else {
            self.current_rate = self.rate;

            // 3. Output Unit
            if !self.silence_flag {
                let bit = self.shift_register & 0x01;
                if bit == 1 {
                    if self.output <= 125 {
                        self.output += 2;
                    }
                } else {
                    if self.output >= 2 {
                        self.output -= 2;
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
        self.irq_enabled && self.irq_flag
    }

    pub fn output(&self) -> u8 {
        self.output
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
