use crate::rom::Rom;
use super::*;

#[test]
fn test_ppu2_events() {
    let ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    for y in 0..240 {
        let index = y * WIDTH;
        assert_ne!(ppu.events[index + 1] & NT, 0);
        assert_ne!(ppu.events[index + 3] & AT, 0);
        assert_ne!(ppu.events[index + 5] & BG_LSBITS, 0);
        assert_ne!(ppu.events[index + 7] & BG_MSBITS, 0);
        assert_ne!(ppu.events[index + 8] & INC_HORIZ_V, 0);
    }
    assert_eq!(ppu.events[241 * WIDTH + 0], SET_VBLANK_FLAG);

    let y = 261;
    let index = y * WIDTH;
    assert_ne!(ppu.events[index + 1] & NT, 0);
    assert_ne!(ppu.events[index + 1] & CLEAR_FLAGS, 0);
    assert_ne!(ppu.events[index + 1] & SECONDARY_OAM_CLEAR, 0);
    assert_ne!(ppu.events[index + 3] & AT, 0);
    assert_ne!(ppu.events[index + 5] & BG_LSBITS, 0);
    assert_ne!(ppu.events[index + 7] & BG_MSBITS, 0);
    assert_ne!(ppu.events[index + 8] & INC_HORIZ_V, 0);

    for x in 280..305 {
        assert_ne!(ppu.events[261 * WIDTH + x] & VERT_V_EQUALS_VERT_T, 0);
    }

    // Test sprites
    for y in 0..240 {
        for x in 1..65 {
            assert_ne!(ppu.events[y * WIDTH + x] & SECONDARY_OAM_CLEAR, 0);
        }
        for x in 65..257 {
            assert_ne!(ppu.events[y * WIDTH + x] & SPRITE_EVALUATION, 0);
        }
        // 8 sprite fetch slots at step_by(8): 261, 269, 277, 285, 293, 301, 309, 317
        for x in (261..321).step_by(8) {
            assert_ne!(ppu.events[y * WIDTH + x] & SPRITE_LS_BITS, 0);
            assert_ne!(ppu.events[y * WIDTH + x + 2] & SPRITE_MS_BITS, 0);
        }
    }
    let y = 261;
    for x in (261..321).step_by(8) {
        assert_ne!(ppu.events[y * WIDTH + x] & SPRITE_LS_BITS, 0);
        assert_ne!(ppu.events[y * WIDTH + x + 2] & SPRITE_MS_BITS, 0);
    }
}

#[test]
fn test_ppu2_scroll_events_do_not_advance_v_when_rendering_disabled() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    // Coarse X = 31 so INC_HORIZ_V would wrap/toggle nametable if it fires.
    memory.ir.v = 0x001F;
    let v_before = memory.ir.v;

    // Dot 8 on scanline 0 carries INC_HORIZ_V in the event table.
    for _ in 0..9 {
        ppu.tick(false, false, &mut memory);
    }

    assert_eq!(memory.ir.v, v_before);
}

#[test]
fn test_ppu2_scroll_events_advance_v_when_rendering_enabled() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x18);

    // Coarse X = 31; first INC_HORIZ_V should wrap coarse X to 0 and toggle horizontal NT.
    memory.ir.v = 0x001F;

    // Dot 8 on scanline 0 carries INC_HORIZ_V.
    for _ in 0..9 {
        ppu.tick(true, false, &mut memory);
    }

    assert_eq!(memory.ir.coarse_x(), 0);
    assert_eq!(memory.ir.horizontal_nametable(), 1);
    assert_eq!(memory.ir.v & 0b111_10_11111_00000, 0);
}

#[test]
fn test_ppu2_scroll_events_increment_coarse_x_without_wrap() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x18);

    memory.ir.v = 0x001E;

    for _ in 0..9 {
        ppu.tick(true, false, &mut memory);
    }

    assert_eq!(memory.ir.coarse_x(), 31);
    assert_eq!(memory.ir.horizontal_nametable(), 0);
}

#[test]
fn test_ppu2_tick_advances_ppumask_render_delay() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    // Start with rendering disabled, then request both enables via $2001 semantics.
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x00);
    memory.ppu_mask.from_write(0x18);

    assert!(!memory.ppu_mask.background_rendering());
    assert!(!memory.ppu_mask.sprite_rendering());

    // BG should enable after 3 dots, sprite after 4 dots.
    for _ in 0..3 {
        let sprite = memory.ppu_mask.sprite_rendering();
        let bg = memory.ppu_mask.background_rendering();
        ppu.tick(sprite, bg, &mut memory);
    }
    assert!(memory.ppu_mask.background_rendering());
    assert!(!memory.ppu_mask.sprite_rendering());

    let sprite = memory.ppu_mask.sprite_rendering();
    let bg = memory.ppu_mask.background_rendering();
    ppu.tick(sprite, bg, &mut memory);
    assert!(memory.ppu_mask.sprite_rendering());
}

