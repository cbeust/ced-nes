#[derive(Default, Clone)]
pub struct Envelope {
    start: bool,
    volume: u8,
    counter: u8,
}

impl Envelope {
    pub fn clock(&mut self, reg_ctrl: u8) {
        if self.start {
            self.start = false;
            self.volume = 15;
            self.counter = reg_ctrl & 0x0F;
        } else {
            if self.counter > 0 {
                self.counter -= 1;
            } else {
                self.counter = reg_ctrl & 0x0F;
                if self.volume > 0 {
                    self.volume -= 1;
                } else if (reg_ctrl & 0x20) != 0 {
                    self.volume = 15;
                }
            }
        }
    }

    pub fn volume(&self) -> u8 {
        self.volume
    }

    pub fn set_start(&mut self, v: bool) {
        self.start = v;
    }
}