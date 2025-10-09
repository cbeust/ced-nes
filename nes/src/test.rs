use crate::app::SharedState;
use crate::constants::{RomInfo, ROM_NAMES};
use crate::emulator::Emulator;
use crate::joypad::Joypad;
use crate::nes_memory::{NesMemory, IR};
use crate::ppu::Ppu;
use crate::rom::Mirroring;
use crate::{get_bits, Args};
use cpu::memory::Memory;
use std::ops::Range;
use std::sync::{Arc, RwLock};

fn find_rom(id: usize) -> RomInfo {
    ROM_NAMES.iter().find(|rom| rom.id == id).cloned().unwrap_or(ROM_NAMES[0].clone())
}

#[test]
pub fn test_ppu2() {
    let mut ppu = Ppu::default();
    let mut cycles_to_first_vbl = 0;
    let mut memory = NesMemory::new_for_testing();
    let mut stop = false;

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

    // assert_eq!(cycles_to_first_vbl, 82182);
    // assert_eq!(cycles_for_a_frame, 89342);
    println!("Total cycles: {cycles_to_first_vbl} {cycles_for_a_frame}");
    println!("Expected    : 82182 89342");
}

// #[test]
// pub fn test_pattern() {
//     let tile_0 = [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7e, 0x00];
//     let tile_1 = [0, 0, 0, 0, 0, 0, 0, 0];
//     let expected = [
//         [0, 0, 0, 1, 1, 0, 0, 0],
//         [0, 0, 1, 1, 1, 0, 0, 0],
//         [0, 0, 0, 1, 1, 0, 0, 0],
//         [0, 0, 0, 1, 1, 0, 0, 0],
//         [0, 0, 0, 1, 1, 0, 0, 0],
//         [0, 0, 0, 1, 1, 0, 0, 0],
//         [0, 1, 1, 1, 1, 1, 1, 0],
//         [0, 0, 0, 0, 0, 0, 0, 0],
//     ];
//     for i in 0..8 {
//         let byte1 = tile_0[i];
//         let byte2 = tile_1[i];
//         let values = Rom::get_pattern_from_bytes(byte1, byte2);
//         assert_eq!(values, expected[i]);
//     }
// }
// #[cfg(test)]
fn mirror(mut mirrors: Range<u16>, addresses: Range<u16>, mirror_fn: impl Fn(u16) -> u16) {
    let mut index = 0;
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
        index += 1;
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
    ppu: Ppu,
    pub memory: NesMemory,
}

impl TestPpu {
    fn new() -> Self {
        let memory = NesMemory::new_for_testing();
        Self { memory, ppu: Ppu::default() }
    }

    fn set_vram(&mut self, address: usize, value: u8) {
        self.ppu.set_vram(address, value, &mut self.memory.mapper);
    }

    fn get_vram(&mut self, address: usize) -> u8 {
        self.ppu.get_vram(address, &mut self.memory.mapper)
    }

    fn tick(&mut self, bg: bool, sprites: bool) {
        self.ppu.tick(bg, sprites, &mut self.memory);
    }

