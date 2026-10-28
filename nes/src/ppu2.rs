use crate::emulator::FRAME;
use crate::mappers::mapper_base::MapperBase;
use crate::nes_memory::NesMemory;
use cpu::memory::Memory;
use std::time::Instant;
use tracing::{info};
use crate::delayed::Delayed;
use once_cell::sync::Lazy;
use std::sync::RwLock;

#[derive(Default)]
pub struct PpuResult {
    pub vbl: bool,
    pub frame_start: bool,
    pub frame_end: bool,
    pub irq_requested: bool,
}

pub const VRAM_SIZE: usize = 0x4000;

pub const DEFAULT_SPRITE_PALETTE: [u8;16] = [
    0x09, 0x01, 0x34, 0x03, 0x00, 0x04, 0x00, 0x14, 0x08, 0x3A, 0x00, 0x02, 0x00, 0x20, 0x2C, 0x08
];

pub static CURRENT_CYCLE: Lazy<RwLock<u16>> = Lazy::new(|| RwLock::new(0));
pub static CURRENT_SCANLINE: Lazy<RwLock<u16>> = Lazy::new(|| RwLock::new(0));

pub const BIT_SPRITE_OVERFLOW: u8 = 5;
pub const BIT_SPRITE_0_HIT: u8 = 6;
pub const BIT_VBL: u8 = 7;

/// $2000 | (v & $0FFF)
const NT: u16 = 1 << 1;
/// $23C0 | (v & $0C00) | ((v >> 4) & $38) | ((v >> 2) & $07)
const AT: u16 = 1 << 2;
const BG_LSBITS: u16 = 1 << 3;
const BG_MSBITS: u16 = 1 << 4;
const INC_HORIZ_V: u16 = 1 << 5;
const INC_VERT_V: u16 = 1 << 6;
const HORI_V_EQUALS_HORI_T: u16 = 1 << 7;
const SET_VBLANK_FLAG: u16 = 1 << 8;
/// Clear VBL, Sprite 0 hit, and Sprite Overflow flags
const CLEAR_FLAGS: u16 = 1 << 9;
const VERT_V_EQUALS_VERT_T: u16 = 1 << 10;
const SPRITE_LS_BITS: u16 = 1 << 11;
const SPRITE_MS_BITS: u16 = 1 << 12;
/// Phase 1 — Clear (dots 1–64):
/// The PPU writes $FF to all 32 bytes of secondary OAM. This is done by the PPU internally,
/// 2 dots per byte (hence 64 dots to clear all 32 bytes).
const SECONDARY_OAM_CLEAR: u16 = 1 << 13;
/// Phase 2 — Sprite evaluation (dots 65–256):
/// The PPU reads through all 64 sprites in main OAM one by one, checking if each sprite's Y
/// coordinate falls within the next scanline. Specifically it checks:
/// nextScanline >= spriteY && nextScanline < spriteY + spriteHeight
/// where sprite height is 8 or 16 depending on PPUCTRL
const SPRITE_EVALUATION: u16 = 1 << 14;
const END_SPRITE_EVALUATION: u16 = 1 << 15;

const HEIGHT: usize = 262;
const WIDTH: usize = 341;

#[derive(Clone)]
pub struct Ppu2 {
    events: [u16; HEIGHT * WIDTH],
    event_index: usize,
    screen: [u8; crate::constants::WIDTH * crate::constants::HEIGHT],
    delayed_pixels: Delayed<(usize, u8)>,
    _delayed_sprite_colors: Delayed<u8>,

    pub x: u16,
    pub scanline: u16,

    // Where we store the sprites we detected will appear on the next scanline, max 32
    pub(crate) oam2: [u8; 32],
    /// Source primary OAM sprite index (0..63) for each oam2 entry (0..7).
    oam2_source_index: [u8; 8],
    /// Secondary OAM write index during sprite evaluation (0..32 step 4)
    pub oam2_eval_index: usize,
    /// Secondary OAM clear index during dots 1..64 (0..32 step 1)
    oam2_clear_index: usize,
    /// Sprite fetch cursor during dots 261..320 (0..sprite_count)
    sprite_fetch_index: usize,

    /// At most 8 fetched sprite rows for the next scanline.
    pub(crate) sprite_latches: Vec<SpriteLatch>,
    sprite_counting: [bool; 64],

    pub(crate) oam: [u8; 256],
    pub oam_address: u8,
    open_bus: u8,
    open_bus_decay_cycles: u32,

    tile_index: u8,
    pt_lsb: u8,
    pt_msb: u8,
    palette: u8,
    /// NES colour values (0-63) stored at palette RAM indices 0-31
    palette_table: [u8; 32],
    pattern_shift_low: u16,
    pub(crate) pattern_shift_high: u16,
    attr_shift_low: u16,
    attr_shift_high: u16,
    last_screen_sent: Instant,
    sprite_0_hit_delay: i8,
    pub vblank_suppressed: bool,
    pub nmi_suppressed: bool,
    pub oam2_has_sprite_zero: bool,
    pub render_has_sprite_zero: bool,
    eval_byte_index: u8,
    sprite_overflow_delay: i8,
    eval_finished: bool,
    eval_overflow_count: u8,
    pub oam_buffer: u8,
    pub pending_2007_read_delay: u8,
    pub pending_oam_corruption_row: Option<u8>,
    /// True once rendering has actually turned off while a corruption is pending.
    pub oam_corruption_armed: bool,
    pub vbl_cleared_at_dot1: bool,
    pub old_v: u16,
    pub is_odd_frame: bool,
    pub fetched_bg_tile: bool,
}