#[test]
fn test_ppu2_pattern_shift_registers_do_not_advance_when_rendering_disabled() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    ppu.pattern_shift_low = 0x4001;
    ppu.pattern_shift_high = 0x8000;

    ppu.tick(false, false, &mut memory);

    assert_eq!(ppu.pattern_shift_low, 0x4001);
    assert_eq!(ppu.pattern_shift_high, 0x8000);
}

#[test]
fn test_ppu2_pattern_shift_registers_advance_on_visible_dot_when_bg_enabled() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x08);

    ppu.pattern_shift_low = 0x4001;
    ppu.pattern_shift_high = 0x8000;

    // event_index 1 => visible scanline dot 1, first active visible shifter dot.
    ppu.event_index = 1;
    ppu.tick(false, true, &mut memory);

    assert_eq!(ppu.pattern_shift_low, 0x8002);
    assert_eq!(ppu.pattern_shift_high, 0x0000);
}

#[test]
fn test_ppu2_pattern_shift_registers_do_not_advance_on_dot_zero() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    ppu.pattern_shift_low = 0x4001;
    ppu.pattern_shift_high = 0x8000;

    // Dot 0 is not in the hardware shifter window (1..=256).
    ppu.event_index = 0;
    ppu.tick(false, true, &mut memory);

    assert_eq!(ppu.pattern_shift_low, 0x4001);
    assert_eq!(ppu.pattern_shift_high, 0x8000);
}

#[test]
fn test_ppu2_pattern_shift_registers_do_not_advance_on_hblank_dot() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    ppu.pattern_shift_low = 0x4001;
    ppu.pattern_shift_high = 0x8000;

    // Dot 257 is outside active BG shift windows (0..255, 321..336).
    ppu.event_index = 257;
    ppu.tick(false, true, &mut memory);

    assert_eq!(ppu.pattern_shift_low, 0x4001);
    assert_eq!(ppu.pattern_shift_high, 0x8000);
}

#[test]
fn test_ppu2_emits_frame_boundaries_for_fps_cap() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    ppu.event_index = 0;
    let start = ppu.tick(false, false, &mut memory);
    assert!(start.frame_start);
    assert!(!start.frame_end);

    ppu.event_index = 240 * WIDTH;
    let end = ppu.tick(false, false, &mut memory);
    assert!(!end.frame_start);
    assert!(end.frame_end);
}

#[test]
fn test_ppu2_leftmost_8_pixels_clip_background() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    ppu.delayed_pixels = Delayed::new(1);
    ppu.event_index = 0;
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x08);

    // Seed tile 0 pixel 0 as non-transparent background color index 1.
    ppu.pattern_shift_low = 0x8000;
    ppu.pattern_shift_high = 0x0000;
    ppu.attr_shift_low = 0x0000;
    ppu.attr_shift_high = 0x0000;

    ppu.palette_table[0] = 0x11;
    ppu.palette_table[1] = 0x22;

    ppu.tick(false, true, &mut memory);

    // BG is clipped at x<8, so backdrop color is output.
    assert_eq!(ppu.screen[0], 0x11);
}

#[test]
fn test_ppu2_leftmost_8_pixels_show_background_when_unclipped() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    ppu.delayed_pixels = Delayed::new(1);
    ppu.event_index = 0;
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x0A);

    ppu.pattern_shift_low = 0x8000;
    ppu.pattern_shift_high = 0x0000;
    ppu.attr_shift_low = 0x0000;
    ppu.attr_shift_high = 0x0000;

    ppu.palette_table[0] = 0x11;
    ppu.palette_table[1] = 0x22;

    ppu.tick(false, true, &mut memory);

    assert_eq!(ppu.screen[0], 0x22);
}

#[test]
fn test_ppu2_leftmost_8_pixels_clip_sprites() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    ppu.delayed_pixels = Delayed::new(1);
    ppu.event_index = 0;
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x10);

    ppu.palette_table[0] = 0x11;
    ppu.palette_table[0x11] = 0x33;

    // Sprite 0 produces color index 1 at x=0.
    ppu.sprite_latches.push(SpriteLatch {
        source_index: 0,
        attr: 0,
        x: 0,
        low: 0x80,
        high: 0x00,
    });

    ppu.tick(true, false, &mut memory);

    // Sprites are clipped at x<8, so backdrop color remains.
    assert_eq!(ppu.screen[0], 0x11);
}

#[test]
fn test_ppu2_leftmost_8_pixels_show_sprites_when_unclipped() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    ppu.delayed_pixels = Delayed::new(1);
    ppu.event_index = 0;
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x14);

    ppu.palette_table[0] = 0x11;
    ppu.palette_table[0x11] = 0x33;

    ppu.sprite_latches.push(SpriteLatch {
        source_index: 0,
        attr: 0,
        x: 0,
        low: 0x80,
        high: 0x00,
    });

    ppu.tick(true, false, &mut memory);

    assert_eq!(ppu.screen[0], 0x33);
}

