use crate::apu::envelope::Envelope;
use crate::apu::LENGTH_TABLE;

const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0], // 12.5%
    [0, 1, 1, 0, 0, 0, 0, 0], // 25%
    [0, 1, 1, 1, 1, 0, 0, 0], // 50%
    [1, 0, 0, 1, 1, 1, 1, 1], // 25% negated
];

#[derive(Default, Clone)]
pub struct Pulse {
    // register values (raw writes)
    pub reg_ctrl: u8,      // $4000/$4004 - duty, loop, constant vol, volume
    pub reg_sweep: u8,     // $4001/$4005 - sweep unit
    pub reg_timer_lo: u8,  // $4002/$4006 - timer low
    pub reg_timer_hi: u8,  // $4003/$4007 - length counter load, timer high

    // internal state
    pub timer: u16,          // 11-bit timer (period)
    pub timer_counter: u16,  // counts down
    pub duty_pos: usize,        // where we are in the duty cycle (0-7)
    pub length_counter: u8,  // counts down, channel silenced when 0
    pub envelope: Envelope,

    // sweep unit
    pub sweep_reload: bool,
    pub sweep_counter: u8,
    pub sweep_enabled: bool,
    pub sweep_negate: bool,
    pub sweep_period: u8,
    pub sweep_shift: u8,

    pub enabled: bool,
}

impl Pulse {
    pub fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer;
            self.duty_pos = (self.duty_pos + 1) & 7;
        } else {
            self.timer_counter -= 1;
        }
    }

    pub fn clock_envelope(&mut self) {
        self.envelope.clock(self.reg_ctrl);
    }

    pub fn clock_sweep(&mut self, is_pulse1: bool) {
        let change = self.timer >> self.sweep_shift;
        let mut target = 0;
        if self.sweep_negate {
            target = self.timer - change;
            if is_pulse1 {
                target -= 1;
            }
        } else {
            target = self.timer + change;
        }
        let _mute = target > 0x7ff || self.timer == 8;

        if self.sweep_counter == 0 && self.sweep_enabled && ! _mute && self.sweep_shift > 0 {
            self.timer = target;
        }
        if self.sweep_counter == 0 || self.sweep_reload {
            self.sweep_counter = self.sweep_period;
            self.sweep_reload = false;
        } else {
            self.sweep_counter -= 1;
        }
    }

    pub fn output(&self) -> u8 {
        if ! self.enabled || self.length_counter == 0 || self.timer < 8 {
            return 0;
        }
        let duty = (self.reg_ctrl >> 6) & 0x03;
        let duty_out = DUTY_TABLE[duty as usize][self.duty_pos];
        if duty_out == 0 { return 0; }

        // Constant volume of envelop
        if self.reg_ctrl & 0x10 != 0 {
            self.reg_ctrl & 0x0F
        } else {
            self.envelope.volume()
        }
    }

    pub fn sweep_control(&mut self, val: u8) {
        self.reg_sweep = val;
        self.sweep_enabled = (val & 0x80) != 0;
        self.sweep_period = (val >> 4) & 0x07;
        self.sweep_negate = (val & 0x08) != 0;
        self.sweep_shift = val & 0x07;
        self.sweep_reload = true;
    }

    pub fn set_timer_high(&mut self, val: u8) {
        self.reg_timer_hi = val;
        self.timer = (self.timer & 0xFF) | ((val as u16 & 0x07) << 8);
        if self.enabled {
            self.length_counter = LENGTH_TABLE[val as usize >> 3];
        }
        self.duty_pos = 0;
        self.envelope.set_start(true);
    }
}