    fn cycle(&self) -> u16 { self.ppu.cycle }
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

#[test]
pub fn test_blarg() {
    let tests = vec![
        // Sprite 0 hits
        551, 552, 553, 554, 555, 556, 558,
        // PPU
        561, 562,
        // Branches
        582, 583, 584,
    ];

    // init_logging(true);
    for test_id in tests {
        let selected_rom = test_id;
        let shared_state = Arc::new(RwLock::new(SharedState::default()));
        let rom_info = find_rom(selected_rom);

        let mut emulator = Emulator::new(
            rom_info.clone(),
            shared_state,
            Arc::new(RwLock::new(Joypad::default())),
            Args::default());

        let mut stop = false;
        let mut success = false;
        let mem = if test_id == 561 || test_id == 562 { 0xf0 } else { 0xf8 };
        let mut previous_pc = 0;
        while !stop {
            let mut has_advanced = false;
            while ! has_advanced {
                (has_advanced, _) = emulator.tick_one();
            }
            if emulator.cpu.pc == previous_pc {
                stop = true;
                success = emulator.cpu.memory.get(mem) == 1;
            }
            previous_pc = emulator.cpu.pc;
            // emulator.tick_one_cycle();
        }

        if success {
            println!("✅ Test \"{}\" passed", rom_info.name());
        } else {
            println!("❌ Test \"{}\" failed: {}", rom_info.name(), emulator.cpu.memory.get(mem));
        }
        assert!(success,
            "❌ Test \"{}\" failed: {}", rom_info.name(), emulator.cpu.memory.get(mem));
    }
}

fn assert_eq(ppu: &Ppu, ir: &IR, v: u16, expected: u16, message: &str) {
    if v != expected {
        println!("{message} -- scanline:{} cycle:{} -- expected {}, got {}",
            ppu.scanline, ppu.cycle, expected, v);
        println!("IR:{ir}");
        panic!("{message}");
    }
}

#[test]
pub fn test_tick() {
    let mut ppu = Ppu::default();
    ppu.cycle = 0;
    let mem = &mut NesMemory::new_for_testing();
    // for _ in 0..WIDTH * HEIGHT {
    //     ppu.pixels.push(Pixel::new(0, 0, 0, false));
    // }

    let mut stop = false;
    let mut frame_count = 0;
    while !stop {
        ppu.tick(true, true, mem);
        if ppu.scanline == 240 && ppu.cycle == 256 {
            println!("Incrementing frame:{frame_count}");
            frame_count += 1;
        }
        stop = frame_count == 10;
        if !stop && ppu.scanline < 240 && ppu.cycle < 256 {
            assert_eq(&ppu, &mem.ir, ppu.scanline / 8, mem.ir.coarse_y(), "coarse_y");
            assert_eq(&ppu, &mem.ir, ppu.scanline % 8, mem.ir.fine_y(), "fine_y");
            assert_eq(&ppu, &mem.ir, ppu.cycle / 8, mem.ir.coarse_x(), "coarse_x");
            // println!("Scanline:{} cycle:{} ir:{}", ppu.scanline, ppu.cycle, mem.ir);
            // println!();
        }
        // if ppu.cycle < 256 && mem.ir.screen_x() != ppu.cycle {
        //     println!("x:{} != cycle:{} ir:{}",
        //         mem.ir.screen_x(), ppu.cycle, mem.ir);
        //     println!();
        // }

    }
}

#[test]
pub fn test_2000_range() {
    // 2000
    {
        let mem = &mut NesMemory::new_for_testing();
        assert_eq!(0, mem.ir.t);
        let data = [
            (0, 0, 0), (0, 1, 0x400), (0, 2, 0x800), (0, 3, 0xc00),
            (0xffff, 0, 0b1111_0011_1111_1111), (0xffff, 1, 0b1111_0111_1111_1111),
            (0xffff, 0b10, 0b1111_1011_1111_1111), (0xffff, 0b11, 0b1111_1111_1111_1111)
        ];
        for (_index, (initial, value, expected)) in data.iter().enumerate() {
            mem.ir.t = *initial;
            mem.set(0x2000, *value);
            assert_eq!(mem.ir.t, *expected as u16, "Iteration {index} failed");
        }
    }

    // 2002
    {
        let mem = &mut NesMemory::new_for_testing();
        mem.ir.w = true;
        mem.get(0x2002);
        assert_eq!(mem.ir.w, false);
    }

    // 2005
    // t: ....... ...ABCDE <- d: ABCDE...
    // x:              FGH <- d: .....FGH
    // w:                  <- 1
    {
        let mem = &mut NesMemory::new_for_testing();
        mem.ir.x = 0b111;
        mem.ir.t = 0xffff;
        mem.set(0x2005, 0xaa);
        assert_eq!(mem.ir.t, 0b1111_1111_1110_0000 | (0xaa >> 3), "Expected {:0b} but got {:0b}",
            0b1110_0000 | (0xaa >> 3), mem.ir.t);
        assert_eq!(mem.ir.x, 0xaa & 0b111);
        assert_eq!(mem.ir.w, true);
    }

    // 2005
    // t: FGH..AB CDE..... <- d: ABCDEFGH
    // w:                  <- 0
    {
        let mem = &mut NesMemory::new_for_testing();
        mem.ir.w = true;
        mem.ir.t = 0xffff;
        // 1010_1010
        // abcd_efgh
        mem.set(0x2005, 0xaa);
        assert_eq!(mem.ir.t, 0b010_11_10101_11111, "$2005: Expected {:0b} but got {:0b}",
            0b010_11_10101_11111, mem.ir.t);
        assert_eq!(mem.ir.w, false);
    }

    // 2006
    // t: .CDEFGH ........ <- d: ..CDEFGH
    //        <unused>     <- d: AB......
    // t: Z...... ........ <- 0 (bit Z is cleared)
    // w:                  <- 1
    {
        let mem = &mut NesMemory::new_for_testing();
        mem.ir.w = false;
        mem.ir.t = 0xffff;

        mem.set(0x2006, 0xaa);
        // let ected = 0b1010_1010_1010_1010
        let expected = 0b1010_1010_1111_1111;
        assert_eq!(mem.ir.t, expected, "$2006-1: Expected {:0b} but got {:0b}",
            expected, mem.ir.t);
        assert_eq!(mem.ir.w, true);
    }

    // 2006
    // t: ....... ABCDEFGH <- d: ABCDEFGH
    // v: <...all bits...> <- t: <...all bits...>
    // w:                  <- 0
    {
        let mem = &mut NesMemory::new_for_testing();
        mem.ir.w = true;
        mem.ir.t = 0xc0ff;
        mem.ir.set_v(0xffff);

        mem.set(0x2006, 0xaa);
        assert_eq!(mem.ir.t, 0xc0aa, "$2006-2: Expected {:0b} but got {:0b}", 0xc0aa, mem.ir.t);
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
    ppu.memory.set(0x2005, 128);
    for _ in 0..200_000 {
        let x = ppu.cycle();
        let y = ppu.scanline();
        if y > 0 && y < 240 {
            if x < 128 && ppu.memory.ir.nametable() != 0 {
                assert!(false, "{x},{y} FAILED: NAMETABLE SHOULD BE 0");
            } else if x >= 129 && x < 256 && ppu.memory.ir.nametable() != 1 {
                assert!(false, "{x},{y} FAILED: NAMETABLE SHOULD BE 1");
            }
        }
        ppu.tick(true, true);
    }
}

#[test]
pub fn test_tile_addresses() {
    let mut mem = &mut NesMemory::new_for_testing();
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
    let mut count = 0;
    let mut stop = false;
    while !stop {
        ppu.tick(true, true);
        count += 1;
        stop = ppu.scanline() == 0 && ppu.cycle() == 0;
    }
    assert_eq!(count, 89314);
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
        (Mirroring::Horizontal, 0x2368, Vram_A),
        (Mirroring::Horizontal, 0x2768, Vram_A),
        (Mirroring::Horizontal, 0x2801, Vram_B),
        (Mirroring::Horizontal, 0x2c01, Vram_B),
        (Mirroring::Vertical, 0x2768, Vram_B),
        (Mirroring::Vertical, 0x2f68, Vram_B),
        (Mirroring::Vertical, 0x2001, Vram_A),
        (Mirroring::Vertical, 0x2801, Vram_A),
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

// #[test]
// pub fn test_mapper2() {
//     let mut mem: Vec<u8> = Vec::new();
//     for _ in 0..0x4000 * 8 { mem.push(0); }
//     let mut index = 0;
//     let mut value = 1;
//     for _ in 0..8 {
//         mem[index] = value;
//         index += 0x4001;
//         value += 0x11;
//     }
//
//     let mut m = Mapper2::new(mem);
//
//     let mut expected = 0x1;
//     let address = 0x8000;
//     for i in 0..8 {
//         m.write(0xc000, i);
//         let result = m.read(address + i as u16);
//         assert_eq!(result, expected,
//             "Failed {address:04X}: expected {expected:02X} got {result:02X}");
//         expected += 0x11;
//     }
// }
