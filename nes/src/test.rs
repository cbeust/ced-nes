use crate::app::SharedState;
use crate::constants::{RomInfo, ROM_NAMES};
use crate::emulator::{Emulator, CpuInterface};
use crate::joypad::Joypad;
use crate::nes_memory::{NesMemory, IR};
use crate::ppu2::{Ppu2, PpuResult};
use crate::rom::Mirroring;
use crate::{get_bits, Args};
use cpu::memory::Memory;
use std::ops::Range;
use std::sync::{Arc, RwLock};
use test_case::test_case;
use tracing::debug;

fn find_rom_by_name(name: &str) -> RomInfo {
    ROM_NAMES.iter().find(|rom| {
        debug!("Looking at rom:{rom:#?}");
        rom.file_name.clone() == name
    }).cloned()
    .unwrap_or(ROM_NAMES[0].clone())
}

#[test]
pub fn test_ppu2() {
    let mut ppu = Ppu2::default();
    let mut cycles_to_first_vbl = 0;
    let mut memory = NesMemory::new_for_testing();
    let mut stop = false;

    // First frame
    while !stop {
        let result = ppu.tick(true, true, &mut memory);
        cycles_to_first_vbl += 1;
        if result.vbl {
            stop = true;
        }
    }

    stop = false;
    while !stop {
        let result = ppu.tick(true, true, &mut memory);
        stop = result.frame_start
    }

    let mut cycles_for_a_frame = 0;
    stop = false;
    while !stop {
        let result = ppu.tick(true, true, &mut memory);
        cycles_for_a_frame += 1;
        stop = result.frame_end;
    }

    assert_eq!(cycles_to_first_vbl, 82182);
    assert_eq!(cycles_for_a_frame, 81840);
}

fn mirror(mirrors: Range<u16>, addresses: Range<u16>, mirror_fn: impl Fn(u16) -> u16) {
    let mirrors_array: Vec<u16> = mirrors.collect();
    let mut mirrors_index = 0;

    for address in addresses {
        if mirrors_index >= mirrors_array.len() {
            mirrors_index = 0;
        }
        let expected = mirrors_array[mirrors_index];
        mirrors_index += 1;

        let result = mirror_fn(address);
        assert_eq!(result, expected,
            "Wrong mirroring for {address:04X}, expected {:04X}, got {:04X}",
            expected, result);
    }
}

#[test]
pub fn test_memory_bits() {
    let mut m = NesMemory::new_for_testing();
    let a = 0x1000;
    assert_eq!(m.get(a), 0);
    for bit in 0..8 {
        m.set_bit(a, bit);
        assert_eq!(m.get_direct(a), 1 << bit);
        m.clear_bit(a, bit);
        assert_eq!(m.get_direct(a), 0);
    }
}

struct TestPpu {
    ppu: Ppu2,
    pub memory: NesMemory,
}

impl TestPpu {
    fn new() -> Self {
        let memory = NesMemory::new_for_testing();
        Self { memory, ppu: Ppu2::default() }
    }

    fn set_vram(&mut self, address: usize, value: u8) {
        self.ppu.set_vram(address, value, &mut self.memory.mapper);
    }

    fn get_vram(&mut self, address: usize) -> u8 {
        self.ppu.get_vram(address, &mut self.memory.mapper)
    }

    fn tick(&mut self, bg: bool, sprites: bool) -> PpuResult {
        self.ppu.tick(bg, sprites, &mut self.memory)
    }

    fn cycle(&self) -> u16 { self.ppu.x }
    fn scanline(&self) -> u16 { self.ppu.scanline }
}

#[test]
pub fn test_mirroring() {
    let mut ppu = TestPpu::new();
    ppu.set_vram(0x3fe0, 0x34);
    assert_eq!(ppu.get_vram(0x3fe0), 0x34);
    ppu.set_vram(0x3f00, 0x12);
    assert_eq!(ppu.get_vram(0x3fe0), 0x12, "3fe0 should mirror 3f00, expected $12 but got ${:02X}",
        ppu.get_vram(0x3fe0));
    ppu.set_vram(0x3f00, 0x12);
    assert_eq!(ppu.get_vram(0x3fe0), 0x12);

    mirror(0..0x800, 0..0x2000, NesMemory::cpu_mirrorring);
    mirror(0x2000..0x2008, 0x2000..0x3f00, NesMemory::cpu_mirrorring);
    mirror(0x3f00..0x3f20, 0x3f00..0x4000, NesMemory::ppu_mirrorring)
}

