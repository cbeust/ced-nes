use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::num::ParseIntError;
use std::path::Path;
use zip::ZipArchive;

#[derive(Debug, Default, Clone)]
pub struct ControllerState {
    pub right: bool,
    pub left: bool,
    pub down: bool,
    pub up: bool,
    pub start: bool,
    pub select: bool,
    pub b: bool,
    pub a: bool,
}

impl ControllerState {
    /// Returns `true` when no buttons are pressed.
    pub fn is_empty(&self) -> bool {
        !self.right && !self.left && !self.down && !self.up
            && !self.start && !self.select && !self.b && !self.a
    }
}

impl fmt::Display for ControllerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut btns = String::new();
        if self.up     { btns.push('U'); }
        if self.down   { btns.push('D'); }
        if self.left   { btns.push('L'); }
        if self.right  { btns.push('R'); }
        if self.start  { btns.push('S'); }
        if self.select { btns.push('s'); }
        if self.b      { btns.push('B'); }
        if self.a      { btns.push('A'); }
        if btns.is_empty() { btns.push('.'); }
        write!(f, "{}", btns)
    }
}

#[derive(Debug, Default)]
pub struct Bk2Movie {
    pub header: HashMap<String, String>,
    pub frames: Vec<ControllerState>,
    index: usize,
}

impl Bk2Movie {
    /// Open a `.bk2` zip file and parse it.
    pub fn parse_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let mut movie = Bk2Movie::default();

        // Parse Headers.txt
        if let Ok(mut entry) = archive.by_name("Header.txt") {
            let mut contents = String::new();
            entry.read_to_string(&mut contents)?;
            movie.header = Self::parse_headers(&contents);
        }

        // Parse "Input Log.txt" (note the space — real BK2 convention)
        if let Ok(mut entry) = archive.by_name("Input Log.txt") {
            let mut contents = String::new();
            entry.read_to_string(&mut contents)?;
            movie.frames = Self::parse_input_log(&contents);
        }

        Ok(movie)
    }

    /// Parse the `Header.txt` key-value pairs.
    fn parse_headers(input: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if let Some((key, value)) = line.split_once(' ') {
                map.insert(key.to_string(), value.to_string());
            }
        }
        map
    }

    /// Parse the `Input Log.txt` file into a `Vec<ControllerState>`.
    ///
    /// Real BK2 format (no section markers, directly pipe-delimited):
    /// ```text
    /// |..|........|........|
    /// |..|...R....|........|
    /// ```
    /// Column layout after splitting on `|`:
    ///   [0] = ""  (before first pipe)
    ///   [1] = 2-char system/reset flags
    ///   [2] = 8-char P1 buttons: U D L R S s B A
    ///   [3] = 8-char P2 buttons
    ///
    /// A letter at the expected position means pressed; `.` means released.
    fn parse_input_log(input: &str) -> Vec<ControllerState> {
        let mut frames = Vec::new();

        for line in input.lines() {
            let line = line.trim();
            // Every frame line starts and ends with '|'
            if !line.starts_with('|') {
                continue;
            }

            // e.g. |..|UDLR.s..|........|
            let cols: Vec<&str> = line.split('|').collect();
            // Need at least cols[2] for P1 buttons
            if cols.len() < 3 {
                continue;
            }
            let btns = cols[2].as_bytes();

            // BK2 NES P1 button order: U D L R S s B A
            let pressed = |i: usize, c: u8| btns.get(i).map_or(false, |&b| b == c);

            frames.push(ControllerState {
                up:     pressed(0, b'U'),
                down:   pressed(1, b'D'),
                left:   pressed(2, b'L'),
                right:  pressed(3, b'R'),
                start:  pressed(4, b'S'),
                select: pressed(5, b's'),
                b:      pressed(6, b'B'),
                a:      pressed(7, b'A'),
            });
        }

        frames
    }

    /// Get a header value by key.
    pub fn _get_header(&self, key: &str) -> Option<&str> {
        self.header.get(key).map(|s| s.as_str())
    }

    /// SHA1 or MD5 hash of the ROM.
    pub fn _rom_checksum(&self) -> Result<u32, ParseIntError> {
        let s = self._get_header("SHA1").or_else(|| self._get_header("MD5"));
        u32::from_str_radix(s.unwrap_or(""), 16)
    }

    /// Emulator system/core name.
    pub fn _system(&self) -> Option<&str> {
        self._get_header("System")
    }

    /// Total number of input frames.
    pub fn _frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Advance and return the next frame's controller state, or `None` when
    /// the movie is finished.
    pub fn next_state(&mut self) -> Option<ControllerState> {
        if self.index >= self.frames.len() {
            None
        } else {
            let result = self.frames[self.index].clone();
            self.index += 1;
            Some(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_input_log() {
        // Real format: |<2-char flags>|<8-char P1>|<8-char P2>|
        let log = "|..|UDLR.s.A|........|\n|..|........|........|\n";
        let frames = Bk2Movie::parse_input_log(log);
        assert_eq!(frames.len(), 2);
        // Frame 0: U D L R s(select) A pressed
        assert!(frames[0].up);
        assert!(frames[0].down);
        assert!(frames[0].left);
        assert!(frames[0].right);
        assert!(!frames[0].start); // position 4 = '.' → not pressed
        assert!(frames[0].select); // position 5 = 's'
        assert!(!frames[0].b);
        assert!(frames[0].a);
        // Frame 1: nothing pressed
        assert!(!frames[1].up);
        assert!(!frames[1].a);
    }

    #[test]
    fn test_parse_headers() {
        let hdrs = "System NES\nSHA1 abc123\n";
        let map = Bk2Movie::parse_headers(hdrs);
        assert_eq!(map.get("System").map(|s| s.as_str()), Some("NES"));
        assert_eq!(map.get("SHA1").map(|s| s.as_str()), Some("abc123"));
    }
}

