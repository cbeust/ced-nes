use std::fmt::Display;
use std::fs;
use std::io;
use std::path::Path;
use crate::rom::Rom;

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
    pub fn _is_empty(&self) -> bool {
        !self.right && !self.left && !self.down && !self.up && !self.start && !self.select && !self.b && !self.a
    }
}

impl Display for ControllerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        if self.right { s.push_str("Right "); }
        if self.left { s.push_str("Left "); }
        if self.down { s.push_str("Down "); }
        if self.up { s.push_str("Up "); }
        if self.start { s.push_str("Start "); }
        if self.select { s.push_str("Select "); }
        if self.b { s.push_str("B "); }
        if self.a { s.push_str("A "); }
        write!(f, "{}", s)
    }
}

#[derive(Debug)]
pub struct Fm2Movie {
    pub rom_md5: String,
    pal: bool,
    frames: Vec<ControllerState>,
    _index: usize,
}

impl Fm2Movie {
    pub fn parse_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let input = fs::read_to_string(path)?;
        let result = Self::parse(&input);
        println!("File contains {} frames", result.frames.len());
        Ok(result)
    }

    pub fn _next_state(&mut self) -> Option<ControllerState> {
        if self._index >= self.frames.len() {
            None
        } else {
            let result = self.frames[self._index].clone();
            self._index += 1;
            Some(result)
        }   
    }

    pub fn rom_checksum(&self) -> Option<&str> {
        let checksum = self.rom_md5.trim();
        if checksum.is_empty() {
            None
        } else {
            Some(checksum)
        }
    }

    pub fn rom_checksum_matches(&self, rom: &Rom) -> Option<bool> {
        self.rom_checksum()
            .map(|movie_checksum| movie_checksum.eq_ignore_ascii_case(rom.checksum.as_str()))
    }

    pub fn parse(input: &str) -> Self {
        let mut movie = Fm2Movie {
            rom_md5: String::new(),
            pal: false,
            frames: Vec::new(),
            _index: 0,
        };

        for line in input.lines() {
            if line.starts_with("romChecksum") {
                movie.rom_md5 = line.strip_prefix("romChecksum")
                    .map(str::trim)
                    .map(str::to_string)
                    .unwrap_or_default();
            } else if line.starts_with("palFlag 1") {
                movie.pal = true;
            } else if line.starts_with('|') {
                // e.g. "|0|RLDUTSBA|........|"
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 3 {
                    let btns = parts[2].as_bytes();
                    let get = |i: usize, c: u8| btns.get(i).map_or(false, |&b| b == c);
                    movie.frames.push(ControllerState {
                        right:  get(0, b'R'),
                        left:   get(1, b'L'),
                        down:   get(2, b'D'),
                        up:     get(3, b'U'),
                        start:  get(4, b'T'),
                        select: get(5, b'S'),
                        b:      get(6, b'B'),
                        a:      get(7, b'A'),
                    });
                }
            }
        }

        movie
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rom_checksum_header() {
        let movie = Fm2Movie::parse("romChecksum   base64:bUqUw0RGPlYjRCSeGOm5nw==\n");
        assert_eq!(movie.rom_checksum(), Some("base64:bUqUw0RGPlYjRCSeGOm5nw=="));
    }

    #[test]
    fn test_rom_checksum_matches_is_case_insensitive() {
        let movie = Fm2Movie::parse("romChecksum base64:BUQUW0RGPLYJRCSEGOM5NW==\n");
        let rom = Rom {
            checksum: "base64:bUqUw0RGPlYjRCSeGOm5nw==".to_string(),
            ..Rom::default()
        };

        assert_eq!(movie.rom_checksum_matches(&rom), Some(true));
    }
}

