#[derive(Copy, Clone)]
pub enum Button {
    A, // bit 0
    B, // bit 1
    Select, // bit 2
    Start, // bit 3
    Up, // bit 4
    Down, // bit 5
    Left, // bit 6
    Right, // bit 7
}

#[derive(Default)]
pub struct Joypad {
    _strobe: bool,
    button_index: u8,
    button_status: u8,
    /// OUT0 level from $4016 bit 0.
    out0: bool,
    /// OUT0 was high at the end of the previous put cycle (held across gets).
    out0_stable_from_put: bool,
}

impl Joypad {
    pub fn new() -> Self {
        Joypad {
            _strobe: false,
            button_index: 0,
            button_status: 0,
            out0: false,
            out0_stable_from_put: false,
        }
    }

    pub fn write(&mut self, value: u8, on_put: bool) {
        let new_out0 = (value & 1) != 0;
        // AccuracyCoin Controller Strobing with get-synced OAM:
        // - Test 3: high write on get, low on put → latch (via put edge while high)
        // - Test 4: high write on put, low on get → no latch
        // - Multi-cycle: put edges while OUT0 held high keep reloading
        if !new_out0 && self.out0 {
            // Falling edge: clear put-stable marker
            self.out0_stable_from_put = false;
        }
        self.out0 = new_out0;
        self._strobe = new_out0;
        let _ = on_put;
    }

    pub fn on_put_edge(&mut self) {
        // Sample OUT0 at the start of put. Latch if already high (held from a
        // previous get write or prior put). A put-cycle write of 1 happens
        // *after* this edge, so test 4's put-high does not latch here.
        if self.out0 {
            self.button_index = 0;
            self.out0_stable_from_put = true;
        }
    }

    pub fn end_put_cycle(&mut self) {
        let _ = self.out0_stable_from_put;
    }

    pub fn is_out0(&self) -> bool {
        self.out0
    }

    pub fn read_status(&self) -> u8 {
        self.button_status
    }

    /// Serial read of the controller shift register (bit 0).
    /// After 8 clocks, subsequent reads return 1 until the next strobe.
    pub fn read(&mut self) -> u8 {
        if self._strobe {
            // While strobed, always report button A (bit 0 of status)
            self.button_index = 0;
            return self.button_status & 1;
        }
        let result = if self.button_index < 8 {
            ((self.button_status >> self.button_index) & 1) as u8
        } else {
            1
        };
        // Advance shift; do not wrap — stay past 8 so further reads keep returning 1.
        if self.button_index < 9 {
            self.button_index += 1;
        }
        result
    }

    fn index(&mut self, button: Button) -> u8 {
        match button {
            Button::A => { 0 }
            Button::B => { 1 }
            Button::Select => { 2 }
            Button::Start => { 3 }
            Button::Up => { 4 }
            Button::Down => { 5 }
            Button::Left => { 6 }
            Button::Right => { 7 }
        }
    }

    pub fn set_button_status(&mut self, button: Button, status: bool) {
        if status {
            self.button_status |= 1 << self.index(button);
        } else {
            self.button_status &= !(1 << self.index(button));
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    // #[test]
    pub fn test_strobe_mode() {
        let mut joypad = Joypad::new();
        joypad.write(1, true);
        joypad.set_button_status(Button::A, true);
        for _x in 0..10 {
            assert_eq!(joypad.read(), 1);
        }
    }

    // #[test]
    pub fn test_strobe_mode_on_off() {
        let mut joypad = Joypad::new();

        joypad.write(0, true);
        joypad.set_button_status(Button::Right, true);
        joypad.set_button_status(Button::Left, true);
        joypad.set_button_status(Button::Select, true);
        joypad.set_button_status(Button::B, true);

        for _ in 0..=1 {
            assert_eq!(joypad.read(), 0);
            assert_eq!(joypad.read(), 1);
            assert_eq!(joypad.read(), 1);
            assert_eq!(joypad.read(), 0);
            assert_eq!(joypad.read(), 0);
            assert_eq!(joypad.read(), 0);
            assert_eq!(joypad.read(), 1);
            assert_eq!(joypad.read(), 1);

            for _x in 0..10 {
                assert_eq!(joypad.read(), 1);
            }
            joypad.write(1, true);
            joypad.write(0, true);
        }
    }
}