impl Default for Ppu2 {
    fn default() -> Self {
         Self::new(&mut MapperBase::default())
    }
}

impl Ppu2 {
    pub(crate) fn new(mapper: &mut MapperBase) -> Self {
        let events = Self::init_events();
        let mut result = Self {
            events,
            event_index: 0,
            screen: [0; crate::constants::WIDTH * crate::constants::HEIGHT],
            delayed_pixels: Delayed::new(4),
            _delayed_sprite_colors: Delayed::new(2),
            x: 0,
            scanline: 0,
            oam2: [0; 32],
            oam2_source_index: [0; 8],
            oam2_eval_index: 0,
            oam2_clear_index: 0,
            sprite_fetch_index: 0,
            sprite_latches: Vec::new(),
            sprite_counting: [false; 64],
            oam: [0; 256],
            oam_address: 0,
            open_bus: 0,
            open_bus_decay_cycles: 0,
            tile_index: 0,
            pt_lsb: 0,
            pt_msb: 0,
            palette: 0,
            palette_table: [0; 32],
            pattern_shift_low: 0,
            pattern_shift_high: 0,
            attr_shift_low: 0,
            attr_shift_high: 0,
            last_screen_sent: Instant::now(),
            sprite_0_hit_delay: -1,
            vblank_suppressed: false,
            nmi_suppressed: false,
            oam2_has_sprite_zero: false,
            render_has_sprite_zero: false,
            eval_byte_index: 0,
            sprite_overflow_delay: -1,
            eval_finished: false,
            eval_overflow_count: 0,
            oam_buffer: 0,
            pending_2007_read_delay: 0,
            pending_oam_corruption_row: None,
            oam_corruption_armed: false,
            vbl_cleared_at_dot1: false,
            old_v: 0,
            is_odd_frame: false,
            fetched_bg_tile: false,
        };

        for i in 0..DEFAULT_SPRITE_PALETTE.len() {
            result.set_vram(0x3f10 + i, DEFAULT_SPRITE_PALETTE[i], mapper);
        }

        result
    }

