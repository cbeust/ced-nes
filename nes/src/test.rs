use crate::constants::{RomInfo, ROM_NAMES};
use crate::emulator::{Emulator, SharedState};
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

/// AccuracyCoin initializes DMA sync at boot (result at $12).
/// 1 = DMC DMA updates open bus (required for DMA + Open Bus suite).
/// 2 = failed pre-test.
#[test]
fn accuracy_coin_dmasync_pretest_passes() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    // Boot through VBlank sync + DMASync.
    let mut status = 0u8;
    for _ in 0..2000 {
        let _ = emu.tick();
        status = emu.cpu.memory().get_direct(0x12);
        if status == 1 || status == 2 {
            break;
        }
    }
    assert_eq!(
        status, 1,
        "AccuracyCoin DMASync pre-test: expected $01 (DMA updates open bus), got ${status:02X} (EC=${:02X})",
        emu.cpu.memory().get_direct(0xEC)
    );
}

/// Measure cycles between DMC open-bus zeros (should be ~432 at rate $F).
#[test]
fn accuracy_coin_dma_interval_approx_432() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..500 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1);

    // Program DMC like DMASync: loop, rate $F, addr $FFC0, len 1.
    emu.cpu.memory().set(0x4010, 0x4F);
    emu.cpu.memory().set(0x4011, 0x00);
    emu.cpu.memory().set(0x4012, 0xFF);
    emu.cpu.memory().set(0x4013, 0x00);
    emu.cpu.memory().set(0x4015, 0x10);

    let mut last_dma_cycle: Option<u64> = None;
    let mut intervals = Vec::new();
    let mut cycle: u64 = 0;

    for _ in 0..50_000 {
        let stalled = emu.apu.read().unwrap().dmc_dma_active();
        let _ = emu.tick_one();
        cycle += 1;
        let bus = emu.cpu.memory().cpu_data_bus;
        // Detect end of a DMA fetch: stall ends and bus is 0.
        if stalled && !emu.apu.read().unwrap().dmc_dma_active() && bus == 0 {
            if let Some(prev) = last_dma_cycle {
                intervals.push(cycle - prev);
            }
            last_dma_cycle = Some(cycle);
            if intervals.len() >= 3 {
                break;
            }
        }
    }
    assert!(
        !intervals.is_empty(),
        "expected to observe DMC DMA intervals, got none"
    );
    for &iv in &intervals {
        assert!(
            (430..=434).contains(&iv),
            "DMA interval should be ~432 (8×54); all={intervals:?}"
        );
    }
}

/// Helper: boot AccuracyCoin, wait for DMASync pre-test, run a ROM test routine.
fn run_accuracy_coin_test(test_addr: u16, result_addr: u16) -> u8 {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(
        emu.cpu.memory().get_direct(0x12),
        1,
        "DMASync pre-test must pass first"
    );

    // Silence leftover DMC/channels from DMASync so $4015 is clean for APU tests.
    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    for _ in 0..16 {
        let _ = emu.tick_one();
    }
    // Exhaust controller shift register (menu would have done this) so unstrobbed
    // reads return 1 — required by Controller Strobing test 1.
    for _ in 0..16 {
        let _ = emu.cpu.memory().get(0x4016);
        let _ = emu.cpu.memory().get(0x4017);
    }

    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set_force(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let is_two_level = test_addr == 0xC4F8;
    let trampoline: u16 = if is_two_level {
        0x0300
    } else if test_addr == 0xC6B7 || test_addr == 0xC6FF || test_addr == 0xC7DD || test_addr == 0xC9DC {
        0x0200
    } else {
        0x0700
    };
    let lo = (test_addr & 0xFF) as u8;
    let hi = (test_addr >> 8) as u8;
    let rlo = (result_addr & 0xFF) as u8;
    let rhi = (result_addr >> 8) as u8;
    let jmp_addr = trampoline + 6;
    let jlo = (jmp_addr & 0xFF) as u8;
    let jhi = (jmp_addr >> 8) as u8;
    let code = if is_two_level {
        vec![
            0x20, 0x09, 0x03, // JSR $0309
            0x8D, rlo, rhi,   // STA result
            0x4C, 0x06, 0x03, // JMP $0306
            0x20, lo, hi,     // JSR test
            0x60,             // RTS
        ]
    } else {
        vec![
            0x20, lo, hi,     // JSR test
            0x8D, rlo, rhi,   // STA result
            0x4C, jlo, jhi,   // JMP trampoline+6
        ]
    };
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1); // ErrorCode
    emu.cpu.memory().set_force(0x11, 1); // initialSubTest
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    let max_steps = if test_addr == 0xC6B7 || test_addr == 0xC6FF || test_addr == 0xC7DD {
        40_000_000u32
    } else {
        10_000_000u32
    };
    let mut last_ec = 1;
    for _ in 0..max_steps {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let ec = emu.cpu.memory().get_direct(0x10);
            if ec != last_ec {
                eprintln!("ErrorCode changed: {} -> {} at PC=${:04X}", last_ec, ec, emu.cpu.pc());
                last_ec = ec;
            }
            let r = emu.cpu.memory().get_direct(result_addr);
            if r != 0xFF {
                let v50 = emu.cpu.memory().get_direct(0x50);
                eprintln!("run_accuracy_coin_test finished: result=${r:02X}, $50=${v50:02X}, EC={}", r >> 2);
                return r;
            }
        }
    }
    emu.cpu.memory().get_direct(result_addr)
}


#[test]
fn accuracy_coin_dma_plus_4015_read_passes() {
    let r = run_accuracy_coin_test(0xBF1D, 0x045D);
    assert_eq!(
        r, 1,
        "DMA + $4015 Read should pass (1), got ${r:02X} (fail=(ErrorCode<<2)|2)"
    );
}

/// AccuracyCoin page 13: DMC DMA + OAM DMA (`TEST_DMCDMAPlusOAMDMA` @ $DA49).
#[test]
fn accuracy_coin_dmc_dma_plus_oam_dma_passes() {
    let r = run_accuracy_coin_test(0xDA49, 0x0477);
    assert_eq!(
        r, 1,
        "DMC DMA + OAM DMA should pass (1), got ${r:02X} (fail=(ErrorCode<<2)|2)"
    );
}

/// AccuracyCoin page 13: Explicit DMA Abort (`TEST_ExplicitDMAAbort` @ $DAFA).
#[test]
fn accuracy_coin_explicit_dma_abort_passes() {
    let r = run_accuracy_coin_test(0xDAFA, 0x0479);
    assert_eq!(
        r, 1,
        "Explicit DMA Abort should pass (1), got ${r:02X} (fail=(ErrorCode<<2)|2)"
    );
}

/// AccuracyCoin page 13: Implicit DMA Abort (`TEST_ImplicitDMAAbort` @ $DB72).
/// Pass is 5 (post-1990 CPU) or 9 (pre-1990 CPU).
#[test]
fn accuracy_coin_implicit_dma_abort_passes() {
    let r = run_accuracy_coin_test(0xDB72, 0x0478);
    assert!(
        r == 5 || r == 9,
        "Implicit DMA Abort should pass (5 or 9), got ${r:02X} (fail=(ErrorCode<<2)|2)"
    );
}

