use crate::apu::LENGTH_TABLE;

const TRIANGLE_TABLE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15
];

#[derive(Default, Clone)]
pub struct Triangle {
    pub reg_ctrl: u8,      // $4008
    pub reg_timer_lo: u8,  // $400A
    pub reg_timer_hi: u8,  // $400B

    pub timer: u16,
    pub timer_counter: u16,
    pub sequence_pos: usize,    // 0-31, triangle uses 32-step sequence
    pub length_counter: u8,
    pub linear_counter: u8,
    pub linear_reload: bool,
    pub control_flag: bool,

    pub enabled: bool,
}

impl Triangle {
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
        if ! self.enabled { 0 }
        else if self.length_counter == 0 { 0 }
        else if self.linear_counter == 0 { 0 }
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
