use crate::emulator::FRAME;
use crate::mappers::mapper_base::MapperBase;
use crate::nes_memory::NesMemory;
use crate::ppu::{PpuResult, BIT_SPRITE_0_HIT, BIT_SPRITE_OVERFLOW, BIT_VBL, CURRENT_CYCLE, CURRENT_SCANLINE};
use std::time::Instant;
use tracing::{info};
use crate::delayed::Delayed;

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

    x: u16,
    pub scanline: u16,
    /// Current NT byte
    nt: u8,
    at: u16,

    // Where we store the sprites we detected will appear on the next scanline, max 32
    oam2: [u8; 32],
    /// Source primary OAM sprite index (0..63) for each oam2 entry (0..7).
    oam2_source_index: [u8; 8],
    /// Secondary OAM write index during sprite evaluation (0..32 step 4)
    oam2_eval_index: usize,
    /// Secondary OAM clear index during dots 1..64 (0..32 step 1)
    oam2_clear_index: usize,
    /// Sprite fetch cursor during dots 261..320 (0..sprite_count)
    sprite_fetch_index: usize,

    /// At most 8 fetched sprite rows for the next scanline.
    sprite_latches: Vec<SpriteLatch>,

    pub(crate) oam: [u8; 256],
    pub oam_address: u8,
    open_bus: u8,

    tile_index: u8,
    pt_lsb: u8,
    pt_msb: u8,
    palette: u8,
    /// NES colour values (0-63) stored at palette RAM indices 0-31
    palette_table: [u8; 32],
    pattern_shift_low: u16,
    pattern_shift_high: u16,
    attr_shift_low: u16,
    attr_shift_high: u16,
    /// Fine X scroll latched at dot 0 of each scanline. This ensures that
    /// mid-frame scroll changes via $2005 don't misalign the pre-fetched tile data.
    fine_x_latched: u8,
    last_screen_sent: Instant,
    sprite_0_hit_delay: i8,
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
            nt: 0,
            at: 0,
            oam2: [0; 32],
            oam2_source_index: [0; 8],
            oam2_eval_index: 0,
            oam2_clear_index: 0,
            sprite_fetch_index: 0,
            sprite_latches: Vec::new(),
            oam: [0; 256],
            oam_address: 0,
            open_bus: 0,
            tile_index: 0,
            pt_lsb: 0,
            pt_msb: 0,
            palette: 0,
            palette_table: [0; 32],
            pattern_shift_low: 0,
            pattern_shift_high: 0,
            attr_shift_low: 0,
            attr_shift_high: 0,
            fine_x_latched: 0,
            last_screen_sent: Instant::now(),
            sprite_0_hit_delay: -1,
        };

        for i in 0..crate::ppu::DEFAULT_SPRITE_PALETTE.len() {
            result.set_vram(0x3f10 + i, crate::ppu::DEFAULT_SPRITE_PALETTE[i], mapper);
        }

        result
    }

    pub fn tick(&mut self, sprite_rendering: bool, background_rendering: bool,
        memory: &mut NesMemory) -> PpuResult
    {
        let mut result = PpuResult::default();
        let event = self.events[self.event_index];

        // Keep PPUMASK delayed enable/disable timing moving one dot per PPU tick.
        memory.ppu_mask.tick();

        let ei = self.event_index;
        let dot_x = (ei % WIDTH) as u16;
        let dot_scanline = (ei / WIDTH) as u16;

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

        self.event_index += 1;
        if self.event_index >= self.events.len() {
            // End of frame
            self.event_index = 0;
            self.x = (self.event_index % WIDTH) as u16;
            self.scanline = (self.event_index / WIDTH) as u16;
        }
        if self.x == 0 {
            // End of scanline
            self.oam2_eval_index = 0;
            self.oam2_clear_index = 0;
            self.sprite_fetch_index = 0;
        }

        if dot_scanline < crate::constants::HEIGHT as u16
            && dot_x < crate::constants::WIDTH as u16
        {
            // At dot 0 of each visible scanline, latch fine_x so that any mid-frame
            // scroll changes via $2005 don't misalign the pre-fetched tile data.
            if dot_x == 0 {
                self.fine_x_latched = memory.ir.x;
            }

            // Emit pixel for visible area only
            let fine_x = self.fine_x_latched as u16;
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

            // OAM order priority: first non-transparent sprite pixel wins.
            if sprites_visible {
                for (_, sprite) in self.sprite_latches.iter().enumerate() {
                    let sprite_x = sprite.x;
                    let dx = dot_x as i16 - sprite_x as i16;
                    if dx < 0 || dx >= 8 {
                        continue;
                    }

                    let h_flip = (sprite.attr & 0x40) != 0;
                    let bit_index = if h_flip { dx as u8 } else { 7 - dx as u8 };

                    let s0 = (sprite.low >> bit_index) & 1;
                    let s1 = (sprite.high >> bit_index) & 1;
                    let sprite_color = (s1 << 1) | s0; // 0..3

                    // Sprite color index 0 is transparent.
                    if sprite_color == 0 {
                        continue;
                    }

                    // Sprite 0 hit: both bg and sprite non-transparent, not at x=255,
                    // and not suppressed by left-column clipping.
                    if sprite.source_index == 0
                        && bg_opaque
                        && background_rendering
                        && dot_scanline > 0
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

                    break;
                }
            }

            if sprite_0_hit && self.sprite_0_hit_delay == -1 {
                self.sprite_0_hit_delay = 3;
                // info!("SPRITE 0 DETECTED ON SCANLINE {}, SETTING DELAY TO {}", dot_scanline, self.sprite_0_hit_delay);
            }

            let index = dot_scanline as usize * crate::constants::WIDTH + dot_x as usize;
            let _ = self.delayed_pixels.push((index, final_color));
            if let Some((index, color)) = self.delayed_pixels.pop() {
                self.screen[index] = color;
            }
        }

        // Background shifters run only during visible fetch/render dots and tile prefetch dots.
        // Shifting on every dot over-shifts data loaded at the end of a scanline and causes a
        // left-edge black bar on the next scanline.
        let bg_shift_active = background_rendering
            && (dot_scanline < 240 || dot_scanline == 261)
            && ((1..=256).contains(&dot_x) || (321..337).contains(&dot_x));
        if bg_shift_active {
            self.attr_shift_low <<= 1;
            self.attr_shift_high <<= 1;
            self.pattern_shift_low <<= 1;
            self.pattern_shift_high <<= 1;
        }

        // Only update scrolling v/t state while rendering is enabled on
        // visible or pre-render scanlines.
        let rendering_enabled = background_rendering || sprite_rendering;
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
            // Already calculated at BG_LSBITS
        }
        // Only update scrolling v/t state while rendering is enabled on
        if can_update_scroll_v {
            if (event & INC_HORIZ_V) != 0 {
                // Visible-scanline INC: load new tile data into the shift registers.
                // At the moment INC fires (after emit+shift for this dot), bit 8 of the
                // register is 0 (the previous tile's bit 7 was just emitted and shifted out
                // of the u16). Placing the new byte at bits 8-1 (via << 1) fills that gap,
                // so after exactly 7 more shifts the new byte is cleanly in bits 15-8 and
                // will be output MSB-first starting at the correct column.
                // Clear low 9 bits (8..0) before inserting bits 8..1 from the next tile.
                // Keeping bit 8 stale creates a 1-pixel seam at tile boundaries.
                self.pattern_shift_low  = (self.pattern_shift_low  & 0xFE00) | ((self.pt_lsb as u16) << 1);
                self.pattern_shift_high = (self.pattern_shift_high & 0xFE00) | ((self.pt_msb as u16) << 1);

                // Attribute latch expands to 8 replicated bits for the upcoming tile.
                self.attr_shift_low  = (self.attr_shift_low  & 0xFF00)
                    | if (self.palette & 1) != 0 { 0x00FF } else { 0x0000 };
                self.attr_shift_high = (self.attr_shift_high & 0xFF00)
                    | if (self.palette & 2) != 0 { 0x00FF } else { 0x0000 };

                // NES coarse-X increment semantics (not raw v+1): wrap 31->0 and switch H nametable.
                if memory.ir.coarse_x() == 31 {
                    memory.ir.set_coarse_x(0);
                    memory.ir.switch_horizontal_nametable();
                } else {
                    memory.ir.increment_coarse_x();
                }
            }
            if (event & INC_VERT_V) != 0 {
                memory.ir.increment_vert_v();
            }
            if (event & HORI_V_EQUALS_HORI_T) != 0 {
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
            memory.set_bit(0x2002, BIT_VBL);
            result.vbl = true;
            // End of frame, copy the newly generated frame to the frame buffer
            if self.last_screen_sent.elapsed().as_millis() > 16 {
                *FRAME.write().unwrap() =
                    <[u8; 61440]>::try_from(self.screen.clone()).unwrap();
                self.last_screen_sent = Instant::now();
            }
        }
        if (event & CLEAR_FLAGS) != 0 {
            memory.clear_bit(0x2002, BIT_VBL);
            memory.clear_bit(0x2002, BIT_SPRITE_0_HIT);
            memory.clear_bit(0x2002, BIT_SPRITE_OVERFLOW);
        }
        //
        // Sprites
        //
        if rendering_enabled && (event & SPRITE_EVALUATION) != 0 {
            // Sprite evaluation runs from dot 65..256 (inclusive).
            // One OAM entry is evaluated per pair of dots; dot 65 → OAM[0], dot 66 → OAM[1], …
            let sprite_count = self.oam2_eval_index / 4;
            let sprite_evaluated = (self.x as usize).wrapping_sub(65);
            if sprite_evaluated < 64 {
                let oam_index = sprite_evaluated * 4;
                let sprite_height = memory.ppu_ctrl.sprite_height as u16;
                // OAM stores Y-1; on-screen top Y is OAM Y + 1.
                let sprite_y = self.oam[oam_index] as u16 + 1;
                // Pre-render scanline (261) evaluates sprites for scanline 0.
                let next_scanline = if self.scanline == 261 { 0u16 } else { self.scanline + 1 };
                if sprite_y <= next_scanline
                    && next_scanline < sprite_y + sprite_height
                    && next_scanline < 240
                {
                    if sprite_count == 8 {
                        info!(target: "ppu", "Sprite overflow");
                        memory.set_bit(0x2002, BIT_SPRITE_OVERFLOW);
                    } else {
                        // This sprite is visible on the next scanline, copy it to
                        // secondary OAM
                        self.oam2_source_index[sprite_count] = sprite_evaluated as u8;
                        self.oam2[self.oam2_eval_index] = self.oam[oam_index];
                        self.oam2[self.oam2_eval_index + 1] = self.oam[oam_index + 1];
                        self.oam2[self.oam2_eval_index + 2] = self.oam[oam_index + 2];
                        self.oam2[self.oam2_eval_index + 3] = self.oam[oam_index + 3];
                        self.oam2_eval_index += 4;
                    }
                }
            }
        }
        if (event & END_SPRITE_EVALUATION) != 0 {
            // End of sprite evaluation, sprite data for the next scanline is now available in oam2
            // and can be used for rendering/fetching. Clear previous scanline latches now,
            // then refill during sprite fetch events (261..320).
            self.sprite_latches.clear();
            self.sprite_fetch_index = 0;
        }
        if rendering_enabled && (event & SPRITE_LS_BITS) != 0 {
             let sprite_count = self.oam2_eval_index / 4;
             if self.sprite_fetch_index < sprite_count {
                 let off = self.sprite_fetch_index * 4;
                 // OAM stores Y-1; on-screen top Y is OAM Y + 1.
                 let source_index = self.oam2_source_index[self.sprite_fetch_index];
                 let sprite_y = self.oam2[off] as u16 + 1;
                 let tile_index = self.oam2[off + 1] as u16;
                 let attr = self.oam2[off + 2];
                 let sprite_x = self.oam2[off + 3] as u16;

                 // On the pre-render scanline (261) we're fetching tiles for scanline 0,
                 // not scanline 262. Guard against that wrap explicitly.
                 let next_scanline = if self.scanline == 261 { 0u16 } else { self.scanline + 1 };
                 let sprite_height = memory.ppu_ctrl.sprite_height as u16;
                 if sprite_y <= next_scanline && next_scanline < sprite_y + sprite_height && next_scanline < 240 {
                     let mut y_in_sprite = next_scanline - sprite_y;
                     let v_flip = (attr & 0x80) != 0;

                     let (pattern_base, effective_tile, fine_y) = if sprite_height == 16 {
                         // 8x16 mode: bit 0 of tile index selects pattern table; top/bottom tile
                         // is selected by y_in_sprite, with vertical flip swapping halves.
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
                     self.sprite_latches.push(SpriteLatch {
                         source_index,
                         attr,
                         x: sprite_x,
                         low: self.get_vram(tile_offset, &mut memory.mapper),
                         high: self.get_vram(tile_offset + 8, &mut memory.mapper),
                     });
                 }

                 self.sprite_fetch_index += 1;
             }
         }
        if (event & SPRITE_MS_BITS) != 0 {}
        if (event & SECONDARY_OAM_CLEAR) != 0 {
            if self.oam2_clear_index < self.oam2.len() {
                self.oam2[self.oam2_clear_index] = 0xff;
                self.oam2_clear_index += 1;
            }
        }

        result
    }

    fn init_events() -> [u16; HEIGHT * WIDTH] {
        let mut result = [0; HEIGHT * WIDTH];
        for i in 0..HEIGHT * WIDTH {
            result[i] = 0;
        }

        // First row
        for x in (0..256).step_by(8) {
            let index = x;
            result[index + 1] = NT;
            result[index + 3] = AT;
            result[index + 5] = BG_LSBITS;
            result[index + 7] = BG_MSBITS;
            result[index + 8] = INC_HORIZ_V;
        }

        for x in (321..337).step_by(8) {
            let index = x;
            result[index + 1] = NT;
            result[index + 3] = AT;
            result[index + 5] = BG_LSBITS;
            result[index + 7] = BG_MSBITS;
            result[index + 8] = INC_HORIZ_V;
        }

        result[256] |= INC_VERT_V;
        result[257] |= HORI_V_EQUALS_HORI_T;

        // Copy row 0 to row 1..239
        for y in 1..240 {
            for x in 0..WIDTH {
                result[y * WIDTH + x] = result[x];
            }
        }

        // Row 241 (just set VBL)
        // Fire VBL at dot 0 (one dot before hardware's dot 1) to match ppu1 timing.
        // 241*341 = 82181, and 82181 % 3 == 2 (last PPU tick of a CPU-cycle batch),
        // so the NMI reaches the CPU one cycle earlier than dot 1 — necessary for
        // Branch Basics timing test to pass (same fix as ppu1).
        result[241 * WIDTH + 0] = SET_VBLANK_FLAG;

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
            result[y * WIDTH + 260] = END_SPRITE_EVALUATION;
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
    }

    pub fn get_vram(&self, address: usize, mapper: &mut MapperBase) -> u8 {
        let after = NesMemory::ppu_mirrorring(address as u16) as usize;
        if (0x2000..=0x2fff).contains(&after) {
            mapper.read_nametable(after)
        } else if address >= 0x3f00 {
            self.palette_table[after & 0x1f]
        } else {
            mapper.read_chr(after as u16)
        }
    }

    pub fn set_vram(&mut self, address: usize, value: u8, mapper: &mut MapperBase) {
        let after = NesMemory::ppu_mirrorring(address as u16) as usize;
        if (0x2000..=0x2fff).contains(&after) {
            mapper.write_nametable(after, value);
        } else if address >= 0x3f00 {
            self.palette_table[after & 0x1f] = value & 0x3f;
        } else {
            mapper.write_chr(after as u16, value);
        }
    }

    pub fn write_oam(&mut self, address: u8, value: u8) {
        self.oam[address as usize] = value;
    }

    pub fn update_beam(&mut self, _rendering_enabled: bool) {
        self.event_index += 1;
        if self.event_index >= self.events.len() {
            self.event_index = 0;
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
struct SpriteLatch {
    /// Primary OAM sprite index (0..63) this latch originated from.
    source_index: u8,
    attr: u8,
    /// Stores sprite X for pixel alignment (kept as `y` to minimize churn).
    x: u16,
    low: u8,
    high: u8,
}


#[cfg(test)]
#[path = "ppu2_test.rs"]
mod ppu2_test;