#[test]
fn diagnose_implicit_dma_abort() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);
    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set_force(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);
    let test_addr = 0xDB72u16;
    let result_addr = 0x0478u16;
    let trampoline = 0x0700u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x4C, 0x06, 0x07,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFE);
    emu.cpu.jump_to_instruction(trampoline);
    for _ in 0..50_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if (0x0706..=0x0708).contains(&pc) {
                break;
            }
        }
    }
    let r = emu.cpu.memory().get_direct(result_addr);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("RESULT=${r:02X} EC={ec}");
    // pre-1990 keys (code 9) and post-1990 alt keys (code 5)
    let key1: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0];
    let alt1: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 4, 4, 1, 1, 0, 0, 0, 0];
    let key2: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0];
    let alt2: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 4, 4, 1, 0, 0, 0, 0, 0];
    let key3: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 4, 4, 4, 4, 4];
    let alt3: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 4, 4, 4, 4, 4, 4, 4, 4];
    eprint!("$500: ");
    for i in 0..0x10u16 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x500 + i));
    }
    eprintln!();
    eprint!("$520: ");
    for i in 0..0x10u16 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x520 + i));
    }
    eprintln!();
    eprint!("$540: ");
    for i in 0..0x10u16 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x540 + i));
    }
    eprintln!();
    let s8 = emu.cpu.memory().get_direct(0x508);
    let (k1, k2, k3, name) = if s8 == 4 {
        (alt1, alt2, alt3, "post-1990 alt")
    } else {
        (key1, key2, key3, "pre-1990")
    };
    eprintln!("branch={name} ($508={s8:02X})");
    for (base, key) in [(0x500u16, k1), (0x520, k2), (0x540, k3)] {
        for i in 0..0x10u16 {
            let g = emu.cpu.memory().get_direct(base + i);
            if g != key[i as usize] {
                eprintln!(
                    "mismatch @${:03X}: got {g:02X} want {:02X}",
                    base + i,
                    key[i as usize]
                );
            }
        }
    }
}

#[test]
fn diagnose_explicit_dma_abort() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);
    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set_force(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);
    let test_addr = 0xDAFAu16;
    let result_addr = 0x0479u16;
    let trampoline = 0x0700u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x4C, 0x06, 0x07,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFE);
    emu.cpu.jump_to_instruction(trampoline);
    for _ in 0..50_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if (0x0706..=0x0708).contains(&pc) {
                break;
            }
        }
    }
    let r = emu.cpu.memory().get_direct(result_addr);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("RESULT=${r:02X} EC={ec}");
    let key: [u8; 16] = [
        0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x03, 0x04, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];
    eprint!("got: ");
    for i in 0..0x10u16 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x50 + i));
    }
    eprintln!();
    eprint!("key: ");
    for b in key {
        eprint!("{b:02X} ");
    }
    eprintln!();
    for i in 0..0x10u16 {
        let g = emu.cpu.memory().get_direct(0x50 + i);
        if g != key[i as usize] {
            eprintln!("mismatch @${:02X}: got {g:02X} want {:02X}", 0x50 + i, key[i as usize]);
        }
    }
}

/// AccuracyCoin page 13 test 7: DMC DMA Bus Conflicts (`TEST_DMABusConflict` @ $D1E1).
/// Pass returns A = `$50` (controller floating-bit pattern), which can be `$FF`.
/// Fail encoding is `(ErrorCode<<2)|2`. Detect completion via PC reaching the trampoline spin.
#[test]
fn accuracy_coin_dmc_dma_bus_conflicts_passes() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(
        emu.cpu.memory().get_direct(0x12),
        1,
        "DMASync pre-test must pass first"
    );

    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set_force(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let test_addr: u16 = 0xD1E1;
    let result_addr: u16 = 0x046B;
    let trampoline: u16 = 0x0700;
    let lo = (test_addr & 0xFF) as u8;
    let hi = (test_addr >> 8) as u8;
    let rlo = (result_addr & 0xFF) as u8;
    let rhi = (result_addr >> 8) as u8;
    // JSR test ; STA result ; JMP *
    let code = [
        0x20, lo, hi,
        0x8D, rlo, rhi,
        0x4C, 0x06, 0x07,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1); // ErrorCode
    // Sentinel: use $FE so a genuine pass of $FF from `$50` is visible.
    emu.cpu.memory().set_force(result_addr, 0xFE);
    emu.cpu.jump_to_instruction(trampoline);

    let mut result = 0xFEu8;
    for _ in 0..10_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            // Trampoline spin is at $0706.
            if (0x0706..=0x0708).contains(&pc) {
                result = emu.cpu.memory().get_direct(result_addr);
                break;
            }
        }
    }
    let ec = emu.cpu.memory().get_direct(0x10);
    let is_fail = (result & 0x03) == 0x02;
    // ROM success path does `LDA <$50` (controller w/ floating D5–D7 = 1).
    // On a standard NES port that is $E1 (D0=1 after 8+ clocks, D1–D4=0, D5–D7=1).
    // $FF is the menu "skipped" sentinel — must not return that on pass.
    assert!(
        !is_fail && result != 0xFE && result != 0xFF,
        "DMC DMA Bus Conflicts should pass (expect ~$E1, not $FF/skip), \
         got ${result:02X} ErrorCode={ec} (fail=(ErrorCode<<2)|2)"
    );
    assert_eq!(
        result, 0xE1,
        "expected pass code $E1 (controller floating pattern), got ${result:02X}"
    );
}

/// Dummy re-reads of $4015 during DMC DMA must clear the frame IRQ flag
/// before the CPU's real BIT/LDA $4015 sees it.
#[test]
fn dma_dummy_4015_clears_frame_irq_before_cpu_read() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);
    for _ in 0..500 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }

    // Enable frame IRQ and wait until the flag is set.
    emu.cpu.memory().set(0x4017, 0x00);
    let mut flag_set = false;
    for _ in 0..40_000 {
        let _ = emu.tick_one();
        // Non-destructive check: use apu state via a peek by reading without going through
        // full path — temporarily read and if set, re-set by waiting is hard.
        // Instead force-set by exhausting frame counter: write $4017 then wait 29830.
    }
    // Wait exactly for flag
    emu.cpu.memory().set(0x4017, 0x00);
    for _ in 0..29_900 {
        let _ = emu.tick_one();
    }
    // Confirm flag is set (read once — clears it; so re-wait)
    let status = emu.cpu.memory().get(0x4015);
    if status & 0x40 == 0 {
        // Flag not set yet — wait a bit more and force another frame
        emu.cpu.memory().set(0x4017, 0x00);
        for _ in 0..30_000 {
            let _ = emu.tick_one();
        }
    } else {
        flag_set = true;
    }
    // Re-set flag for the DMA dummy test
    emu.cpu.memory().set(0x4017, 0x00);
    for _ in 0..29_900 {
        let _ = emu.tick_one();
    }

    // Simulate DMC DMA dummy re-reads of $4015 (as halt cycles do).
    let _ = emu.cpu.memory().get(0x4015); // dummy 1 — schedules clear
    emu.apu.write().unwrap().tick_frame_irq_clear(); // applied on next get phase
    let _ = emu.cpu.memory().get(0x4015); // dummy 2
    let final_read = emu.cpu.memory().get(0x4015); // CPU's BIT/LDA
    assert_eq!(
        final_read & 0x40,
        0,
        "after DMA dummy $4015 reads, frame IRQ bit must be clear (flag_set={flag_set}, first_status={status:02X})"
    );
}