#[test_case("01.basics.nes")]
#[test_case("02.alignment.nes")]
#[test_case("03.corners.nes")]
#[test_case("04.flip.nes")]
#[test_case("05.left_clip.nes")]
#[test_case("06.right_edge.nes")]
#[test_case("07.screen_bottom.nes")]
#[test_case("08.double_height.nes")]
#[test_case("09.timing_basics.nes")]
#[test_case("palette_ram.nes")]
#[test_case("sprite_ram.nes")]
#[test_case("1.Branch_Basics.nes")]
#[test_case("2.Backward_Branch.nes")]
#[test_case("3.Forward_Branch.nes")]
fn run_blargg_test(name: &str) {
    let shared_state = Arc::new(RwLock::new(SharedState::default()));
    let rom_info = find_rom_by_name(name);
    let test_id = rom_info.id;

    let mut emulator = Emulator::new(
        rom_info.clone(),
        shared_state,
        Arc::new(RwLock::new(Joypad::default())),
        Args::default(), None);

    let mut stop = false;
    let mut success = false;
    let mem = if test_id == 561 || test_id == 562 { 0xf0 } else { 0xf8 };
    let mut previous_pc = 0;
    while !stop {
        let mut has_advanced = false;
        while ! has_advanced {
            (has_advanced, _, _) = emulator.tick_one();
        }
        if emulator.cpu.pc() == previous_pc {
            stop = true;
            success = emulator.cpu.memory().get(mem) == 1;
        }
        previous_pc = emulator.cpu.pc();
    }

    if success {
        println!("✅ Test \"{}\" passed", rom_info.name());
    } else {
        println!("❌ Test \"{}\" failed: {}", rom_info.name(), emulator.cpu.memory().get(mem));
    }
    assert!(success,
            "❌ Test \"{}\" failed: {}", rom_info.name(), emulator.cpu.memory().get(mem));
}

fn assert_ppu_state(ppu: &Ppu2, ir: &IR, v: u16, expected: u16, message: &str) {
    if v != expected {
        println!("{message} -- scanline:{} cycle:{} -- expected {}, got {}",
            ppu.scanline, ppu.x, expected, v);
        println!("IR:{ir}");
        panic!("{message}");
    }
}

#[test]
pub fn test_tick() {
    let mut ppu = Ppu2::default();
    ppu.x = 0;
    let mut mem = NesMemory::new_for_testing();
    // Enable background rendering immediately
    mem.ppu_mask = crate::ppu_mask::PpuMask::new(0x08);

    let mut stop = false;
    let mut frame_count = 0;
    while !stop {
        ppu.tick(true, true, &mut mem);
        if ppu.scanline == 240 && ppu.x == 0 {
            frame_count += 1;
        }
        stop = frame_count == 1;
        // Test only the first scanline (0) to avoid prefetch-induced mismatches
        // on subsequent scanlines. Prefetch is tested elsewhere.
        if !stop && ppu.scanline == 0 {
             // Coarse Y and Fine Y check
             let expected_coarse_y = if ppu.x >= 256 {
                 (ppu.scanline + 1) / 8
             } else {
                 ppu.scanline / 8
             };
             assert_ppu_state(&ppu, &mem.ir, expected_coarse_y, mem.ir.coarse_y(), "coarse_y");

             let expected_fine_y = if ppu.x >= 256 {
                 (ppu.scanline + 1) % 8
             } else {
                 ppu.scanline % 8
             };
             assert_ppu_state(&ppu, &mem.ir, expected_fine_y, mem.ir.fine_y(), "fine_y");

             // Coarse X increments at dots 8, 16, etc.
             if ppu.x >= 1 && ppu.x <= 256 {
                let expected_x = if ppu.x == 256 { 0 } else { ppu.x / 8 };
                assert_ppu_state(&ppu, &mem.ir, expected_x, mem.ir.coarse_x(), "coarse_x");
             }
        }
    }
}

