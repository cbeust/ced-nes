use std::fmt::{Display, Formatter};

/// PPUMASK
/// 7  bit  0
// ---- ----
/// BGRs bMmG
/// |||| ||||
/// |||| |||+- Greyscale (0: normal color, 1: greyscale)
/// |||| ||+-- 1: Show background in leftmost 8 pixels of screen, 0: Hide
/// |||| |+--- 1: Show sprites in leftmost 8 pixels of screen, 0: Hide
/// |||| +---- 1: Enable background rendering
/// |||+------ 1: Enable sprite rendering
/// ||+------- Emphasize red (green on PAL/Dendy)
/// |+-------- Emphasize green (red on PAL/Dendy)
/// +--------- Emphasize blue
#[derive(Clone, Copy)]
pub struct PpuMask {
    emphasize_blue: bool,
    emphasize_green: bool,
    emphasize_red: bool,
    sprite_rendering_count: u8,
    background_rendering_count: u8,
    pub(crate) clip_sprites: bool,
    pub clip_background: bool,
    greyscale: bool,
}

impl Default for PpuMask {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Display for PpuMask {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("b:{} g:{} r:{} sprite:{} background:{} show_sprites:{} \
            show_bg:{} greyscale:{}",
            self.emphasize_blue, self.emphasize_green, self.emphasize_red,
            self.sprite_rendering(), self.background_rendering(), self.clip_sprites,
            self.clip_background, self.greyscale
        ))
    }
}

const PIXEL_DELAY: u8 = 0;

impl PpuMask {
    fn to_count(value: u8) -> u8 {
        // If set to 0, delay by N pixels before disabling
        if value == 0 { PIXEL_DELAY } else { 0xff }
    }

    pub fn new(value: u8) -> Self {
        let emphasize_blue = 0 != (value & (1 << 7));
        let emphasize_green = 0 != (value & (1 << 6));
        let emphasize_red = 0 != (value & (1 << 5));
        let sprite_rendering_count = Self::to_count(value & (1 << 4));
        let background_rendering_count = Self::to_count(value & (1 << 3));
        let clip_sprites = 0 == (value & (1 << 2));
        let clip_background = 0 == (value & (1 << 1));
        let greyscale = 0 != (value & (1 << 0));

        Self {
            emphasize_blue, emphasize_green, emphasize_red,
            // sprite_rendering, background_rendering,
            sprite_rendering_count, background_rendering_count,
            clip_sprites, clip_background, greyscale
        }
    }

    pub fn background_rendering(&self) -> bool {
        self.background_rendering_count > 0
    }

    pub fn sprite_rendering(&self) -> bool { self.sprite_rendering_count > 0 }

    pub fn update_rendering_counts(&mut self) {
        if self.background_rendering_count > 0 && self.background_rendering_count <= PIXEL_DELAY {
            self.background_rendering_count -= 1;
        }
        if self.sprite_rendering_count > 0 && self.sprite_rendering_count <= PIXEL_DELAY {
            self.sprite_rendering_count -= 1;
        }
    }
}