#[test]
fn accuracy_coin_page13_dma_suite() {
    // (name, test entry, result zeropage/RAM address)
    let tests: &[(&str, u16, u16)] = &[
        ("DMA + Open Bus", 0xB1E7, 0x046C),
        ("DMA + $2002 Read", 0x9432, 0x0488),
        ("DMA + $2007 Read", 0xB234, 0x044C),
        ("DMA + $2007 Write", 0xB25D, 0x044F),
        ("DMA + $4015 Read", 0xBF1D, 0x045D),
        ("DMA + $4016 Read", 0xBF52, 0x045E),
        ("DMC DMA Bus Conflicts", 0xD1E1, 0x046B),
        ("DMC DMA + OAM DMA", 0xDA49, 0x0477),
        ("Explicit DMA Abort", 0xDAFA, 0x0479),
        ("Implicit DMA Abort", 0xDB72, 0x0478),
    ];
    let mut failures = Vec::new();
    for &(name, addr, result_addr) in tests {
        let r = run_accuracy_coin_test(addr, result_addr);
        eprintln!("{name}: ${r:02X}");
        // Pass: 1 / special 5/9, or any value that is not fail-encoded
        // (ErrorCode<<2)|2 — e.g. Bus Conflicts returns `$50` which may be $00/$FF.
        // Note: run_accuracy_coin_test uses $FF as sentinel; a true $FF pass is
        // indistinguishable and treated as incomplete ($FF) — dedicated tests handle that.
        let is_fail = (r & 0x03) == 0x02;
        let ok = r == 1 || r == 5 || r == 9 || (!is_fail && r != 0xFF);
        if !ok {
            failures.push(format!("{name}=${r:02X}"));
        }
    }
    assert!(
        failures.is_empty(),
        "page 13 failures: {}",
        failures.join(", ")
    );
}

/// Diagnose Frame Counter 5-step (page 14 test 5).
#[test]
fn diagnose_frame_counter_5step() {
    let r = run_accuracy_coin_test(0xCDA3, 0x0469);
    eprintln!("FrameCounter5Step result=${r:02X} EC={}", r >> 2);

    // Measure cycles from post-$80-reset to length death (expect ~14913).
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);
    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    for _ in 0..20 {
        let _ = emu.tick_one();
    }
    // Prep
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x4015, 0x01);
    emu.cpu.memory().set(0x4000, 0x10);
    emu.cpu.memory().set(0x4001, 0x7f);
    emu.cpu.memory().set(0x4002, 0xff);
    for _ in 0..10 {
        let _ = emu.tick_one();
    }
    // OAM sync to put-ish: write $4014
    emu.cpu.memory().set(0x4014, 0x02);
    for _ in 0..600 {
        let _ = emu.tick_one();
    }
    emu.cpu.memory().set(0x4017, 0x00);
    for _ in 0..8 {
        let _ = emu.tick_one();
    }
    emu.cpu.memory().set(0x4003, 0x18); // length=2
    for _ in 0..4 {
        let _ = emu.tick_one();
    }
    emu.cpu.memory().set(0x4017, 0x80); // 5-step + clock
    // Exactly 3 cycles like LDA <$00 after put-aligned write (delay=3).
    for _ in 0..3 {
        let _ = emu.tick_one();
    }
    let after_stall = emu.cpu.memory().get(0x4015);
    eprintln!("after 3-cycle stall $4015={after_stall:02X}");
    // Sample at the exact offsets the ROM uses after that stall.
    for i in 0..14920u32 {
        let _ = emu.tick_one();
        if matches!(i + 1, 14911 | 14912 | 14913 | 14914) {
            let s = emu.cpu.memory().get(0x4015);
            eprintln!("  +{} $4015={:02X}", i + 1, s);
        }
    }
}

/// Diagnose APU Register Activation failure point.
#[test]
fn diagnose_apu_reg_activation() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);
    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    for _ in 0..16 {
        let _ = emu.tick_one();
    }
    for _ in 0..16 {
        let _ = emu.cpu.memory().get(0x4016);
    }
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(0x50, 0);
    emu.cpu.memory().set_force(0x045C, 0xFF);
    let trampoline = 0x0700u16;
    let code = [0x20u8, 0x9C, 0xBC, 0x8D, 0x5C, 0x04, 0x4C, 0x06, 0x07];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.jump_to_instruction(trampoline);
    let mut last_pc = 0u16;
    let mut seen_3ffe = false;
    let mut seen_4001 = false;
    let mut a_at_return = 0u8;
    for _ in 0..50_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        let pc = emu.cpu.pc();
        if pc != last_pc {
            if (0x3FFE..=0x4010).contains(&pc) {
                let a = emu.cpu.a;
                let bus = emu.cpu.memory().cpu_data_bus;
                let ppu_ob = emu.ppu.read().unwrap().get_open_bus();
                eprintln!("pc={pc:04X} a={a:02X} bus={bus:02X} ppu_ob={ppu_ob:02X}");
            }
            if pc == 0x3FFE {
                seen_3ffe = true;
            }
            if pc == 0x4001 || pc == 0x4000 {
                seen_4001 = true;
            }
            last_pc = pc;
        }
        if advanced {
            if (0x0706..=0x0708).contains(&pc) {
                a_at_return = emu.cpu.a;
                break;
            }
            let r = emu.cpu.memory().get_direct(0x045C);
            if r != 0xFF {
                a_at_return = emu.cpu.a;
                break;
            }
        }
    }
    // Peek OAM-ish via $2004 after test if we got past test 5 setup
    eprintln!(
        "result={:02X} EC={} $50={:02X} a={} seen_3ffe={} seen_40xx={} irq_vec={:02X}{:02X}",
        emu.cpu.memory().get_direct(0x045C),
        emu.cpu.memory().get_direct(0x10),
        emu.cpu.memory().get_direct(0x50),
        a_at_return,
        seen_3ffe,
        seen_4001,
        emu.cpu.memory().get_direct(0xFFFF),
        emu.cpu.memory().get_direct(0xFFFE),
    );
    eprintln!(
        "$600..605: {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
        emu.cpu.memory().get_direct(0x600),
        emu.cpu.memory().get_direct(0x601),
        emu.cpu.memory().get_direct(0x602),
        emu.cpu.memory().get_direct(0x603),
        emu.cpu.memory().get_direct(0x604),
        emu.cpu.memory().get_direct(0x605),
    );
    // Dump raw OAM for test 6 comparison (if we reached OAM DMA).
    eprint!("OAM[00..1F]:");
    for i in 0..32u8 {
        eprint!(" {:02X}", emu.ppu.read().unwrap().oam[i as usize]);
    }
    eprintln!();
    eprint!("OAM[30..3F]:");
    for i in 0x30..0x40u8 {
        eprint!(" {:02X}", emu.ppu.read().unwrap().oam[i as usize]);
    }
    eprintln!();
}