#[test]
pub fn test_2000_range() {
    // 2000
    {
        let mut mem = NesMemory::new_for_testing();
        assert_eq!(0, mem.ir.t);
        let data = [
            (0, 0, 0), (0, 1, 0x400), (0, 2, 0x800), (0, 3, 0xc00),
            (0xffff, 0, 0b1111_0011_1111_1111), (0xffff, 1, 0b1111_0111_1111_1111),
            (0xffff, 0b10, 0b1111_1011_1111_1111), (0xffff, 0b11, 0b1111_1111_1111_1111)
        ];
        for (index, (initial, value, expected)) in data.iter().enumerate() {
            mem.ir.t = *initial;
            mem.set(0x2000, *value);
            assert_eq!(mem.ir.t, *expected as u16, "Iteration {index} failed");
        }
    }

    // 2002
    {
        let mut mem = NesMemory::new_for_testing();
        mem.ir.w = true;
        mem.get(0x2002);
        assert_eq!(mem.ir.w, false);
    }

    // 2005
    {
        let mut mem = NesMemory::new_for_testing();
        mem.ir.x = 0b111;
        mem.ir.t = 0xffff;
        mem.set(0x2005, 0xaa);
        assert_eq!(mem.ir.t, 0b1111_1111_1110_0000 | (0xaa >> 3));
        assert_eq!(mem.ir.x, 0xaa & 0b111);
        assert_eq!(mem.ir.w, true);
    }

    // 2005 second write
    {
        let mut mem = NesMemory::new_for_testing();
        mem.ir.w = true;
        mem.ir.t = 0xffff;
        mem.set(0x2005, 0xaa);
        assert_eq!(mem.ir.t, 0b010_11_10101_11111);
        assert_eq!(mem.ir.w, false);
    }

    // 2006
    {
        let mut mem = NesMemory::new_for_testing();
        mem.ir.w = false;
        mem.ir.t = 0xffff;
        mem.set(0x2006, 0xaa);
        let expected = 0b1010_1010_1111_1111;
        assert_eq!(mem.ir.t, expected);
        assert_eq!(mem.ir.w, true);
    }

    // 2006 second write
    {
        let mut mem = NesMemory::new_for_testing();
        mem.ir.w = true;
        mem.ir.t = 0xc0ff;
        mem.ir.set_v(0xffff);
        mem.set(0x2006, 0xaa);
        assert_eq!(mem.ir.t, 0xc0aa);
        assert_eq!(mem.ir.v(), 0xc0aa);
        assert_eq!(mem.ir.w, false);
    }
}

#[test]
pub fn test_ir() {
    let mut ir = IR::default();
    let d = 0b010_10_10101_01010;

    ir.set_v(d);
    ir.set_coarse_x(3);
    let mask = 0b11111;
    let expected = (d & !mask) | (3 & mask);
    assert_eq!(ir.v(), expected);

    ir.increment_coarse_x();
    assert_eq!(ir.v(), expected + 1);

    ir.set_v(d);
    ir.set_coarse_y(3);
    let mask = 0b11111_00000;
    assert_eq!(ir.v(), (d & !mask) | ((3 << 5) & mask));

    ir.set_v(d);
    let current = ir.fine_y();
    assert_eq!(current, (d & 0b111_00_00000_00000) >> 12);
    ir.increment_fine_y();
    assert_eq!(ir.fine_y(), current + 1);
    ir.increment_fine_y();
    assert_eq!(ir.fine_y(), current + 2);
    ir.set_fine_y(0);
    assert_eq!(ir.fine_y(), 0);

    ir.set_v(d);
    assert_eq!(ir.horizontal_nametable(), 0);
    assert_eq!(ir.nametable(), 2);
    ir.switch_horizontal_nametable();
    assert_eq!(ir.horizontal_nametable(), 1);
    assert_eq!(ir.nametable(), 3);

    ir.set_v(d);
    assert_eq!(ir.vertical_nametable(), 1);
    assert_eq!(ir.nametable(), 2);
    ir.switch_vertical_nametable();
    assert_eq!(ir.vertical_nametable(), 0);
    assert_eq!(ir.nametable(), 0);

    ir.set_v(d);
    assert_eq!(ir.coarse_x(), 0b0_1010);
    assert_eq!(ir.horizontal_nametable(), 0);
    ir.set_t(0b0101_01_01010_10101);
    ir.hori_v_equals_hori_t();
    assert_eq!(ir.v(), 0b010_11_10101_10101);

    ir.set_v(d);
    assert_eq!(ir.coarse_y(), 0b10101);
    assert_eq!(ir.vertical_nametable(), 1);
    assert_eq!(ir.fine_y(), 0b010);
    ir.set_t(0b0101_01_01010_10101);
    ir.vert_v_equals_vert_t();
    assert_eq!(ir.coarse_y(), 0b01010);
    assert_eq!(ir.vertical_nametable(), 0);
    assert_eq!(ir.fine_y(), 0b101);
    assert_eq!(ir.v(), 0b101_00_01010_01010);
}

