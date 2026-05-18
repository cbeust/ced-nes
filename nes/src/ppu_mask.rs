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

/// How many PPU dots before a rendering-enable takes effect.
/// Disabling rendering is immediate; enabling is delayed.
/// Background: 3 dots, Sprites: 4 dots (per NESDev hardware behaviour).
const BG_RENDER_DELAY: u8 = 3;
const SPRITE_RENDER_DELAY: u8 = 4;

#[derive(Clone, Copy)]
pub struct PpuMask {
    emphasize_blue: bool,
    emphasize_green: bool,
    emphasize_red: bool,
    /// The *effective* rendering state – what the PPU is actually using right now.
    sprite_rendering_effective: bool,
    background_rendering_effective: bool,
    /// Pending change: (target_value, dots_remaining).
    /// Set when $2001 is written; applied once dots_remaining reaches 0.
    sprite_rendering_pending: Option<(bool, u8)>,
    background_rendering_pending: Option<(bool, u8)>,
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

impl PpuMask {
    /// Create a PpuMask that takes effect immediately (used at initialisation / reset).
    pub fn new(value: u8) -> Self {
        let emphasize_blue  = 0 != (value & (1 << 7));
        let emphasize_green = 0 != (value & (1 << 6));
        let emphasize_red   = 0 != (value & (1 << 5));
        let sprite_rendering_effective     = 0 != (value & (1 << 4));
        let background_rendering_effective = 0 != (value & (1 << 3));
        let clip_sprites    = 0 == (value & (1 << 2));
        let clip_background = 0 == (value & (1 << 1));
        let greyscale       = 0 != (value & (1 << 0));

        Self {
            emphasize_blue, emphasize_green, emphasize_red,
            sprite_rendering_effective,
            background_rendering_effective,
            sprite_rendering_pending: None,
            background_rendering_pending: None,
            clip_sprites, clip_background, greyscale,
        }
    }

    /// Apply a CPU write to $2001.  Non-rendering bits take effect immediately;
    /// rendering bits are deferred by 3 (BG) / 4 (sprites) PPU dots.
    pub fn from_write(&mut self, value: u8) {
        self.emphasize_blue  = 0 != (value & (1 << 7));
        self.emphasize_green = 0 != (value & (1 << 6));
        self.emphasize_red   = 0 != (value & (1 << 5));
        self.clip_sprites    = 0 == (value & (1 << 2));
        self.clip_background = 0 == (value & (1 << 1));
        self.greyscale       = 0 != (value & (1 << 0));

        let new_bg     = 0 != (value & (1 << 3));
        let new_sprite = 0 != (value & (1 << 4));

        // Enabling rendering is delayed (latched after 3 BG / 4 sprite PPU dots).
        // Disabling rendering takes effect immediately.
        if new_bg != self.background_rendering_effective {
            if new_bg {
                self.background_rendering_pending = Some((new_bg, BG_RENDER_DELAY));
            } else {
                self.background_rendering_effective = false;
                self.background_rendering_pending = None;
            }
        }
        if new_sprite != self.sprite_rendering_effective {
            if new_sprite {
                self.sprite_rendering_pending = Some((new_sprite, SPRITE_RENDER_DELAY));
            } else {
                self.sprite_rendering_effective = false;
                self.sprite_rendering_pending = None;
            }
        }
    }

    /// Called once per PPU dot to advance the pending-change counters.
    /// Decrement first, then apply: so a delay of N means the change takes
    /// effect exactly N PPU dots after the $2001 write (matching hardware).
    pub fn tick(&mut self) {
        if let Some((target, ref mut dots)) = self.background_rendering_pending {
            *dots -= 1;
            if *dots == 0 {
                self.background_rendering_effective = target;
                self.background_rendering_pending = None;
            }
        }
        if let Some((target, ref mut dots)) = self.sprite_rendering_pending {
            *dots -= 1;
            if *dots == 0 {
                self.sprite_rendering_effective = target;
                self.sprite_rendering_pending = None;
            }
        }
    }

    pub fn background_rendering(&self) -> bool {
        self.background_rendering_effective
    }

    pub fn sprite_rendering(&self) -> bool {
        self.sprite_rendering_effective
    }

    pub fn greyscale(&self) -> bool {
        self.greyscale
    }
}