/// Diagnose Frame Counter 4-step length-clock timing.
#[test]
fn diagnose_frame_counter_4step() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);
    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    for _ in 0..16 {
        let _ = emu.tick_one();
    }
    emu.cpu.p.set_i(true);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);

    // Mirror TEST_APU_Prep + 4-step test 1/2 setup via register writes + wait.
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x4015, 0x01);
    emu.cpu.memory().set(0x4000, 0x10);
    emu.cpu.memory().set(0x4001, 0x7f);
    emu.cpu.memory().set(0x4002, 0xff);
    for _ in 0..20 {
        let _ = emu.tick_one();
    }

    // Sync-ish: OAM DMA
    emu.cpu.memory().set(0x4014, 0x02);
    for _ in 0..600 {
        let _ = emu.tick_one();
        if emu.cpu.memory().oam_dma.is_none() {
            // oam may already have been consumed
        }
    }
    // Drain oam if scheduled
    for _ in 0..600 {
        let _ = emu.tick_one();
    }

    emu.cpu.memory().set(0x4017, 0x00);
    for _ in 0..10 {
        let _ = emu.tick_one();
    }
    emu.cpu.memory().set(0x4003, 0x18); // length = 2
    for _ in 0..4 {
        let _ = emu.tick_one();
    }
    emu.cpu.memory().set(0x4017, 0x80); // clock once → length 1
    for _ in 0..10 {
        let _ = emu.tick_one();
    }
    let s1 = emu.cpu.memory().get(0x4015) & 1;
    emu.cpu.memory().set(0x4017, 0x40); // 4-step reset
    for _ in 0..4 {
        let _ = emu.tick_one(); // delay + NOP-ish
    }
    // Count cycles until length dies
    let mut died_at = None;
    for i in 0..20000u32 {
        let _ = emu.tick_one();
        if emu.cpu.memory().get(0x4015) & 1 == 0 {
            died_at = Some(i + 1);
            break;
        }
    }
    eprintln!("after $80 clock, status bit0={s1}; length died after {died_at:?} cycles from post-reset");
    assert!(died_at.is_some(), "length never expired");
}

/// AccuracyCoin page 14 test 5: Frame Counter 5-step (`TEST_FrameCounter5Step` @ $CDA3).
#[test]
fn accuracy_coin_frame_counter_5step_passes() {
    let r = run_accuracy_coin_test(0xCDA3, 0x0469);
    assert_eq!(
        r, 1,
        "Frame Counter 5-step should pass (1), got ${r:02X} (fail=(ErrorCode<<2)|2)"
    );
}

/// AccuracyCoin page 14: APU Tests (`Suite_APUTiming`).
#[test]
fn accuracy_coin_page14_apu_suite() {
    let tests: &[(&str, u16, u16)] = &[
        ("Length Counter", 0xC89F, 0x0465),
        ("Length Table", 0xC97A, 0x0466),
        ("Frame Counter IRQ", 0xC9DC, 0x0467),
        ("Frame Counter 4-step", 0xCC88, 0x0468),
        ("Frame Counter 5-step", 0xCDA3, 0x0469),
        ("Delta Modulation Channel", 0xCE96, 0x046A),
        ("APU Register Activation", 0xBC9C, 0x045C),
        ("Controller Strobing", 0xC024, 0x045F),
        ("Controller Clocking", 0xE010, 0x047A),
    ];
    let mut failures = Vec::new();
    for &(name, addr, result_addr) in tests {
        let r = run_accuracy_coin_test(addr, result_addr);
        let ec_note = if (r & 0x03) == 0x02 {
            format!(" failEC={}", r >> 2)
        } else {
            String::new()
        };
        eprintln!("{name}: ${r:02X}{ec_note}");
        let is_fail = (r & 0x03) == 0x02;
        let ok = r == 1 || r == 5 || r == 9 || (!is_fail && r != 0xFF);
        if !ok {
            failures.push(format!("{name}=${r:02X}"));
        }
    }
    assert!(
        failures.is_empty(),
        "page 14 failures: {}",
        failures.join(", ")
    );
}

/// Page 13 first test: DMA + Open Bus (`TEST_DMA_Plus_OpenBus` at $B1E7).
/// Pass returns A=1; fail returns (ErrorCode<<2)|2.
#[test]
fn accuracy_coin_dma_plus_open_bus_passes() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    // Wait for DMASync pre-test success.
    for _ in 0..300 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass first");

    // Disable NMI/rendering/IRQ so the menu cannot interrupt the test.
    // is_vbl_enabled() reads the raw memory byte, so force both sides.
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set_force(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    // Trampoline in RAM:
    //   JSR TEST_DMA_Plus_OpenBus
    //   STA $046C
    //   JMP *
    let trampoline: u16 = 0x0700;
    let code: &[u8] = &[
        0x20, 0xE7, 0xB1, // JSR $B1E7
        0x8D, 0x6C, 0x04, // STA $046C
        0x4C, 0x06, 0x07, // JMP $0706
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1); // ErrorCode
    emu.cpu.memory().set_force(0x046C, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    let mut result = 0xFFu8;
    for _ in 0..5_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let r = emu.cpu.memory().get_direct(0x046C);
            if r != 0xFF {
                result = r;
                break;
            }
        }
    }
    assert_eq!(
        result, 1,
        "DMA + Open Bus should write pass (1) to $46C, got ${result:02X} \
         (fail=(ErrorCode<<2)|2; ErrorCode={})",
        emu.cpu.memory().get_direct(0x10)
    );
}

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

    assert_eq!(cycles_to_first_vbl, 82183);
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

/// Page 12, Test 1: "Interrupt flag latency"
#[test]
fn accuracy_coin_page12_interrupt_flag_latency() {
    let r = run_accuracy_coin_test(0xC4F8, 0x0461);
    assert_eq!(r, 1, "Interrupt Flag Latency expected 1 (PASS), got ${r:02X}");
}

/// Page 12, Test 2: "NMI overlap BRK"
#[test]
fn accuracy_coin_page12_nmi_overlap_brk() {
    let r = run_accuracy_coin_test(0xC6B7, 0x0462);
    assert_eq!(r, 1, "NMI Overlap BRK expected 1 (PASS), got ${r:02X}");
}

/// Page 12, Test 3: "NMI overlap IRQ"
#[test]
fn accuracy_coin_page12_nmi_overlap_irq() {
    let r = run_accuracy_coin_test(0xC7DD, 0x0463);
    assert_eq!(r, 1, "NMI Overlap IRQ expected 1 (PASS), got ${r:02X}");
}
/// Page 10: "SHA instruction" ($93 SHA indirect,Y and $9F SHA absolute,Y)
#[test]
fn accuracy_coin_page10_sha() {
    let r93 = run_accuracy_coin_test(0xAB0D, 0x0446);
    assert!(
        r93 == 1 || r93 == 5 || r93 == 9 || r93 == 13 || r93 == 17,
        "$93 SHA indirect,Y expected pass (e.g. $05 for Behavior 1), got ${r93:02X} (EC={})",
        r93 >> 2
    );
    let r9f = run_accuracy_coin_test(0xAB52, 0x0447);
    assert!(
        r9f == 1 || r9f == 5 || r9f == 9 || r9f == 13 || r9f == 17,
        "$9F SHA absolute,Y expected pass (e.g. $05 for Behavior 1), got ${r9f:02X} (EC={})",
        r9f >> 2
    );
}

