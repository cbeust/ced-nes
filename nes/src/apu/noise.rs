use crate::apu::envelope::Envelope;
use crate::apu::LENGTH_TABLE;

///
/// Noise
/// https://www.nesdev.org/wiki/APU_Noise
///
#[derive(Clone, Default)]
pub struct Noise {
    pub reg_ctrl: u8, // $400c
    reg_period: u8, // $400e
    reg_length: u8, // $400f

    shift_reg: u16, // 15 (fifteen) bit shift register
    timer: u16,
    timer_counter: u16,
    pub length_counter: u8,
    envelope: Envelope,
    // Feedback is calculated as the exclusive-OR of bit 0 and one other bit:
    // bit 6 if Mode flag is set, otherwise bit 1.
    mode: bool,
    enabled: bool,
}

impl Noise {
    pub fn new() -> Self {
        let mut result = Self::default();
        result.shift_reg = 1;
        result
    }

    pub fn set(&mut self, address: u16, val: u8) {
        let a = address & 0x03;
        match a {
            0 => { self.reg_ctrl = val; }
            2 => {
                self.reg_period = val;
                self.mode = (val & 0x80) != 0;
                self.timer = NOISE_PERIOD_TABLE[(val & 0x0F) as usize];
            }
            3 => {
                self.reg_length = val;
                if self.enabled {
                    self.length_counter = LENGTH_TABLE[val as usize >> 3];
                }
                self.envelope.set_start(true);
            }
            _ => {}
        }
    }

    pub fn clock_envelope(&mut self) {
        self.envelope.clock(self.reg_ctrl);
    }

    pub fn output(&self) -> u8 {
        if !self.enabled || self.length_counter == 0 || (self.shift_reg & 1) != 0 {
            return 0;
        }

        self.envelope.output(self.reg_ctrl)
    }

    /// The shift register is 15 bits wide, with bits numbered
    /// 14 - 13 - 12 - 11 - 10 - 9 - 8 - 7 - 6 - 5 - 4 - 3 - 2 - 1 - 0
    ///
    /// When the timer clocks the shift register, the following actions occur in order:
    ///
    /// Feedback is calculated as the exclusive-OR of bit 0 and one other bit:
    /// bit 6 if Mode flag is set, otherwise bit 1.
    /// The shift register is shifted right by one bit.
    /// Bit 14, the leftmost bit, is set to the feedback calculated earlier
    pub fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer;
            let bit = if self.mode { 6 } else { 1 };
            let feedback = (self.shift_reg & 1) ^ ((self.shift_reg >> bit) & 1);
            self.shift_reg >>= 1;
            self.shift_reg |= feedback << 14;
        } else {
            self.timer_counter -= 1;
        }
    }

    pub fn set_enabled(&mut self, v: bool) {
        self.enabled = v;
        if ! v { self.length_counter = 0; }
    }
}

// Noise period lookup table
const NOISE_PERIOD_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068
];

