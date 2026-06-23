<h2>
<p align="center">CedNES</p>
</h2>
<h3>
<p align="center">A NES emulator written in Rust</p>
</h3>

<img width="1259" height="525" alt="image" src="https://github.com/user-attachments/assets/3571356f-49d7-4fc3-aef3-202667d46dab" />


<p align="center">
<img src="https://github.com/user-attachments/assets/8f71231c-8a7c-47ca-9d4b-ffb669c83ecb" width=40% />
<img src="https://github.com/user-attachments/assets/886c56e9-f232-4785-914c-7ecfad39a92d" width=40% />
<img src="https://github.com/user-attachments/assets/92282c1d-8f0e-4e6e-b96b-6ec2a4980c6b" width=40% />
<img src="https://github.com/user-attachments/assets/f4b7df2c-64ac-42d9-91bf-e697707bd5e6" width=40% />
</p>

# Features

- Full sound (triangle, pulse 1, pulse 2, noise, DMC), with visualization
- Controller

# Mappers supported

- MMC1 (1)
- UxROM (2)
- CNRom (3)
- MMC3 (4)
- AxRom (7)
- MMC2 (9)
- Mapper 19 (19)
- GxRom (66)

# Launching

```
cargo run -r -- -r <rom>.nes
```

# CPU implementation

[`cpu2.rs`](nes/src/cpu2.rs) implements a cycle-accurate 6502 CPU emulator. Unlike many high-level emulators that
execute a full instruction in a single step, `cpu2.rs` is memory-cycled. This means the CPU's state machine progresses one
clock cycle at a time, and every single cycle performs exactly one memory operation—either a read or a write.

## Core Design: Memory-Cycled Execution

The fundamental unit of execution in `cpu2.rs` is the `tick()` method. Each call to `tick()` represents one clock cycle
of the 6502.

- Single Memory Access per Cycle: In alignment with the real 6502 hardware, every cycle is characterized by a memory
access. Even cycles that appear to be "internal" to the CPU on paper actually perform a read (often a redundant read of
the next opcode or a stack byte) which is discarded.
- The CPU maintains its internal state (registers, current opcode, current cycle within that opcode)
across `tick()` calls. This allows it to be perfectly synchronized with other hardware components like the PPU or APU.
- An instruction is considered finished when the `finished` flag is set during a `tick()`. The next
call to `tick()` will then fetch the next opcode.

## Key Components
- `Cpu2<T>`: Holds the CPU registers (`A`, `X`, `Y`, `S`, `PC`, `P`) and the execution state (`current_opcode`, `current_cycle`,
`finished`).
- `tick(&mut self, config: &Config) -> u8`: The primary entry point for advancing the CPU by one clock cycle. It uses a
large match statement on the `current_opcode` and a nested match on `current_cycle` to determine the specific action for the
current cycle.
- `run_one_instruction(&mut self, config: &Config) -> u8`: A helper method that calls `tick()` repeatedly until the current
instruction is fully executed, returning the total number of cycles consumed.

## Validation: Single Step Tests

The accuracy of the cycle-by-cycle implementation is verified using the 6502 Single Step tests (commonly referred to as
the "Harte" tests in this codebase).

- Verification Scope: These tests ensure that for every opcode, the CPU performs the exact sequence of reads and writes to
the correct addresses with the correct values, cycle by cycle.
- Status: `cpu2.rs` successfully passes these comprehensive Single Step tests, confirming its behavior matches real 6502
hardware at the bus level.

## Usage in System

In the larger emulator context, `cpu2.rs` is used when high precision and bus-level accuracy are required. It can be
stepped cycle-by-cycle alongside the APU and PPU to ensure perfect timing synchronization, which is critical for many
NES games that rely on precise mid-scanline timing or specific APU behavior.

# PPU implementation

[`ppu2.rs`](nes/src/ppu2.rs) implements the NES Picture Processing Unit (PPU) by simulating its internal logic as closely as possible to the official hardware diagrams (such as the one found on [NesDev](https://www.nesdev.org/w/images/default/4/4f/Ppu.svg)). Unlike high-level renderers that work scanline-by-scanline, `ppu2.rs` operates at the "dot" (pixel clock) level.

## 1. Event-Driven Architecture
The core of the implementation is a large pre-calculated array of events which is computed in [this function](https://github.com/cbeust/ced-nes/blob/main/nes/src/ppu2.rs#L513):
- **Event Array:** An array of `261 * 340` (the dimensions of a NTSC NES frame) elements is created during initialization via `init_events()`.
- **Dot-by-Dot Execution:** Every time the PPU `tick()` function is called, it lookups the event(s) associated with the current dot (`x` and `scanline`).
- **Bitmask Events:** Each entry in the array is a bitmask of actions to perform, such as:
    - `NT` / `AT`: Fetch Name Table or Attribute Table byte.
    - `BG_LSBITS` / `BG_MSBITS`: Fetch Pattern Table (tile) bits.
    - `INC_HORIZ_V` / `INC_VERT_V`: Increment the internal scroll registers (`v` and `t`).
    - `SPRITE_EVALUATION`: Check which sprites belong on the next scanline.

## 2. Hardware-Accurate Shifters
The PPU uses 16-bit shift registers to handle smooth scrolling and pixel output, which `ppu2.rs` replicates exactly:
- **Pattern Shifters:** `pattern_shift_low` and `pattern_shift_high` hold the 2 bits of color data for the next 16 pixels.
- **Attribute Shifters:** `attr_shift_low` and `attr_shift_high` hold the palette selection bits.
- **Fine-X Scrolling:** The `fine_x` scroll value acts as a selector for which bit in the 16-bit shifters is currently being "emitted" as the pixel.
- **Reloading:** Every 8 dots, the shifters are updated with new data fetched from VRAM. The implementation ensures that bits are shifted only when rendering is enabled and during specific windows (visible area and pre-fetch periods), preventing graphical glitches like the "left-edge black bar."

## 3. Sprite Logic
Sprite handling is split into two distinct phases, matching the hardware's 341-dot cycle:
- **Evaluation (Dots 65–256):** The PPU scans the 256-byte primary OAM (Object Attribute Memory) to find up to 8 sprites that intersect the *next* scanline. These are copied to a 32-byte `oam2` (Secondary OAM).
- **Fetching (Dots 261–320):** The PPU fetches the actual tile data for the 8 sprites found during evaluation.
- **Latches:** The fetched sprite data is stored in `sprite_latches`. During the visible part of the next scanline, these latches are checked to see if any sprite pixel should override the background pixel.

## 4. Scrolling and Timing
- **Internal Registers:** It uses the standard `v` (current VRAM address) and `t` (temporary VRAM address) register logic for scrolling.
- **VBlank and NMI:** The `SET_VBLANK_FLAG` event is precisely timed (triggered at scanline 241, dot 0) to ensure compatibility with sensitive timing tests like *Branch Basics*.
- **Sprite 0 Hit:** The implementation includes a specific `sprite_0_hit_delay` to account for the pipeline delay between the PPU detecting a collision and the CPU seeing the flag in the status register.

## Summary of the Flow
1. **Initialize:** Generate the `events` table once.
2. **Tick:**
    - Get the current event mask.
    - Advance shift registers.
    - Perform VRAM fetches (NT, AT, Tile).
    - Calculate pixel color using current shift register states + Fine-X.
    - Update scrolling registers (`v`) if the event calls for an increment or reset.
    - Handle Sprite Evaluation/Fetching for the next line.
    - Emit the final pixel to the `screen` buffer.
3. **Repeat:** 89,342 times per frame.