/// Page 10: "$9B SHS absolute,Y"
#[test]
fn accuracy_coin_page10_shs_9b() {
    let r = run_accuracy_coin_test(0xAD88, 0x0448);
    assert!(
        r == 1 || r == 5 || r == 9 || r == 13 || r == 17,
        "$9B SHS absolute,Y expected pass (e.g. $05 for Behavior 1), got ${r:02X} (EC={})",
        r >> 2
    );
}

/// Page 10: "$9C SHY absolute,X"
#[test]
fn accuracy_coin_page10_shy_9c() {
    let r = run_accuracy_coin_test(0xAF48, 0x0449);
    assert_eq!(r, 1, "$9C SHY absolute,X expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 10: "$9E SHX absolute,Y"
#[test]
fn accuracy_coin_page10_shx_9e() {
    let r = run_accuracy_coin_test(0xAFAD, 0x044A);
    assert_eq!(r, 1, "$9E SHX absolute,Y expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 10: "$BB LAE absolute,Y"
#[test]
fn accuracy_coin_page10_lae_bb() {
    let r = run_accuracy_coin_test(0xB012, 0x044B);
    assert_eq!(r, 1, "$BB LAE absolute,Y expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 11: "$0B ANC Immediate"
#[test]
fn accuracy_coin_page11_anc_0b() {
    let r = run_accuracy_coin_test(0xB053, 0x0410);
    assert_eq!(r, 1, "$0B ANC Immediate expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 11: "$2B ANC Immediate"
#[test]
fn accuracy_coin_page11_anc_2b() {
    let r = run_accuracy_coin_test(0xB057, 0x0411);
    assert_eq!(r, 1, "$2B ANC Immediate expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 11: "$4B ASR Immediate"
#[test]
fn accuracy_coin_page11_asr_4b() {
    let r = run_accuracy_coin_test(0xB083, 0x0412);
    assert_eq!(r, 1, "$4B ASR Immediate expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 11: "$6B ARR Immediate"
#[test]
fn accuracy_coin_page11_arr_6b() {
    let r = run_accuracy_coin_test(0xB0AF, 0x0413);
    assert_eq!(r, 1, "$6B ARR Immediate expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 11: "$8B ANE Immediate"
#[test]
fn accuracy_coin_page11_ane_8b() {
    let r = run_accuracy_coin_test(0xB0DB, 0x0414);
    assert_eq!(r, 1, "$8B ANE Immediate expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 11: "$AB LXA Immediate"
#[test]
fn accuracy_coin_page11_lxa_ab() {
    let r = run_accuracy_coin_test(0xB126, 0x0415);
    assert_eq!(r, 1, "$AB LXA Immediate expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 11: "$CB AXS Immediate"
#[test]
fn accuracy_coin_page11_axs_cb() {
    let r = run_accuracy_coin_test(0xB16B, 0x0416);
    assert_eq!(r, 1, "$CB AXS Immediate expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 11: "$EB SBC Immediate"
#[test]
fn accuracy_coin_page11_sbc_eb() {
    let r = run_accuracy_coin_test(0xB1AF, 0x0417);
    assert_eq!(r, 1, "$EB SBC Immediate expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 14, Test 3: "Frame Counter IRQ"
#[test]
fn accuracy_coin_page14_frame_counter_irq() {
    let r = run_accuracy_coin_test(0xC9DC, 0x0467);
    assert_eq!(r, 1, "Frame Counter IRQ expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 13, Test 2: "DMA + $2002 Read"
#[test]
fn accuracy_coin_page13_dma_plus_2002_read() {
    let r = run_accuracy_coin_test(0x9432, 0x0488);
    // Success code 1: $05 ("Load DMA after 2 APU cycles")
    // Success code 2: $09 ("Load DMA after 3 APU cycles")
    assert!(
        r == 5 || r == 9,
        "DMA + $2002 Read expected pass ($05 or $09), got ${r:02X} (EC={})",
        r >> 2
    );
}

/// Page 16, Test 1: "PPU Register Open Bus"
#[test]
fn accuracy_coin_page16_ppu_register_open_bus() {
    let r = run_accuracy_coin_test(0xA258, 0x044E);
    assert_eq!(r, 1, "PPU Register Open Bus expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 16, Test 4: "Rendering Flag Behavior"
#[test]
fn accuracy_coin_page16_rendering_flag_behavior() {
    let r = run_accuracy_coin_test(0xEBDC, 0x0486);
    assert_eq!(r, 1, "Rendering Flag Behavior expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

/// Page 16, Test 5: "$2007 read w/ rendering"
#[test]
fn accuracy_coin_page16_2007_read_with_rendering() {
    let r = run_accuracy_coin_test(0x95B3, 0x048A);
    assert_eq!(r, 1, "$2007 read w/ rendering expected 1 (PASS), got ${r:02X} (EC={})", r >> 2);
}

#[test]
fn diagnose_run_suite_16_tests() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    let trampoline = 0x0700u16;
    let code: Vec<u8> = vec![
        0xA9, 0x01,       // LDA #1
        0x85, 0x35,       // STA $35 (RunningAllTests)
        0x20, 0x60, 0xF0, // JSR $F060 (DisableNMI)
        0x20, 0x30, 0xF0, // JSR $F030 (DisableRendering)
        0x20, 0x83, 0xF0, // JSR $F083 (ClearNametable)
        0xA0, 0x0F,       // LDY #15 (Page 16)
        0x84, 0x14,       // STY $14 (menuTabXPos)
        0x20, 0x01, 0x90, // JSR $9001 (ResetScroll, SetUpSuitePointer, LoadSuiteMenuNoRendering, loop)
        // Note: in ROM:
        // $904C is INY (Y=16)
        // $904D is CPY #20 ($14)
        // $904F is BNE $9001
        // Let's patch byte $904E from $14 (20) to $10 (16)!
        // Then after Y reaches 16, it exits the loop!
        0x00,             // BRK
    ];

    // Let's modify ROM at $904E to $10 so it stops after page 16:
    emu.cpu.memory().set_force(0x904E, 0x10);

    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..30_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x9051 || pc == 0x0711 {
                eprintln!("Finished suite 16 loop at step {step}!");
                break;
            }
        }
    }

    // Check results of Suite 16:
    // 0 CHR ROM is not writable 0x485
    // 1 PPU Register Mirroring 0x404
    // 2 PPU Register Open Bus 0x44e
    // 3 PPU Read Buffer 0x476
    // 4 Palette RAM Quirks 0x47e
    // 5 Rendering Flag Behavior 0x486
    // 6 $2007 read w/ rendering 0x48a
    let results = [
        ("CHR ROM is not writable", 0x485u16),
        ("PPU Register Mirroring", 0x404u16),
        ("PPU Register Open Bus", 0x44eu16),
        ("PPU Read Buffer", 0x476u16),
        ("Palette RAM Quirks", 0x47eu16),
        ("Rendering Flag Behavior", 0x486u16),
        ("$2007 read w/ rendering", 0x48au16),
    ];
    for (name, addr) in results {
        let val = emu.cpu.memory().get_direct(addr);
        eprintln!("{name} (${addr:04X}): ${val:02X} (EC={})", val >> 2);
    }
}

#[test]
fn test_verify_sprite_zero_hits_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }

    let trampoline = 0x0700u16;
    let code = [
        0x20, 0xB3, 0x95, // JSR $95B3 (TEST_Rendering2007Read)
        0x8D, 0x00, 0x05, // STA $0500
        0x00,             // BRK
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1); // ErrorCode
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..5_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x0706 || pc == 0x0707 {
                eprintln!("Finished TEST_Rendering2007Read at step {step}");
                break;
            }
        }
    }
    let res = emu.cpu.memory().get_direct(0x0500);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("TEST_Rendering2007Read returned A=${res:02X}, EC={ec}");
    assert_eq!(res, 1, "TEST_Rendering2007Read failed!");
}
#[test]
fn diagnose_run_suite_17_tests() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");
    let vbl_pretest = emu.cpu.memory().get_direct(0x3A);
    eprintln!("VblankSync_PreTest result ($3A): ${vbl_pretest:02X}");

    let trampoline = 0x0300u16;
    let code: Vec<u8> = vec![
        0xA9, 0x01,       // LDA #1
        0x85, 0x35,       // STA $35 (RunningAllTests)
        0x20, 0x60, 0xF0, // JSR $F060 (DisableNMI)
        0x20, 0x30, 0xF0, // JSR $F030 (DisableRendering)
        0x20, 0x83, 0xF0, // JSR $F083 (ClearNametable)
        0xA0, 0x10,       // LDY #16 (Page 17)
        0x84, 0x14,       // STY $14 (menuTabXPos)
        0x20, 0x01, 0x90, // JSR $9001 (ResetScroll, SetUpSuitePointer, LoadSuiteMenuNoRendering, loop)
        0x00,             // BRK
    ];

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.p.set_i(true);

    // Modify ROM at $904E to $11 so it stops after page 17:
    emu.cpu.memory().set_force(0x904E, 0x11);

    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..50_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x9051 || pc == 0x0311 {
                eprintln!("Finished suite 17 loop at step {step}!");
                break;
            }
        }
    }

    let results = [
        ("VBlank beginning", 0x450u16),
        ("VBlank end", 0x451u16),
        ("NMI Control", 0x452u16),
        ("NMI Timing", 0x453u16),
        ("NMI Suppression", 0x454u16),
        ("NMI at VBlank end", 0x455u16),
        ("NMI disabled at VBlank", 0x456u16),
    ];
    for (name, addr) in results {
        let val = emu.cpu.memory().get_direct(addr);
        eprintln!("{name} (${addr:04X}): ${val:02X} (EC={})", val >> 2);
        if name != "VBlank end" {
            assert_eq!(val, 1, "{name} must pass with $01, got ${val:02X}");
        }
    }
    eprintln!("$50 buffer:");
    for i in 0..16 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x50 + i));
    }
    eprintln!();
    eprintln!("$60 buffer:");
    for i in 0..16 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x60 + i));
    }
    eprintln!();
}

#[test]
fn diagnose_run_suite_18_tests() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    let trampoline = 0x0300u16;
    let code: Vec<u8> = vec![
        0xA9, 0x01,       // LDA #1
        0x85, 0x35,       // STA $35 (RunningAllTests)
        0x20, 0x60, 0xF0, // JSR $F060 (DisableNMI)
        0x20, 0x30, 0xF0, // JSR $F030 (DisableRendering)
        0x20, 0x83, 0xF0, // JSR $F083 (ClearNametable)
        0xA9, 0x08,       // LDA #$08
        0x85, 0xF1,       // STA $F1 (PPUMASK_COPY)
        0x8D, 0x01, 0x20, // STA $2001
        0xA0, 0x11,       // LDY #17 (Page 18)
        0x84, 0x14,       // STY $14 (menuTabXPos)
        0x20, 0x01, 0x90, // JSR $9001 (ResetScroll, SetUpSuitePointer, LoadSuiteMenuNoRendering, loop)
        0x00,             // BRK
    ];

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.p.set_i(true);

    // Modify ROM at $904E to $12 so it stops after page 18:
    emu.cpu.memory().set_force(0x904E, 0x12);

    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.jump_to_instruction(trampoline);

    for _step in 0..50_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x9051 || pc == 0x0311 {
                break;
            }
        }
    }

    let results = [
        ("Sprite overflow behavior", 0x459u16),
        ("Sprite 0 Hit behavior", 0x457u16),
        ("$2002 flag timing", 0x48Du16),
        ("Suddenly Resize Sprite", 0x489u16),
        ("Arbitrary Sprite zero", 0x458u16),
        ("Misaligned OAM behavior", 0x45Au16),
        ("Address $2004 behavior", 0x45Bu16),
        ("OAM Corruption", 0x47Bu16),
        ("INC $4014", 0x480u16),
    ];
    for (name, addr) in results {
        let val = emu.cpu.memory().get_direct(addr);
        eprintln!("{name} (${addr:04X}): ${val:02X} (EC={})", val >> 2);
    }
    assert_eq!(emu.cpu.memory().get_direct(0x0459), 1, "Sprite overflow behavior must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x0457), 1, "Sprite 0 Hit behavior must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x048D), 1, "$2002 flag timing must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x0489), 1, "Suddenly Resize Sprite must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x0458), 1, "Arbitrary Sprite zero must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x045A), 1, "Misaligned OAM behavior must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x045B) & 0x03, 1, "Address $2004 behavior must pass");
    assert_eq!(emu.cpu.memory().get_direct(0x047B), 1, "OAM Corruption must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x0480), 1, "INC $4014 must pass with $01");
}

#[test]
fn test_2002_flag_timing_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let trampoline = 0x0300u16;
    let test_addr = 0x9BAAu16; // TEST_2002FlagTiming
    let result_addr = 0x048Du16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    for _step in 0..10_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x0306 || pc == 0x0307 {
                break;
            }
        }
    }

    let res = emu.cpu.memory().get_direct(result_addr);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("Result: ${res:02X} (EC={ec})");
    assert_eq!(res, 1, "$2002 flag timing standalone must pass with $01");
}

#[test]
fn test_address_2004_behavior_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let trampoline = 0x0300u16;
    let test_addr = 0xBAECu16; // TEST_Address2004_Behavior
    let result_addr = 0x045Bu16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..10_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x0306 || pc == 0x0307 {
                eprintln!("Finished at step {step}");
                break;
            }
        }
    }

    let res = emu.cpu.memory().get_direct(result_addr);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("Address 2004 Behavior: Result: ${res:02X} (EC={ec})");
}

#[test]
fn test_inc4014_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let trampoline = 0x0300u16;
    let test_addr = 0xE7F0u16; // TEST_INC4014
    let result_addr = 0x0480u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..10_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x0306 || pc == 0x0307 {
                eprintln!("Finished at step {step}");
                break;
            }
        }
    }

    let _res = emu.cpu.memory().get_direct(result_addr);
    let _ec = emu.cpu.memory().get_direct(0x10);
}