#[test]
fn test_ppu2_sprite0_hit_requires_source_index_0() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    ppu.delayed_pixels = Delayed::new(1);
    ppu.event_index = WIDTH;
    // Show BG+sprites and do not clip left 8 pixels.
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x1C);

    // Opaque background pixel at x=0.
    ppu.pattern_shift_low = 0x8000;
    ppu.pattern_shift_high = 0x0000;
    ppu.attr_shift_low = 0x0000;
    ppu.attr_shift_high = 0x0000;
    ppu.palette_table[0] = 0x11;
    ppu.palette_table[1] = 0x22;

    // Non-transparent sprite pixel at x=0, but this is NOT sprite 0 from primary OAM.
    ppu.sprite_latches.push(SpriteLatch {
        source_index: 5,
        attr: 0,
        x: 0,
        low: 0x80,
        high: 0x00,
    });

    ppu.tick(true, true, &mut memory);

    assert_eq!(memory.get_force(0x2002) & (1 << BIT_SPRITE_0_HIT), 0);
}

#[test]
fn test_ppu2_sprite0_hit_set_for_source_index_0() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();

    ppu.delayed_pixels = Delayed::new(1);
    ppu.event_index = WIDTH + 8;
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x1C);

    // Opaque background pixel at current dot from pre-seeded shifters.
    ppu.pattern_shift_low = 0x8000;
    ppu.pattern_shift_high = 0x0000;
    ppu.attr_shift_low = 0x0000;
    ppu.attr_shift_high = 0x0000;

    ppu.sprite_latches.push(SpriteLatch {
        source_index: 0,
        attr: 0,
        x: 8,
        low: 0x80,
        high: 0x00,
    });

    ppu.tick(true, true, &mut memory);
    assert_eq!(memory.get_force(0x2002) & (1 << BIT_SPRITE_0_HIT), 0);

    ppu.tick(true, true, &mut memory);
    assert_eq!(memory.get_force(0x2002) & (1 << BIT_SPRITE_0_HIT), 0);

    ppu.tick(true, true, &mut memory);
    assert_eq!(memory.get_force(0x2002) & (1 << BIT_SPRITE_0_HIT), 0);

    ppu.tick(true, true, &mut memory);
    assert_ne!(memory.get_force(0x2002) & (1 << BIT_SPRITE_0_HIT), 0);
}

#[test]
fn test_ppu2_all_8_sprites_fetched_per_scanline() {
    // Regression test: the SPRITE_LS_BITS events must cover all 8 sprite slots
    // (dots 261,269,277,285,293,301,309,317 with step_by(8)).
    // Previously step_by(16) produced only 4 slots, silently dropping sprites 4-7.
    let ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));

    // Count SPRITE_LS_BITS events in the sprite-fetch window (dots 261..321) on scanline 0.
    let slots: Vec<usize> = (261..321)
        .filter(|&x| (ppu.events[x] & SPRITE_LS_BITS) != 0)
        .collect();

    assert_eq!(
        slots.len(), 8,
        "Expected 8 SPRITE_LS_BITS fetch slots in dots 261..320, found {}. Slots: {:?}",
        slots.len(), slots
    );

    // Verify they are evenly spaced every 8 dots.
    for (i, &dot) in slots.iter().enumerate() {
        assert_eq!(dot, 261 + i * 8, "SPRITE_LS_BITS slot {} at unexpected dot {}", i, dot);
    }
}

#[test]
fn test_ppu2_frame_wrap_starts_at_dot_zero() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x18);

    // Position at the final dot of the frame.
    ppu.event_index = HEIGHT * WIDTH - 1;

    // With rendering enabled, odd frames skip dot 0 at frame start.
    ppu.tick(true, true, &mut memory);

    assert_eq!(ppu.event_index, 1);
    assert_eq!(ppu.scanline, 0);
    assert_eq!(ppu.x, 1);
}

#[test]
fn test_ppu2_frame_wrap_odd_skip_resets_sprite_state() {
    let mut ppu = Ppu2::new(&mut MapperBase::new(&Rom::default()));
    let mut memory = NesMemory::new_for_testing();
    memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x18);

    // Seed non-zero state to ensure frame wrap clears it even if dot 0 is skipped.
    ppu.oam2_eval_index = 12;
    ppu.oam2_clear_index = 7;
    ppu.sprite_fetch_index = 3;
    ppu.event_index = HEIGHT * WIDTH - 1;

    ppu.tick(true, true, &mut memory);

    assert_eq!(ppu.event_index, 1);
    assert_eq!(ppu.oam2_eval_index, 0);
    assert_eq!(ppu.oam2_clear_index, 0);
    assert_eq!(ppu.sprite_fetch_index, 0);
}
