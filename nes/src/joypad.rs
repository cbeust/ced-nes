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
}

impl Joypad {
    pub fn new() -> Self {
        Joypad {
            _strobe: false,
            button_index: 0,
            button_status: 0,
        }
    }

    pub fn write(&mut self, _value: u8) {
        // When strobe is written, reset the button index
        self.button_index = 0;
    }

    pub fn read_status(&self) -> u8 {
        self.button_status
    }

    /// Return the status of the next button pressed.
    /// If the strobe is on, the next button will be returned.
    /// If the strobe is off, the next button will be returned.
    /// If the strobe is off and no button is pressed, 1 is returned.
    pub fn read(&mut self) -> u8 {
        let result = if self.button_index < 8 {
            // Get the button state for current index
            ((self.button_status >> self.button_index) & 1) as u8
        } else {
            // After 8 reads, return 1 (indicating no button pressed)
            1
        };

        self.button_index += 1;
        if self.button_index > 7 {
            self.button_index = 0;
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
        joypad.write(1);
        joypad.set_button_status(Button::A, true);
        for _x in 0..10 {
            assert_eq!(joypad.read(), 1);
        }
    }

    // #[test]
    pub fn test_strobe_mode_on_off() {
        let mut joypad = Joypad::new();

        joypad.write(0);
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
            joypad.write(1);
            joypad.write(0);
        }
    }
}