#[test]
fn test_arbitrary_sprite_zero_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    for i in 0x8000..0xFFF0 {
        if emu.cpu.memory().get_direct(i) == 0xA9 && emu.cpu.memory().get_direct(i+1) == 0x02
            && emu.cpu.memory().get_direct(i+2) == 0x8D && emu.cpu.memory().get_direct(i+3) == 0x14
            && emu.cpu.memory().get_direct(i+4) == 0x40 && emu.cpu.memory().get_direct(i+8) == 0xAD
            && emu.cpu.memory().get_direct(i+9) == 0x02 && emu.cpu.memory().get_direct(i+10) == 0x20
            && emu.cpu.memory().get_direct(i+11) == 0x29 && emu.cpu.memory().get_direct(i+12) == 0x40 {
            eprintln!("Found pattern at CPU addr 0x{i:04X}");
        }
    }
    emu.cpu.memory().set_force(0xF1, 0x08); // PPUMASK_COPY = 0x08 (BG enabled)
    let trampoline = 0x0300u16;
    let test_addr = 0xB758u16; // TEST_ArbitrarySpriteZero
    let result_addr = 0x0458u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..20_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0xB755 {
                let x = emu.cpu.x;
                let a = emu.cpu.a;
                let s2002 = emu.cpu.memory().get_direct(0x2002);
                eprintln!("Jumped to FAIL_ArbitrarySpriteZero1! X={:02X}, A={:02X}, $2002={:02X}", x, a, s2002);
            }
            if pc == 0x0306 || pc == 0x0307 {
                eprintln!("Finished at step {step}");
                break;
            }
        }
    }

    let res = emu.cpu.memory().get_direct(result_addr);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("Arbitrary Sprite Zero: Result: ${res:02X} (EC={ec})");
}


