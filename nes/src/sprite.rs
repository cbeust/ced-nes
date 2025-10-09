use std::collections::HashMap;

pub struct Sprite<'a> {
    x: u8,
    y: u8,
    flip_x: bool,
    flip_y: bool,
    attribute: u8,
    tile_index: u8,
    bytes: &'a Vec<u8>  // Should actually be 16 for 8x8 sprites, 32 for 8x16 sprites
}

impl Sprite<'_> {
    pub fn new<'a>(a: &[u8; 4], bytes: &'a Vec<u8>) -> Sprite<'a> {
        Sprite { y: a[0], x: a[3],
            flip_y: a[2] & 0x80 != 0, flip_x: a[2] & 0x40 != 0,
            tile_index: a[1], attribute: a[2], bytes
        }
    }

    /// x and y need to be < 8
    pub fn color_index(&self, mut x: u8, mut y: u8) -> u8 {
        if self.flip_x { x = 7 - x; }
        if self.flip_y { y = 7 - y; }
        let offset = y * 8 + x;
        let byte_index = offset as usize / 8;
        let bit_index = 7 - (offset as usize % 8);
        let mask = 1 << bit_index;
        let byte1 = self.bytes[byte_index];
        let byte2 = self.bytes[byte_index + 8];
        let bit1 = (byte1 & mask) >> bit_index;
        let bit2 = (byte2 & mask) >> bit_index;
        (bit2 << 1) | bit1
    }

    pub fn display(&self) {
        for y in 0..8 {
            for x in 0..8 {
                let c = self.color_index(x, y);
                if c == 0 { print!("."); } else { print!("{c}"); }
            }
            println!("");
        }
        println!("");
    }
}

fn verify(label: &str, sprite: Sprite, expected: Vec<((u8, u8), u8)>) {
    let mut map = HashMap::new();
    expected.iter().for_each(|(key, value)| { map.insert(*key, *value); });


    for y in 0..8 {
        for x in 0..8 {
            let c = sprite.color_index(x, y);
            match map.get(&(x, y)) {
                Some(v) => {
                    assert_eq!(*v, c, "{label}: {x},{y}: Expected color {} but got {c}", *v);
                },
                None => {
                    assert_eq!(0, c, "{label}: {x},{y}: Expected color 0 but got {c}");
                }
            }
        }
    }
}

pub fn test_sprites() {
    let data = [
        ("Sprite", 0, [ ((0, 0), 3) ]),
        ("Mirrored X", 0b0100_0000, [ ((7, 0), 3) ]),
        ("Mirrored Y", 0b1000_0000, [ ((0, 7), 3) ]),
        ("Mirrored XY", 0b1100_0000, [ ((7, 7), 3) ]),
    ];

    for (label, attribute, expected) in data {
        let sprite_spec = &[0x70, 4, attribute, 0x79];
        let sprite_bytes = &vec![0x80, 0, 0, 0, 0, 0, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0];
        let sprite = Sprite::new(sprite_spec, sprite_bytes);
        // sprite.display();
        verify(&label, sprite, expected.into());
    }
}
