use std::fmt::{Display, Formatter};
use std::io::Bytes;
use std::ops::Index;

#[derive(Clone, Copy)]
pub struct Pattern {
    bytes: [u8; 64]
}

impl Pattern {
    pub fn new(bytes: [u8; 64]) -> Pattern {
        Self {
            bytes
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.iter().all(|&x| x == 0)
    }
}

impl Into<String> for Pattern {
    fn into(self) -> String {
        let mut result = String::new();
        for y in 0..8 {
            for x in 0..8 {
                // Should be 0|1|2|3
                let color = self.bytes[y * 8 + x];
                if color == 0 { result.push('.'); }
                else { result.push('#'); }
            }
            result.push('\n');
        }
        result
    }
}

impl Display for Pattern {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for y in 0..8 {
            for x in 0..8 {
                // Should be 0|1|2|3
                let color = self.bytes[y * 8 + x];
                if color == 0 {
                    write!(f, ".")?;
                } else {
                    write!(f, "#")?;
                }
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

impl Index<usize> for Pattern {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.bytes[index]
    }
}