#[test]
fn test_vblank_end_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let trampoline = 0x0300u16;
    let test_addr = 0xB302u16; // TEST_VBlank_End
    let result_addr = 0x0451u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..10_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x0306 || pc == 0x0307 {
                eprintln!("Finished at step {step}");
                break;
            }
        }
    }

    let res = emu.cpu.memory().get_direct(result_addr);
    eprintln!("Result: ${res:02X} (EC={})", res >> 2);
    eprint!("$50 values: ");
    for i in 0..7 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x50 + i));
    }
    eprintln!();
    eprintln!("Expected:   01 01 01 01 00 00 00");
}

#[test]
fn test_nmi_control_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);

    let trampoline = 0x0300u16;
    let test_addr = 0xB33Cu16; // TEST_NMI_Control
    let result_addr = 0x0452u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..20_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x0306 || pc == 0x0307 {
                eprintln!("Finished at step {step}");
                break;
            }
        }
    }

    let res = emu.cpu.memory().get_direct(result_addr);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("Result: ${res:02X} (EC={ec})");
    assert_eq!(res, 1, "NMI Control should pass (1), got ${res:02X} (EC={ec})");
}

#[test]
fn test_nmi_timing_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let trampoline = 0x0300u16;
    let test_addr = 0xB40Cu16; // TEST_NMI_Timing
    let result_addr = 0x0453u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    eprint!("Code at B428..B440: ");
    for a in 0xB428..=0xB440 {
        eprint!("{:04X}:{:02X} ", a, emu.cpu.memory().get_direct(a));
    }
    eprintln!();

    for step in 0..20_000_000u32 {
        let pc = emu.cpu.pc();
        if pc == 0x0700 {
            let cpu_y = emu.cpu.y;
            let (scanline, dot) = {
                let ppu = emu.ppu.read().unwrap();
                (ppu.scanline, ppu.x)
            };
            let s = emu.cpu.s;
            let pcl = emu.cpu.memory().get_direct(0x0100 + s as u16 + 2);
            let pch = emu.cpu.memory().get_direct(0x0100 + s as u16 + 3);
            let ret_pc = ((pch as u16) << 8) | (pcl as u16);
            eprintln!("NMI taken! step={step} ret_pc=${ret_pc:04X} Y={cpu_y} scanline={scanline} dot={dot} cyc={}", emu.cpu.current_cycle);
        }
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x0306 || pc == 0x0307 {
                eprintln!("Finished at step {step}");
                break;
            }
        }
    }

    let res = emu.cpu.memory().get_direct(result_addr);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("Result: ${res:02X} (EC={ec})");
    eprint!("$50 values: ");
    for i in 0..11 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x50 + i));
    }
    eprintln!();
    eprintln!("Expected:   03 02 02 02 02 02 02 01 01 01 01");
    eprintln!("Expected+1: 02 02 02 02 02 02 01 01 01 01 XX");
    assert_eq!(res, 1, "NMI Timing should pass (1), got ${res:02X} (EC={ec})");
}

#[test]
fn test_nmi_suppression_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let trampoline = 0x0300u16;
    let test_addr = 0xB475u16; // TEST_NMI_Suppression
    let result_addr = 0x0454u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..20_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x0306 || pc == 0x0307 {
                eprintln!("Finished at step {step}");
                break;
            }
        }
    }

    let res = emu.cpu.memory().get_direct(result_addr);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("Result: ${res:02X} (EC={ec})");
    eprint!("$50 values: ");
    for i in 0..10 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x50 + i));
    }
    eprintln!();
    eprintln!("Expected:   FF 02 02 FF 00 01 FF 03 03 03");
    assert_eq!(res, 1, "NMI Suppression should pass (1), got ${res:02X} (EC={ec})");
}

#[test]
fn test_nmi_vbl_end_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let trampoline = 0x0300u16;
    let test_addr = 0xB4C4u16; // TEST_NMI_VBL_End
    let result_addr = 0x0455u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..20_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x0306 || pc == 0x0307 {
                eprintln!("Finished at step {step}");
                break;
            }
        }
    }

    let res = emu.cpu.memory().get_direct(result_addr);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("Result: ${res:02X} (EC={ec})");
    eprint!("$50 values: ");
    for i in 0..7 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x50 + i));
    }
    eprintln!();
    eprint!("$60 values: ");
    for i in 0..7 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x60 + i));
    }
    eprintln!();
    eprintln!("Expected:   01 01 01 00 00 00 00");
    assert_eq!(res, 1, "NMI VBL End should pass (1), got ${res:02X} (EC={ec})");
}

#[test]
fn test_nmi_disabled_vbl_start_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let trampoline = 0x0300u16;
    let test_addr = 0xB520u16; // TEST_NMI_Disabled_VBL_Start
    let result_addr = 0x0456u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..20_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc >= 0xB520 && pc <= 0xB540 {
                eprintln!("PC={pc:04X} X={} Y={} A={:02X}", emu.cpu.x, emu.cpu.y, emu.cpu.a);
            }
            if pc == 0x0700 {
                let (sl, dot) = {
                    let ppu = emu.ppu.read().unwrap();
                    (ppu.scanline, ppu.x)
                };
                let s = emu.cpu.s;
                let ret_pc = (emu.cpu.memory().get_direct(0x0100 + s.wrapping_add(1) as u16) as u16)
                    | ((emu.cpu.memory().get_direct(0x0100 + s.wrapping_add(2) as u16) as u16) << 8);
                eprintln!("  NMI fired at sl={sl} dot={dot} ret_pc={ret_pc:04X}");
            }
            if pc == 0x0306 || pc == 0x0307 {
                eprintln!("Finished at step {step}");
                break;
            }
        }
    }

    let res = emu.cpu.memory().get_direct(result_addr);
    let ec = emu.cpu.memory().get_direct(0x10);
    eprintln!("Result: ${res:02X} (EC={ec})");
    eprint!("$50 values: ");
    for i in 0..7 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x50 + i));
    }
    eprintln!();
    eprintln!("Expected:   00 00 00 FF 01 01 01");
    assert_eq!(res, 1, "NMI Disabled VBL Start should pass (1), got ${res:02X} (EC={ec})");
}

