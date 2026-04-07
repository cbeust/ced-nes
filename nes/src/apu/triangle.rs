use crate::apu::LENGTH_TABLE;

const TRIANGLE_TABLE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15
];

// Triangle
// https://www.nesdev.org/wiki/APU_Triangle
#[derive(Default, Clone)]
pub struct Triangle {
    reg_ctrl: u8,      // $4008
    reg_timer_lo: u8,  // $400A
    reg_timer_hi: u8,  // $400B

    timer: u16,
    timer_counter: u16,
    sequence_pos: usize,    // 0-31, triangle uses 32-step sequence
    pub length_counter: u8,
    linear_counter: u8,
    linear_reload: bool,
    pub control_flag: bool,

    pub enabled: bool,
}

impl Triangle {
    pub fn set(&mut self, address: u16, val: u8) {
        let a = address & 0x03;
        match a {
            0 => {
                self.reg_ctrl = val;
                self.control_flag = (val & 0x80) != 0;
            }
            2 => {
                self.reg_timer_lo = val;
                self.timer = (self.timer & 0x700) | val as u16;
                // println!("New timer 400a: {}", self.triangle.timer);
            }
            3 => {
                self.set_timer_high(val);
            }
            _ => {}
        }
    }

    pub fn step(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer;
            if self.length_counter > 0 && self.linear_counter > 0 {
                // println!("Triangle sequence pos: {}", self.sequence_pos);
                self.sequence_pos = (self.sequence_pos + 1) & 31;
            }
        } else {
            self.timer_counter -= 1;
        }
    }

    pub fn clock_linear_counter(&mut self) {
        if self.linear_reload {
            self.linear_counter = self.reg_ctrl & 0x7f;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }
        if ! self.control_flag {
            self.linear_reload = false;
        }
    }

    pub fn output(&self) -> u8 {
        if ! self.enabled || self.length_counter == 0 || self.linear_counter == 0 { 0 }
        else { TRIANGLE_TABLE[self.sequence_pos] }
    }

    pub fn set_timer_high(&mut self, val: u8) {
        self.reg_timer_hi = val;
        self.timer = (self.timer & 0xff) | ((val as u16 & 0x07) << 8);
        if self.enabled {
            self.length_counter = LENGTH_TABLE[val as usize >> 3];
        }
        self.linear_reload = true;
        // println!("New timer 400b: {}", self.triangle.timer);
    }
}