    pub fn tick(&mut self, mut sprite_rendering: bool, mut background_rendering: bool,
        memory: &mut NesMemory) -> PpuResult
    {
        let mut result = PpuResult::default();
        let mut irq_requested = false;
        let event = self.events[self.event_index];
        memory.total_ppu_dots += 1;

        // Keep PPUMASK delayed enable/disable timing moving one dot per PPU tick.
        let had_pending = memory.ppu_mask.has_pending();
        memory.ppu_mask.tick();
        if had_pending {
            sprite_rendering = memory.ppu_mask.sprite_rendering();
            background_rendering = memory.ppu_mask.background_rendering();
        }

        let ei = self.event_index;
        let dot_x = (ei % WIDTH) as u16;
        let dot_scanline = (ei / WIDTH) as u16;

        // Keep mapper cycle-based hooks in sync with legacy PPU timing.
        if (ei % 3) == 0 {
            irq_requested |= memory.mapper.on_cpu_cycle();
        }
        let rendering_enabled_now = background_rendering || sprite_rendering;
        if rendering_enabled_now && dot_scanline < 240 && dot_x == 260 {
            irq_requested |= memory.mapper.on_scanline();
        }

        if dot_scanline == 0 && dot_x == 0 {
            result.frame_start = true;
        } else if dot_scanline == 240 && dot_x == 0 {
            result.frame_end = true;
        }

        self.x = dot_x;
        self.scanline = dot_scanline;

        *CURRENT_CYCLE.write().unwrap() = self.x;
        *CURRENT_SCANLINE.write().unwrap() = self.scanline;

        // Advance any pending sprite-0 hit before doing this dot's rendering work.
        // This keeps the delay count the same while making the flag visible at the
        // start of the dot that expires, which is the phase the CPU samples against.
        if self.sprite_0_hit_delay > 0 {
            self.sprite_0_hit_delay -= 1;
            // info!("SPRITE 0 DELAY NOW {}", self.sprite_0_hit_delay);
        }

        if self.sprite_0_hit_delay == 0 {
            // info!("SPRITE 0 DELAY AT 0, SETTING SPRITE 0 HIT FLAG");
            memory.set_bit(0x2002, BIT_SPRITE_0_HIT);
            self.sprite_0_hit_delay = -1;
        }

        if self.sprite_overflow_delay > 0 {
            self.sprite_overflow_delay -= 1;
        }

        if self.sprite_overflow_delay == 0 {
            memory.set_bit(0x2002, BIT_SPRITE_OVERFLOW);
            self.sprite_overflow_delay = -1;
        }

        // OAM corruption is "seeded" when rendering is disabled mid-render, but it only
        // happens on the first rendering dot (pre-render line..239) *after* rendering has
        // actually been turned off and then back on. Since $2001 disables are delayed by a
        // few dots, rendering can still be on right after the write: don't fire then.
        if self.pending_oam_corruption_row.is_some() && !rendering_enabled_now {
            self.oam_corruption_armed = true;
        }
        if rendering_enabled_now && self.oam_corruption_armed && (dot_scanline < 240 || dot_scanline == 261) {
            self.oam_corruption_armed = false;
            if let Some(row) = self.pending_oam_corruption_row.take() {
                if row > 0 && ((row as usize) * 8 + 8) <= self.oam.len() {
                    let row_start = (row as usize) * 8;
                    self.oam.copy_within(0..8, row_start);
                }
            }
        }

        if self.open_bus_decay_cycles > 0 {
            self.open_bus_decay_cycles -= 1;
            if self.open_bus_decay_cycles == 0 {
                self.open_bus = 0;
            }
        }

        if dot_scanline == 261 && dot_x == 339 && self.is_odd_frame && rendering_enabled_now {
            self.event_index = 0;
            self.is_odd_frame = !self.is_odd_frame;
            self.x = 0;
            self.scanline = 0;
        } else {
            self.event_index += 1;
            if self.event_index >= self.events.len() {
                // End of frame
                self.event_index = 0;
                self.is_odd_frame = !self.is_odd_frame;
                self.x = (self.event_index % WIDTH) as u16;
                self.scanline = (self.event_index / WIDTH) as u16;
            }
        }
        if self.x == 0 {
            // End of scanline
            self.oam2_eval_index = 0;
            self.oam2_clear_index = 0;
            self.sprite_fetch_index = 0;
        }

        if rendering_enabled_now && (dot_scanline < 240 || dot_scanline == 261) && dot_x == 339 {
            self.sprite_counting = [true; 64];
        }

        if dot_scanline < crate::constants::HEIGHT as u16
            && dot_x < crate::constants::WIDTH as u16
        {
            // Emit pixel for visible area only
            let fine_x = memory.ir.x as u16;
            let bit_pos = 15u16.saturating_sub(fine_x);
            let bit0 = (self.pattern_shift_low  >> bit_pos) & 1;
            let bit1 = (self.pattern_shift_high >> bit_pos) & 1;
            let attr0 = (self.attr_shift_low  >> bit_pos) & 1;
            let attr1 = (self.attr_shift_high >> bit_pos) & 1;

            // BG pixel already computed from the shift registers.
            let bg_palette_index = (attr1 << 3) | (attr0 << 2) | (bit1 << 1) | bit0;
            // NES rule: color index 0 (lower 2 bits == 0) is always transparent and
            // maps to the universal background color at palette_table[0], regardless of
            // which palette group the attribute bits selected.
            let bg_pal_addr = if (bg_palette_index & 0x03) == 0 { 0 } else { bg_palette_index as usize & 0x1f };
            let bg_color = self.palette_table[bg_pal_addr];

            // Respect PPUMASK left-column clipping semantics.
            let bg_visible = background_rendering
                && !(dot_x < 8 && memory.ppu_mask.clip_background);
            let sprites_visible = sprite_rendering
                && !(dot_x < 8 && memory.ppu_mask.clip_sprites);

            let bg_opaque = bg_visible && (bg_palette_index & 0x03) != 0;
            let mut final_color = if bg_visible {
                bg_color
            } else {
                // Universal background color when BG is disabled/clipped.
                self.palette_table[0]
            };
            let mut sprite_0_hit = false;
            let mut found_opaque_sprite = false;

            // Clock all 8 sprite X-counters and shift registers in parallel.
            for (i, sprite) in self.sprite_latches.iter_mut().enumerate() {
                let counting = self.sprite_counting.get(i).copied().unwrap_or(false);
                if counting {
                    if sprite.x == 0 {
                        if let Some(c) = self.sprite_counting.get_mut(i) {
                            *c = false;
                        }
                    } else {
                        sprite.x -= 1;
                        continue;
                    }
                }

                if rendering_enabled_now {
                    let h_flip = (sprite.attr & 0x40) != 0;
                    let (s0, s1) = if h_flip {
                        let s0 = sprite.low & 1;
                        let s1 = sprite.high & 1;
                        sprite.low >>= 1;
                        sprite.high >>= 1;
                        (s0, s1)
                    } else {
                        let s0 = (sprite.low >> 7) & 1;
                        let s1 = (sprite.high >> 7) & 1;
                        sprite.low <<= 1;
                        sprite.high <<= 1;
                        (s0, s1)
                    };
                    let sprite_color = (s1 << 1) | s0;

                    if sprite_color != 0 && !found_opaque_sprite {
                        found_opaque_sprite = true;
                        if sprites_visible {
                            // Sprite 0 hit: both bg and sprite non-transparent, not at x=255,
                            // and not suppressed by left-column clipping.
                            if sprite.is_sprite_0
                                && bg_opaque
                                && background_rendering
                                && dot_x < 255
                                && !(dot_x < 8 && (memory.ppu_mask.clip_sprites
                                                   || memory.ppu_mask.clip_background))
                            {
                                sprite_0_hit = true;
                            }

                            let behind_bg = (sprite.attr & 0x20) != 0;
                            if !behind_bg || !bg_opaque {
                                let sprite_palette = (sprite.attr & 0x03) as usize;
                                let pal_idx = 0x10 + (sprite_palette << 2) + sprite_color as usize;
                                final_color = self.palette_table[pal_idx & 0x1f];
                            }
                        }
                    }
                }
            }

            if sprite_0_hit && self.sprite_0_hit_delay == -1 {
                self.sprite_0_hit_delay = 2;
            }


            let index = dot_scanline as usize * crate::constants::WIDTH + dot_x as usize;
            let _ = self.delayed_pixels.push((index, final_color));
            if let Some((index, color)) = self.delayed_pixels.pop() {
                self.screen[index] = color;
            }
        }

        // Background shifters run whenever rendering is enabled (either BG or sprites per
        // AccuracyCoin "Rendering Flag Behavior") during visible fetch/render dots and tile prefetch dots.
        let bg_shift_active = rendering_enabled_now
            && (dot_scanline < 240 || dot_scanline == 261)
            && ((1..=256).contains(&dot_x) || (321..337).contains(&dot_x));
        if bg_shift_active {
            self.attr_shift_low <<= 1;
            self.attr_shift_high <<= 1;
            self.pattern_shift_low <<= 1;
            self.pattern_shift_high = (self.pattern_shift_high << 1) | 1;
        }

        // Only update scrolling v/t state while rendering is enabled on
        // visible or pre-render scanlines.
        let rendering_enabled = rendering_enabled_now;
        let can_update_scroll_v = rendering_enabled && (dot_scanline < 240 || dot_scanline == 261);

        // Process event
        if (event & NT) != 0 {
            let nt_addr = 0x2000 | (memory.ir.v as usize & 0x0FFF);
            self.tile_index = self.get_vram(nt_addr, &mut memory.mapper);
        }
        if (event & AT) != 0 {
            let v = memory.ir.v as usize;
            let address = 0x23C0
                | (v & 0x0C00)
                | ((v >> 4) & 0x38)
                | ((v >> 2) & 0x07);
            let attribute_data = self.get_vram(address, &mut memory.mapper);
            let shift = ((v >> 4) & 4) | (v & 2);
            self.palette = (attribute_data >> shift) & 3;
        }
        if (event & BG_LSBITS) != 0 {
            // 0 or $1000
            let base = memory.ppu_ctrl.background_table as usize;
            let tile_addr = base + (self.tile_index as usize * 16);
            let fine_y = (memory.ir.v as usize >> 12) & 7;
            self.pt_lsb = self.get_vram(tile_addr + fine_y,     &mut memory.mapper);
            self.pt_msb = self.get_vram(tile_addr + fine_y + 8, &mut memory.mapper);
        }
        if (event & BG_MSBITS) != 0 {
            if rendering_enabled_now {
                self.fetched_bg_tile = true;
            }
        }
        // Only update scrolling v/t state while rendering is enabled on
        if can_update_scroll_v {
            if (event & INC_HORIZ_V) != 0 {
                if self.fetched_bg_tile {
                    self.fetched_bg_tile = false;
                    self.pattern_shift_low  = (self.pattern_shift_low  & 0xFE00) | ((self.pt_lsb as u16) << 1);
                    self.pattern_shift_high = (self.pattern_shift_high & 0xFE00) | ((self.pt_msb as u16) << 1) | 1;

                    self.attr_shift_low  = (self.attr_shift_low  & 0xFF00)
                        | if (self.palette & 1) != 0 { 0x00FF } else { 0x0000 };
                    self.attr_shift_high = (self.attr_shift_high & 0xFF00)
                        | if (self.palette & 2) != 0 { 0x00FF } else { 0x0000 };
                }

                // NES coarse-X increment semantics (not raw v+1): wrap 31->0 and switch H nametable.
                memory.ir.increment_horiz_v();
            }
            if (event & INC_VERT_V) != 0 {
                memory.ir.increment_vert_v();
            }
            if (event & HORI_V_EQUALS_HORI_T) != 0 {
                self.old_v = memory.ir.v;
                memory.ir.hori_v_equals_hori_t()
            }
            if (event & VERT_V_EQUALS_VERT_T) != 0 {
                memory.ir.vert_v_equals_vert_t();
            }
        }
        //
        // Flags
        //
        if (event & SET_VBLANK_FLAG) != 0 {
            if self.vblank_suppressed {
                self.vblank_suppressed = false;
            } else {
                memory.set_bit(0x2002, BIT_VBL);
                if !self.nmi_suppressed {
                    result.vbl = true;
                }
            }
            self.nmi_suppressed = false;
            // End of frame, copy the newly generated frame to the frame buffer
            if self.last_screen_sent.elapsed().as_millis() > 16 {
                *FRAME.write().unwrap() =
                    <[u8; 61440]>::try_from(self.screen.clone()).unwrap();
                self.last_screen_sent = Instant::now();
            }
        }
        if (event & CLEAR_FLAGS) != 0 {
            self.sprite_overflow_delay = -1;
            self.vbl_cleared_at_dot1 = (memory.get_direct(0x2002) & (1 << BIT_VBL)) != 0;
            memory.clear_bit(0x2002, BIT_VBL);
            memory.clear_bit(0x2002, BIT_SPRITE_0_HIT);
            memory.clear_bit(0x2002, BIT_SPRITE_OVERFLOW);
        } else if dot_x > 2 {
            self.vbl_cleared_at_dot1 = false;
        }
        //
        // Sprites
        //
        if rendering_enabled && (self.scanline < 240 || self.scanline == 261) && self.x == 65 {
            self.oam2_has_sprite_zero = false;
        }
        if rendering_enabled && self.scanline < 240 && (self.x == 0 || self.x >= 321) {
            self.oam_buffer = self.oam2[0];
        }
        if rendering_enabled && self.scanline < 240 && (event & SPRITE_EVALUATION) != 0 {
            if self.x == 65 {
                self.oam2_eval_index = 0;
                self.eval_byte_index = 0;
                self.eval_finished = false;
                self.eval_overflow_count = 0;

            }
            if (self.x & 1) == 1 {
                let mut s = self.oam[self.oam_address as usize];
                if (self.oam_address & 0x03) == 2 {
                    s &= 0xE3;
                }
                self.oam_buffer = s;
            } else {
                let s = self.oam_buffer;
                let next_scanline = self.scanline + 1;
                let sprite_height = memory.ppu_ctrl.sprite_height as u16;
                let in_range = {
                    let sprite_y = s as u16 + 1;
                    sprite_y <= next_scanline && next_scanline < sprite_y + sprite_height
                };
                let n = (self.oam_address >> 2) & 0x3F;
                let m = self.oam_address & 0x03;

                if !self.eval_finished && self.oam2_eval_index < 32 {
                    self.oam2[self.oam2_eval_index] = s;
                    match self.eval_byte_index {
                        0 => {
                            if in_range {
                                if self.x == 66 {
                                    self.oam2_has_sprite_zero = true;
                                }
                                self.oam2_source_index[self.oam2_eval_index / 4] = self.oam_address;
                                self.oam2_eval_index += 1;
                                self.oam_address = self.oam_address.wrapping_add(1);
                                self.eval_byte_index = 1;
                            } else {
                                let next_n = (n + 1) & 0x3F;
                                self.oam_address = (self.oam_address.wrapping_add(4)) & 0xFC;
                                if next_n == 0 {
                                    self.eval_finished = true;
                                }
                                self.eval_byte_index = 0;
                            }
                        }
                        1 | 2 => {
                            self.oam2_eval_index += 1;
                            self.oam_address = self.oam_address.wrapping_add(1);
                            self.eval_byte_index += 1;
                        }
                        3 => {
                            self.oam2_eval_index += 1;
                            self.eval_byte_index = 0;
                            self.oam_address = self.oam_address.wrapping_add(1);
                            let next_n = (n + 1) & 0x3F;
                            if next_n == 0 {
                                self.eval_finished = true;
                            }
                        }
                        _ => unreachable!(),
                    }
                } else if self.eval_finished {
                    self.oam_buffer = self.oam2[self.oam2_eval_index & 0x1F];
                    self.oam_address = (self.oam_address.wrapping_add(4)) & 0xFC;
                } else {
                    // Secondary OAM is full (32 bytes / 8 sprites)
                    self.oam_buffer = self.oam2[0];
                    if self.eval_overflow_count > 0 {
                        self.eval_overflow_count -= 1;
                        if self.eval_overflow_count > 0 {
                            self.oam_address = self.oam_address.wrapping_add(1);
                        } else {
                            if (self.oam_address & 0x03) == 3 {
                                self.oam_address = self.oam_address.wrapping_add(1);
                            }
                            self.oam_address &= 0xFC;
                            self.eval_finished = true;
                        }
                    } else if in_range {
                        memory.set_bit(0x2002, BIT_SPRITE_OVERFLOW);
                        self.sprite_overflow_delay = -1;
                        self.eval_overflow_count = 3;
                        self.oam_address = self.oam_address.wrapping_add(1);
                    } else {
                        let next_n = (n + 1) & 0x3F;
                        let next_m = (m + 1) & 0x03;
                        self.oam_address = (next_n << 2) | next_m;
                        if next_n == 0 {
                            self.eval_finished = true;
                        }
                    }
                }
            }
        }
        if (event & END_SPRITE_EVALUATION) != 0 {
            self.sprite_fetch_index = 0;
            if rendering_enabled && (self.scanline < 240 || self.scanline == 261) {
                self.render_has_sprite_zero = self.oam2_has_sprite_zero;
            }
        }
        if rendering_enabled && (self.scanline < 240 || self.scanline == 261) && self.x >= 257 && self.x <= 320 {
            self.oam_address = 0;
            if self.scanline < 240 {
                let slot = ((self.x - 257) / 8) as usize;
                let sub = ((self.x - 257) % 8) as usize;
                self.oam_buffer = self.oam2[slot * 4 + sub.min(3)];
            }
        }
        if rendering_enabled && (self.scanline < 240 || self.scanline == 261) && (event & SPRITE_LS_BITS) != 0 {
            let slot = ((self.x - 261) / 8) as usize;
            if slot < 8 {
                if slot == 0 {
                    self.sprite_latches.truncate(8);
                }
                if self.sprite_latches.len() < 8 {
                    self.sprite_latches.resize(8, SpriteLatch {
                        source_index: 0xFF,
                        is_sprite_0: false,
                        attr: 0xFF,
                        x: 0xFF,
                        low: 0,
                        high: 0,
                    });
                }
                let off = slot * 4;
                let source_index = self.oam2_source_index[slot];
                let is_sprite_0 = slot == 0 && self.render_has_sprite_zero;
                let raw_y = self.oam2[off] as u16;
                let tile_index = self.oam2[off + 1] as u16;
                let attr = self.oam2[off + 2];
                let sprite_x = self.oam2[off + 3] as u16;

                let eval_line = self.scanline & 0xFF;
                let sprite_height = memory.ppu_ctrl.sprite_height as u16;
                let (low, high) = if raw_y <= eval_line && eval_line < raw_y + sprite_height {
                    let mut y_in_sprite = eval_line - raw_y;
                    let v_flip = (attr & 0x80) != 0;

                    let (pattern_base, effective_tile, fine_y) = if sprite_height == 16 {
                        let base_tile = tile_index & 0xFE;
                        let mut y_in_tile = y_in_sprite;
                        let tile = if y_in_tile < 8 {
                            if v_flip { base_tile + 1 } else { base_tile }
                        } else {
                            y_in_tile -= 8;
                            if v_flip { base_tile } else { base_tile + 1 }
                        };
                        if v_flip {
                            y_in_tile = 7 - y_in_tile;
                        }
                        let table = if (tile_index & 1) == 0 { 0usize } else { 0x1000usize };
                        (table, tile, y_in_tile as usize)
                    } else {
                        if v_flip {
                            y_in_sprite = 7 - y_in_sprite;
                        }
                        (memory.ppu_ctrl.sprite_table as usize, tile_index, y_in_sprite as usize)
                    };

                    let tile_offset = pattern_base + effective_tile as usize * 16 + fine_y;
                    (
                        self.get_vram(tile_offset, &mut memory.mapper),
                        self.get_vram(tile_offset + 8, &mut memory.mapper),
                    )
                } else {
                    (0, 0)
                };

                self.sprite_latches[slot] = SpriteLatch {
                    source_index,
                    is_sprite_0,
                    attr,
                    x: sprite_x,
                    low,
                    high,
                };
                self.sprite_counting[slot] = false;
                if slot == 7 {
                    self.sprite_latches.truncate(8);
                    if self.scanline < 240 && self.oam2_eval_index == 32 {
                        let start_n = (self.oam2_source_index[7] as usize / 4) + 1;
                        for n in start_n..64 {
                            let oam_off = n * 4;
                            let extra_y = self.oam[oam_off] as u16;
                            if extra_y <= eval_line && eval_line < extra_y + sprite_height {
                                let extra_tile = self.oam[oam_off + 1] as u16;
                                let extra_attr = self.oam[oam_off + 2] & 0xE3;
                                let extra_x = self.oam[oam_off + 3] as u16;
                                let mut y_in_sprite = eval_line - extra_y;
                                let v_flip = (extra_attr & 0x80) != 0;
                                let (pattern_base, effective_tile, fine_y) = if sprite_height == 16 {
                                    let base_tile = extra_tile & 0xFE;
                                    let mut y_in_tile = y_in_sprite;
                                    let tile = if y_in_tile < 8 {
                                        if v_flip { base_tile + 1 } else { base_tile }
                                    } else {
                                        y_in_tile -= 8;
                                        if v_flip { base_tile } else { base_tile + 1 }
                                    };
                                    if v_flip {
                                        y_in_tile = 7 - y_in_tile;
                                    }
                                    let table = if (extra_tile & 1) == 0 { 0usize } else { 0x1000usize };
                                    (table, tile, y_in_tile as usize)
                                } else {
                                    if v_flip {
                                        y_in_sprite = 7 - y_in_sprite;
                                    }
                                    (memory.ppu_ctrl.sprite_table as usize, extra_tile, y_in_sprite as usize)
                                };
                                let tile_offset = pattern_base + effective_tile as usize * 16 + fine_y;
                                let extra_low = self.get_vram(tile_offset, &mut memory.mapper);
                                let extra_high = self.get_vram(tile_offset + 8, &mut memory.mapper);
                                let idx = self.sprite_latches.len();
                                if idx < 64 {
                                    self.sprite_latches.push(SpriteLatch {
                                        source_index: oam_off as u8,
                                        is_sprite_0: false,
                                        attr: extra_attr,
                                        x: extra_x,
                                        low: extra_low,
                                        high: extra_high,
                                    });
                                    self.sprite_counting[idx] = false;
                                }
                            }
                        }
                    }
                }

                self.sprite_fetch_index = slot + 1;
            }
        }
        if (event & SPRITE_MS_BITS) != 0 {}
        if self.pending_2007_read_delay > 0 {
            self.pending_2007_read_delay -= 1;
            if self.pending_2007_read_delay == 0 {
                let is_rendering = can_update_scroll_v;
                if is_rendering {
                    let fetch_byte = match dot_x {
                        1..=256 => {
                            let phase = (dot_x - 1) & 7;
                            match phase {
                                0 | 1 => self.tile_index,
                                2 | 3 => {
                                    let v = memory.ir.v as usize;
                                    let address = 0x23C0
                                        | (v & 0x0C00)
                                        | ((v >> 4) & 0x38)
                                        | ((v >> 2) & 0x07);
                                    self.get_vram(address, &mut memory.mapper)
                                }
                                4 | 5 => self.pt_lsb,
                                6 | 7 => self.pt_msb,
                                _ => unreachable!(),
                            }
                        }
                        257..=320 => {
                            let slot = ((dot_x - 257) / 8) as usize;
                            let sub = (dot_x - 257) & 7;
                            match sub {
                                0 | 1 => {
                                    if dot_x <= 258 {
                                        let nt_addr = 0x2000 | (self.old_v as usize & 0x0FFF);
                                        self.get_vram(nt_addr, &mut memory.mapper)
                                    } else {
                                        let nt_addr = 0x2000 | (memory.ir.v as usize & 0x0FFF);
                                        self.get_vram(nt_addr, &mut memory.mapper)
                                    }
                                }
                                2 | 3 => {
                                    let nt_addr = 0x2000 | (memory.ir.v as usize & 0x0FFF);
                                    self.get_vram(nt_addr, &mut memory.mapper)
                                }
                                4..=7 => {
                                    let off = slot * 4;
                                    let raw_y = self.oam2[off] as u16;
                                    let tile_index = self.oam2[off + 1] as u16;
                                    let attr = self.oam2[off + 2];
                                    let eval_line = dot_scanline & 0xFF;
                                    let sprite_height = memory.ppu_ctrl.sprite_height as u16;
                                    let mut y_in_sprite = eval_line.wrapping_sub(raw_y) & 0x0F;
                                    let v_flip = (attr & 0x80) != 0;
                                    let (pattern_base, effective_tile, fine_y) = if sprite_height == 16 {
                                        let base_tile = tile_index & 0xFE;
                                        let mut y_in_tile = y_in_sprite;
                                        let tile = if y_in_tile < 8 {
                                            if v_flip { base_tile + 1 } else { base_tile }
                                        } else {
                                            y_in_tile -= 8;
                                            if v_flip { base_tile } else { base_tile + 1 }
                                        };
                                        if v_flip {
                                            y_in_tile = 7 - y_in_tile;
                                        }
                                        let table = if (tile_index & 1) == 0 { 0usize } else { 0x1000usize };
                                        (table, tile, y_in_tile as usize)
                                    } else {
                                        y_in_sprite &= 0x07;
                                        if v_flip {
                                            y_in_sprite = 7 - y_in_sprite;
                                        }
                                        (memory.ppu_ctrl.sprite_table as usize, tile_index, y_in_sprite as usize)
                                    };
                                    let tile_offset = pattern_base + effective_tile as usize * 16 + fine_y;
                                    if sub <= 5 {
                                        self.get_vram(tile_offset, &mut memory.mapper)
                                    } else {
                                        self.get_vram(tile_offset + 8, &mut memory.mapper)
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        321..=336 => {
                            let sub = (dot_x - 321) & 7;
                            match sub {
                                0 | 1 => {
                                    let nt_addr = 0x2000 | (memory.ir.v as usize & 0x0FFF);
                                    self.get_vram(nt_addr, &mut memory.mapper)
                                }
                                2 | 3 => {
                                    let v = memory.ir.v as usize;
                                    let address = 0x23C0
                                        | (v & 0x0C00)
                                        | ((v >> 4) & 0x38)
                                        | ((v >> 2) & 0x07);
                                    self.get_vram(address, &mut memory.mapper)
                                }
                                4 | 5 => {
                                    let nt_addr = 0x2000 | (memory.ir.v as usize & 0x0FFF);
                                    let tile = self.get_vram(nt_addr, &mut memory.mapper);
                                    let fine_y = (memory.ir.v as usize >> 12) & 7;
                                    let pattern_base = memory.ppu_ctrl.background_table as usize;
                                    self.get_vram(pattern_base + tile as usize * 16 + fine_y, &mut memory.mapper)
                                }
                                6 | 7 => {
                                    let nt_addr = 0x2000 | (memory.ir.v as usize & 0x0FFF);
                                    let tile = self.get_vram(nt_addr, &mut memory.mapper);
                                    let fine_y = (memory.ir.v as usize >> 12) & 7;
                                    let pattern_base = memory.ppu_ctrl.background_table as usize;
                                    self.get_vram(pattern_base + tile as usize * 16 + fine_y + 8, &mut memory.mapper)
                                }
                                _ => unreachable!(),
                            }
                        }
                        337..=340 | 0 => {
                            let nt_addr = 0x2000 | (memory.ir.v as usize & 0x0FFF);
                            self.get_vram(nt_addr, &mut memory.mapper)
                        }
                        _ => 0,
                    };
                    memory.internal_buffer = fetch_byte;
                    memory.ir.increment_horiz_v();
                    memory.ir.increment_vert_v();
                } else {
                    let a = memory.ir.v as usize & 0x3fff;
                    let is_palette = (a & 0x3f00) == 0x3f00;
                    let vram_addr = if is_palette { 0x2f00 + (a & 0xff) } else { a };
                    memory.internal_buffer = self.get_vram(vram_addr, &mut memory.mapper);
                    memory.ir.increment_v(memory.ppu_ctrl.vram_increment);
                }
            }
        }
        if rendering_enabled && self.scanline < 240 && (event & SECONDARY_OAM_CLEAR) != 0 {
            self.oam_buffer = 0xff;
            if self.oam2_clear_index < self.oam2.len() {
                self.oam2[self.oam2_clear_index] = 0xff;
                self.oam2_clear_index += 1;
            }
        }

        result.irq_requested = irq_requested;
        result
    }

    fn init_events() -> [u16; HEIGHT * WIDTH] {
        let mut result = [0; HEIGHT * WIDTH];
        for i in 0..HEIGHT * WIDTH {
            result[i] = 0;
        }

        // Row 0
        for x in (0..256).step_by(8) {
            result[x + 1] = NT;
            result[x + 3] = AT;
            result[x + 5] = BG_LSBITS;
            result[x + 7] = BG_MSBITS;
            result[x + 8] = INC_HORIZ_V;
        }

        result[256] |= INC_VERT_V;
        result[257] |= HORI_V_EQUALS_HORI_T;

        for x in (321..337).step_by(8) {
            result[x + 1] = NT;
            result[x + 3] = AT;
            result[x + 5] = BG_LSBITS;
            result[x + 7] = BG_MSBITS;
            result[x + 8] = INC_HORIZ_V;
        }

        // Copy row 0 to rows 1..239
        for y in 1..240 {
            for x in 0..WIDTH {
                result[y * WIDTH + x] = result[x];
            }
        }

        // Row 241 (just set VBL)
        result[241 * WIDTH + 1] = SET_VBLANK_FLAG;

        // Row 261 (pre render line)
        let y = 261;
        for x in (0..256).step_by(8) {
            let index = y * WIDTH + x;
            result[index + 1] = NT;
            result[index + 3] = AT;
            result[index + 5] = BG_LSBITS;
            result[index + 7] = BG_MSBITS;
            result[index + 8] = INC_HORIZ_V;
        }
        result[261 * WIDTH + 1] |= CLEAR_FLAGS;

        result[y * WIDTH + 256] |= INC_VERT_V;
        result[y * WIDTH + 257] |= HORI_V_EQUALS_HORI_T;

        for x in 280..305 {
            result[y * WIDTH + x] |= VERT_V_EQUALS_VERT_T;
        }

        // Pre-render line also prefetches tiles 33 and 34 (dots 321-336) to seed the shift
        // registers for scanline 0.  Without these events the prefetch_lsbN / prefetch_msbN
        // fields would still hold stale data from the previous frame's scanline-239 prefetch,
        // giving wrong tile data (red lines etc.) in tiles 0 and 1 of the very first visible row.
        for x in (321..337).step_by(8) {
            let index = y * WIDTH + x;
            result[index + 1] = NT;
            result[index + 3] = AT;
            result[index + 5] = BG_LSBITS;
            result[index + 7] = BG_MSBITS;
            result[index + 8] = INC_HORIZ_V;
        }

        //
        // Sprites
        //
        for y in 0..240 {
            for x in 1..65 {
                result[y * WIDTH + x] |= SECONDARY_OAM_CLEAR;
            }
            for x in 65..257 {
                result[y * WIDTH + x] |= SPRITE_EVALUATION;
            }
        }
        // Pre-render line also evaluates sprites for scanline 0.
        let y = 261;
        for x in 1..65 {
            result[y * WIDTH + x] |= SECONDARY_OAM_CLEAR;
        }
        for x in 65..257 {
            result[y * WIDTH + x] |= SPRITE_EVALUATION;
        }
        for y in 0..262 {
            result[y * WIDTH + 260] |= END_SPRITE_EVALUATION;
            // Hardware uses dots 257-320 (64 dots) to fetch up to 8 sprites (8 dots each).
            // step_by(8) gives exactly 8 fetch slots: 261, 269, 277, 285, 293, 301, 309, 317.
            // step_by(16) would only produce 4 slots and silently drop sprites 4-7.
            for x in (261..321).step_by(8) {
                result[y * WIDTH + x] |= SPRITE_LS_BITS;
                result[y * WIDTH + x + 2] |= SPRITE_MS_BITS;
            }
        }
        result
    }

    pub fn get_open_bus(&self) -> u8 {
        self.open_bus
    }

    pub fn set_open_bus(&mut self, value: u8) {
        self.open_bus = value;
        // PPU open bus decays to 0 after ~600ms (~1,000,000 PPU dots).
        // AccuracyCoin test 5 waits ~120 frames (2 seconds, ~10,700,000 PPU dots).
        self.open_bus_decay_cycles = 1_000_000;
    }

    pub(crate) fn get_oam_memory(&self) -> Vec<u8> {
        Vec::from(self.oam.clone())
    }

    pub(crate) fn get_video_memory(&self) -> Vec<u8> {
        let mut memory = Vec::with_capacity(VRAM_SIZE);
        let mut mapper = MapperBase::default();
        for i in 0..VRAM_SIZE {
            memory.push(self.get_vram(i, &mut mapper))
        }
        memory
    }

    pub fn get_vram(&self, address: usize, mapper: &mut MapperBase) -> u8 {
        let after = NesMemory::ppu_mirrorring(address as u16) as usize;
        if (0x2000..=0x2fff).contains(&after) {
            mapper.read_nametable(after)
        } else if after >= 0x3f00 {
            let mut idx = after & 0x1f;
            if idx >= 0x10 && (idx & 3) == 0 {
                idx -= 0x10;
            }
            self.palette_table[idx]
        } else {
            mapper.read_chr(after as u16)
        }
    }

    pub fn set_vram(&mut self, address: usize, value: u8, mapper: &mut MapperBase) {
        let after = NesMemory::ppu_mirrorring(address as u16) as usize;
        if (0x2000..=0x2fff).contains(&after) {
            mapper.write_nametable(after, value);
        } else if after >= 0x3f00 {
            let mut idx = after & 0x1f;
            if idx >= 0x10 && (idx & 3) == 0 {
                idx -= 0x10;
            }
            self.palette_table[idx] = value & 0x3f;
        } else {
            mapper.write_chr(after as u16, value);
        }
    }

    pub fn write_oam(&mut self, address: u8, value: u8) {
        self.oam[address as usize] = value;
    }

    pub fn update_beam(&mut self, rendering_enabled: bool) {
        let dot_x = (self.event_index % WIDTH) as u16;
        let dot_scanline = (self.event_index / WIDTH) as u16;

        if dot_scanline == 261 && dot_x == 339 && self.is_odd_frame && rendering_enabled {
            self.event_index = 0;
            self.is_odd_frame = !self.is_odd_frame;
        } else {
            self.event_index += 1;
            if self.event_index >= self.events.len() {
                self.event_index = 0;
                self.is_odd_frame = !self.is_odd_frame;
            }
        }

        self.x = (self.event_index % WIDTH) as u16;
        self.scanline = (self.event_index / WIDTH) as u16;

        if self.x == 0 {
            self.oam2_eval_index = 0;
            self.oam2_clear_index = 0;
            self.sprite_fetch_index = 0;
        }
    }
}


/// Each sprite stored in oam2 gets its information extracted into this structure
/// during the SPRITE_LS_BITS event.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SpriteLatch {
    /// Primary OAM sprite index (0..63) this latch originated from.
    source_index: u8,
    is_sprite_0: bool,
    attr: u8,
    /// Stores sprite X for pixel alignment (kept as `y` to minimize churn).
    x: u16,
    low: u8,
    high: u8,
}


#[cfg(test)]
#[path = "ppu2_test.rs"]
mod ppu2_test;