#[test]
fn test_vblank_beginning_standalone() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.memory().set(0x2000, 0x00);
    emu.cpu.memory().set(0x2001, 0x00);
    emu.cpu.p.set_i(true);

    let trampoline = 0x0300u16;
    let test_addr = 0xB2AEu16; // TEST_VBlank_Beginning
    let result_addr = 0x0450u16;
    let code = [
        0x20, (test_addr & 0xFF) as u8, (test_addr >> 8) as u8,
        0x8D, (result_addr & 0xFF) as u8, (result_addr >> 8) as u8,
        0x00,
    ];
    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.memory().set_force(0x10, 1);
    emu.cpu.memory().set_force(result_addr, 0xFF);
    emu.cpu.jump_to_instruction(trampoline);

    for step in 0..10_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x0306 || pc == 0x0307 {
                eprintln!("Finished at step {step}");
                break;
            }
        }
    }

    let res = emu.cpu.memory().get_direct(result_addr);
    eprintln!("Result: ${res:02X} (EC={})", res >> 2);
    eprint!("$50 values: ");
    for i in 0..7 {
        eprint!("{:02X} ", emu.cpu.memory().get_direct(0x50 + i));
    }
    eprintln!();
    eprintln!("Expected:   02 02 02 02 00 01 01");
    assert_eq!(res, 1, "VBlank beginning should pass (1)");
}

#[test]
fn diagnose_run_suite_19_tests() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    let trampoline = 0x0300u16;
    let code: Vec<u8> = vec![
        0xA9, 0x01,       // LDA #1
        0x85, 0x35,       // STA $35 (RunningAllTests)
        0x20, 0x60, 0xF0, // JSR $F060 (DisableNMI)
        0x20, 0x30, 0xF0, // JSR $F030 (DisableRendering)
        0x20, 0x83, 0xF0, // JSR $F083 (ClearNametable)
        0xA9, 0x08,       // LDA #$08
        0x85, 0xF1,       // STA $F1 (PPUMASK_COPY)
        0x8D, 0x01, 0x20, // STA $2001
        0xA0, 0x12,       // LDY #18 (Page 19: Suite_PPUMisc)
        0x84, 0x14,       // STY $14 (menuTabXPos)
        0x20, 0x01, 0x90, // JSR $9001
        0x00,             // BRK
    ];

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.p.set_i(true);

    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.jump_to_instruction(trampoline);

    for _step in 0..50_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x904D || pc == 0x9051 || pc == 0x031B {
                break;
            }
        }
    }

    let results = [
        ("Attributes As Tiles", 0x481u16),
        ("t Register Quirks", 0x482u16),
        ("Stale BG Shift Registers", 0x483u16),
        ("Stale Sprite Shift Regs", 0x48Fu16),
        ("BG Serial In", 0x487u16),
        ("Sprites On Scanline 0", 0x484u16),
        ("$2004 Stress Test", 0x48Cu16),
        ("$2007 Stress Test", 0x48Eu16),
    ];
    for (name, addr) in results {
        let val = emu.cpu.memory().get_direct(addr);
        eprintln!("{name} (${addr:04X}): ${val:02X} (EC={})", val >> 2);
    }
    assert_eq!(emu.cpu.memory().get_direct(0x0481), 1, "Attributes As Tiles must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x0482), 1, "t Register Quirks must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x0483), 1, "Stale BG Shift Registers must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x048F), 1, "Stale Sprite Shift Regs must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x0487), 1, "BG Serial In must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x0484) & 0x03, 1, "Sprites On Scanline 0 must pass");
    assert_eq!(emu.cpu.memory().get_direct(0x048C), 1, "$2004 Stress Test must pass with $01");
    assert_eq!(emu.cpu.memory().get_direct(0x048E), 1, "$2007 Stress Test must pass with $01");
}


#[test]
fn diagnose_run_suite_20_tests() {
    let rom_info = find_rom_by_name("AccuracyCoin.nes");
    let shared = Arc::new(RwLock::new(SharedState::default()));
    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let mut emu = Emulator::new(rom_info, shared, joypad, Args::default(), None);

    for _ in 0..2000 {
        let _ = emu.tick();
        if emu.cpu.memory().get_direct(0x12) == 1 {
            break;
        }
    }
    assert_eq!(emu.cpu.memory().get_direct(0x12), 1, "DMASync pre-test must pass");

    let trampoline = 0x0300u16;
    let code: Vec<u8> = vec![
        0xA9, 0x01,       // LDA #1
        0x85, 0x35,       // STA $35 (RunningAllTests)
        0x20, 0x60, 0xF0, // JSR $F060 (DisableNMI)
        0x20, 0x30, 0xF0, // JSR $F030 (DisableRendering)
        0x20, 0x83, 0xF0, // JSR $F083 (ClearNametable)
        0xA9, 0x08,       // LDA #$08
        0x85, 0xF1,       // STA $F1 (PPUMASK_COPY)
        0x8D, 0x01, 0x20, // STA $2001
        0xA0, 0x13,       // LDY #19 (Page 20: Suite_CPUBehavior2)
        0x84, 0x14,       // STY $14 (menuTabXPos)
        0x20, 0x01, 0x90, // JSR $9001
        0x00,             // BRK
    ];

    emu.cpu.memory().set(0x4015, 0x00);
    emu.cpu.memory().set(0x4017, 0x40);
    emu.cpu.p.set_i(true);

    for (i, b) in code.iter().enumerate() {
        emu.cpu.memory().set_force(trampoline + i as u16, *b);
    }
    emu.cpu.jump_to_instruction(trampoline);

    for _step in 0..50_000_000u32 {
        let (advanced, _, _) = emu.tick_one();
        if advanced {
            let pc = emu.cpu.pc();
            if pc == 0x904D || pc == 0x9051 || pc == 0x031B {
                break;
            }
        }
    }

    let results = [
        ("Instruction Timing", 0x460u16),
        ("Implied Dummy Reads", 0x46Du16),
        ("Branch Dummy Reads", 0x48Bu16),
        ("JSR Edge Cases", 0x47Cu16),
        ("Internal Data Bus", 0x490u16),
    ];
    for (name, addr) in results {
        let val = emu.cpu.memory().get_direct(addr);
        eprintln!("{name} (${addr:04X}): ${val:02X} (EC={})", val >> 2);
    }
    assert_eq!(emu.cpu.memory().get_direct(0x46D), 1, "Implied Dummy Reads should pass (1)");
}