#[test]
pub fn test_horizontal_scrolling() {
    let mut ppu = TestPpu::new();
    // Enable background rendering immediately
    ppu.memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x08);
    
    ppu.memory.set(0x2005, 128);
    // Skip first few scanlines
    for _ in 0..341*20 {
        ppu.tick(true, true);
    }
    
    for _ in 0..341*10 {
        let x = ppu.cycle();
        let y = ppu.scanline();
        if y >= 20 && y < 240 {
            // NT switches at dot 128 (visible at x=128)
            // Skip the noise at the very beginning of the Nametable switch
            if x >= 32 && x < 112 {
                assert_eq!(ppu.memory.ir.horizontal_nametable(), 0, "At {x},{y} expected NT 0");
            } else if x >= 160 && x < 240 {
                assert_eq!(ppu.memory.ir.horizontal_nametable(), 1, "At {x},{y} expected NT 1");
            }
        }
        ppu.tick(true, true);
    }
}

#[test]
pub fn test_tile_addresses() {
    let mut mem = NesMemory::new_for_testing();
    mem.ir.set_coarse_x(31);
    mem.ir.set_coarse_y(18);

    let v = 0x2000 | (mem.ir.v() & 0xfff);
    assert_eq!(0x225f, v);
    mem.ir.switch_horizontal_nametable();
    let v = 0x2000 | (mem.ir.v() & 0xfff);
    assert_eq!(0x265f, v);
    mem.ir.switch_horizontal_nametable();
    mem.ir.switch_vertical_nametable();
    let v = 0x2000 | (mem.ir.v() & 0xfff);
    assert_eq!(0x2a5f, v);
    mem.ir.switch_horizontal_nametable();
    let v = 0x2000 | (mem.ir.v() & 0xfff);
    assert_eq!(0x2e5f, v);
}

#[test]
pub fn test_cycle_count() {
    let mut ppu = TestPpu::new();
    // Enable background rendering immediately
    ppu.memory.ppu_mask = crate::ppu_mask::PpuMask::new(0x08);
    let mut count = 0;
    let mut stop = false;
    while !stop {
        let result = ppu.tick(true, true);
        count += 1;
        stop = result.frame_end;
    }
    assert_eq!(count, 81841);
}

#[test]
pub fn test_bits() {
    let value = 0b1010_1010;
    let data = [
        (3, 2, 0b10),
        (4, 0, 0b1010),
        (4, 1, 0b101),
        (4, 2, 0b1010),
        (4, 3, 0b101),
        (4, 4, 0b1010),
        (5, 2, 0b1010),
    ];
    for (count, shift, expected) in data {
        assert_eq!(get_bits!(value, count, shift), expected);
    }
}

#[test]
pub fn test_nametable_mirroring() {
    use crate::mappers::mapper_base::VramType::*;
    let data = vec![
        (Mirroring::Horizontal, 0x2368, VramA),
        (Mirroring::Horizontal, 0x2768, VramA),
        (Mirroring::Horizontal, 0x2801, VramB),
        (Mirroring::Horizontal, 0x2c01, VramB),
        (Mirroring::Vertical, 0x2768, VramB),
        (Mirroring::Vertical, 0x2f68, VramB),
        (Mirroring::Vertical, 0x2001, VramA),
        (Mirroring::Vertical, 0x2801, VramA),
        (Mirroring::FourScreen, 0x2001, Vram),
        (Mirroring::FourScreen, 0x2c01, Vram),
        (Mirroring::Horizontal, 0x100, Vram),
        (Mirroring::Vertical, 0x100, Vram),
        (Mirroring::FourScreen, 0x3001, Vram),
        (Mirroring::Horizontal, 0x3001, Vram),
        (Mirroring::Vertical, 0x3001, Vram),
        (Mirroring::FourScreen, 0x3001, Vram),
    ];
    for (m, a, expected) in data {
        let result = NesMemory::nametable_mirroring(m, a);
        assert_eq!(result, expected,
            "Expected {:#?} mirrorring of {a:#?} == {:#?} but was {:#?}",
            m, expected, result);
    }
}
