mod envelope;
mod pulse;
mod triangle;
mod noise;
pub mod dmc;
mod apu_rodio;

use self::noise::Noise;
use self::pulse::Pulse;
use self::triangle::Triangle;
use self::FrameCounterMode::{Step4, Step5};
use std::sync::Arc;
use crossbeam_queue::ArrayQueue;
use tracing::{debug, info};
use crate::apu::apu_rodio::{create_buffer, OUTPUT_SAMPLE_RATE};
use crate::apu::dmc::Dmc;
use crate::nes_memory::NesMemory;

const CPU_CLOCK_HZ: f32 = 1_789_773.0;
const AUDIO_THROTTLE_TARGET_SAMPLES: usize = (OUTPUT_SAMPLE_RATE as usize) / 10;
const MASTER_GAIN: f32 = 0.1;

fn apply_master_gain(sample: f32) -> f32 {
    (sample * MASTER_GAIN).clamp(-1.0, 1.0)
}

#[derive(Clone, Debug)]
enum FrameCounterMode {
    Step4,
    Step5,
}

#[derive(Clone)]
pub struct Apu {
    buffer: Arc<ArrayQueue<f32>>,
    local_buffer: Vec<f32>,
    _stream: Arc<dyn Send + Sync>,
    _player: Arc<rodio::Player>,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: Dmc,

    // frame counter
    frame_counter_mode: FrameCounterMode,
    /// Sequencer position in CPU cycles (Mesen/AccuracyCoin step table).
    frame_counter: u32,
    /// Current step index into the 4/5-step sequence (0..5).
    frame_step: u8,
    /// After $4017 write: remaining CPU cycles before reset takes effect (0 = idle).
    frame_reset_delay: u8,
    /// Pending $4017 value while delay is counting (-1 = none), as i16.
    frame_new_value: i16,
    /// Block frame ticks for N cycles after a tick (Mesen `_blockFrameCounterTick`).
    block_frame_tick: u8,
    irq_enabled: bool,
    irq_enabled_needs_to_be_cleared: bool,
    frame_counter_irq: bool,
    /// Physical IRQ level signal driven to the CPU
    frame_irq_line: bool,
    /// $4015 read schedules clear of frame IRQ on the next CPU get cycle.
    frame_irq_clear_pending: bool,
    cycle_count: u64,

    gui_sound_enabled: bool,
    gui_pulse1_enabled: bool,
    gui_pulse2_enabled: bool,
    gui_triangle_enabled: bool,
    gui_noise_enabled: bool,
    gui_dmc_enabled: bool,

    audio_sampler: AudioSampler,
    is_get_phase: bool,
}

impl Apu {
    pub fn new() -> Self {
        let (buffer, stream, player) = create_buffer();
        Self {
            buffer,
            local_buffer: Vec::with_capacity(735),
            _stream: Arc::new(stream),
            _player: Arc::new(player),
            pulse1: Pulse::default(),
            pulse2: Pulse::default(),
            triangle: Triangle::default(),
            noise: Noise::new(),
            dmc: Dmc::default(),

            frame_counter_mode: Step4,
            frame_counter: 0,
            frame_step: 0,
            frame_reset_delay: 0,
            frame_new_value: -1,
            block_frame_tick: 0,
            cycle_count: 0,
            irq_enabled: true,
            irq_enabled_needs_to_be_cleared: false,
            frame_counter_irq: false,
            frame_irq_line: false,
            frame_irq_clear_pending: false,

            gui_sound_enabled: true,
            gui_pulse1_enabled: true,
            gui_pulse2_enabled: true,
            gui_triangle_enabled: true,
            gui_noise_enabled: true,
            gui_dmc_enabled: true,
            audio_sampler: AudioSampler::new(CPU_CLOCK_HZ, OUTPUT_SAMPLE_RATE),
            is_get_phase: false,
        }
    }

    fn clock_length_counters(&mut self) {
        if self.pulse1.length_counter > 0 && (self.pulse1.reg_ctrl & 0x20) == 0 {
            self.pulse1.length_counter -= 1;
        }
        if self.pulse2.length_counter > 0 && (self.pulse2.reg_ctrl & 0x20) == 0 {
            self.pulse2.length_counter -= 1;
        }
        if self.triangle.length_counter > 0 && !self.triangle.control_flag {
            self.triangle.length_counter -= 1;
        }
        if self.noise.length_counter > 0 && (self.noise.reg_ctrl & 0x20) == 0 {
            self.noise.length_counter -= 1;
        }
    }

    fn clock_quarter_frame(&mut self) {
        self.pulse1.clock_envelope();
        self.pulse2.clock_envelope();
        self.noise.clock_envelope();
        self.triangle.clock_linear_counter();
    }

    fn clock_half_frame(&mut self) {
        self.clock_length_counters();
        self.pulse1.clock_sweep(true);
        self.pulse2.clock_sweep(false);
    }

    /// NTSC step cycle table (Mesen / AccuracyCoin).
    const STEP_CYCLES_4: [u32; 6] = [7457, 14913, 22371, 29828, 29829, 29830];
    const STEP_CYCLES_5: [u32; 6] = [7457, 14913, 22371, 29829, 37281, 37282];

    /// Clock the frame sequencer once per CPU cycle (counter is in CPU cycles).
    fn clock_frame_counter(&mut self) {
        self.frame_counter = self.frame_counter.wrapping_add(1);

        let steps = if matches!(self.frame_counter_mode, Step4) {
            &Self::STEP_CYCLES_4
        } else {
            &Self::STEP_CYCLES_5
        };
        let step = self.frame_step as usize;
        if step >= 6 || self.frame_counter < steps[step] {
            return;
        }

        // IRQ flag on last 3 steps of 4-step mode (Mesen / AccuracyCoin).
        // Even with interrupt inhibit set, the flag is forced set on steps 3–4
        // (cycles 29828–29829) and only follows inhibit on step 5 (29830).
        // Setting the flag cancels a pending $4015 clear so a re-set on these
        // cycles is visible to a following read (tests E–G).
        //
        // However, the external IRQ level detector wire is only asserted if
        // irq_enabled is true, and on step >= 4 (cycle 29829 or 29830).
        if matches!(self.frame_counter_mode, Step4) && step >= 3 {
            if self.irq_enabled {
                self.frame_counter_irq = true;
                if step >= 4 {
                    self.frame_irq_line = true;
                }
                self.frame_irq_clear_pending = false;
            } else if step < 5 {
                // Inhibit: still set flag for two cycles (tests I–K).
                self.frame_counter_irq = true;
                self.frame_irq_clear_pending = false;
            } else {
                // Step 5 with inhibit: flag cleared (test L).
                self.frame_counter_irq = false;
                self.frame_irq_line = false;
                self.frame_irq_clear_pending = false;
            }
        }

        // Frame type for this step: Q, H, Q, —, H, —
        let (do_q, do_h) = match step {
            0 => (true, false),
            1 => (true, true),
            2 => (true, false),
            3 => (false, false),
            4 => (true, true),
            _ => (false, false),
        };

        if (do_q || do_h) && self.block_frame_tick == 0 {
            if do_q {
                self.clock_quarter_frame();
            }
            if do_h {
                self.clock_half_frame();
            }
            self.block_frame_tick = 2;
        }

        self.frame_step += 1;
        if self.frame_step >= 6 {
            self.frame_step = 0;
            self.frame_counter = 0;
        }
    }

    pub fn set_phase(&mut self, phase: bool) {
        info!(target: "asm", "SETTING PHASE FORCEFULLY TO {phase}");
        self.is_get_phase = phase;
    }

    pub fn is_get_phase(&self) -> bool {
        self.is_get_phase
    }

    /// Apply a pending $4015 frame-IRQ clear (next get edge after a status read).
    /// Also used between OAM DMA sprite fetches so multi-read tables see the
    /// delayed clear without double-clocking the full APU.
    pub fn tick_frame_irq_clear(&mut self) {
        if self.frame_irq_clear_pending {
            self.frame_counter_irq = false;
            self.frame_irq_line = false;
            self.frame_irq_clear_pending = false;
        }
    }

    /// Return true if IRQ
    pub fn step(&mut self, memory: &mut NesMemory, is_get_phase: bool) -> bool {
        debug!(target: "asm", "{}", if is_get_phase { "GET" } else { "PUT" });
        let _ = self.irq_enabled_needs_to_be_cleared;
        self.cycle_count += 1;

        // Frame IRQ clear runs at the start of a CPU get cycle after $4015 read
        // (AccuracyCoin tests 6–7: clear is delayed to the next get edge).
        if is_get_phase {
            self.tick_frame_irq_clear();
        }
        self.is_get_phase = is_get_phase;

        // Apply pending $4017 write after delay (Mesen).
        let mut just_applied_4017 = false;
        if self.frame_new_value >= 0 {
            self.frame_reset_delay = self.frame_reset_delay.saturating_sub(1);
            if self.frame_reset_delay == 0 {
                let val = self.frame_new_value as u8;
                self.frame_new_value = -1;
                self.frame_counter_mode = if (val & 0x80) == 0 { Step4 } else { Step5 };
                self.frame_step = 0;
                self.frame_counter = 0;
                just_applied_4017 = true;
                // Note: switching to 5-step does NOT clear an already-set frame IRQ
                // flag (AccuracyCoin Frame Counter IRQ test 8).
                if matches!(self.frame_counter_mode, Step5) && self.block_frame_tick == 0 {
                    // Immediate half-frame clock on 5-step write.
                    self.clock_quarter_frame();
                    self.clock_half_frame();
                    self.block_frame_tick = 2;
                }
            }
        }
        if self.block_frame_tick > 0 {
            self.block_frame_tick -= 1;
        }

        // triangle timer clocks every CPU cycle
        self.triangle.step();
        // Delayed $4015 D4 clear (explicit DMA abort window).
        self.dmc.tick_disable_delay();
        // DMC timer/output unit (sample fetches are handled separately via DMA).
        let result = self.dmc.step(memory);

        // Frame sequencer advances every CPU cycle. Skip the apply cycle so
        // counter stays 0 (Mesen previousCycle=0) and half-frame lands at 14913.
        if !just_applied_4017 {
            self.clock_frame_counter();
        }
        // Pulse and noise timers clock every other CPU cycle (APU cycle).
        if self.cycle_count % 2 == 0 {
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
            self.noise.clock_timer();
        }
        let mixed = apply_master_gain(self.calculate_output_sample());
        if let Some(sample) = self.audio_sampler.clock(mixed) {
            self.local_buffer.push(sample);

            // Keep the device queue bounded: on overflow, drop the oldest queued sample.
            if self.buffer.push(sample).is_err() {
                let _ = self.buffer.pop();
                let _ = self.buffer.push(sample);
            }
        }

        result || self.frame_irq_line
    }

    /// Advance one cycle of DMC DMA if needed. Returns true if the CPU is stalled.
    pub fn dmc_dma_tick(
        &mut self,
        memory: &mut NesMemory,
        dummy_read_address: Option<u16>,
        is_get: bool,
        is_write: bool,
    ) -> bool {
        self.dmc.dma_tick(memory, dummy_read_address, is_get, is_write)
    }

    pub fn dmc_dma_active(&self) -> bool {
        self.dmc.dma_active()
    }

    pub fn dmc_dma_pending(&self) -> bool {
        self.dmc.dma_pending()
    }

    pub fn dmc_tick_load_delay(&mut self) {
        self.dmc.tick_load_delay()
    }

    /// Advance DMC DMA state only (no memory I/O). Used by the emulator to avoid
    /// re-entrant APU locks when dummy-reading $4015.
    pub fn dmc_take_cycle(&mut self, is_get: bool, is_write: bool) -> dmc::DmaCycle {
        self.dmc.dma_cycle(is_get, is_write)
    }

    pub fn dmc_finish_fetch(&mut self, byte: u8) {
        self.dmc.finish_sample_fetch(byte);
    }

    pub fn audio_queue_depth(&self) -> usize {
        self.buffer.len()
    }

    pub fn can_sleep_for_video_throttle(&self) -> bool {
        self.audio_queue_depth() >= AUDIO_THROTTLE_TARGET_SAMPLES
    }

    fn calculate_output_sample(&self) -> f32 {
        if ! self.gui_sound_enabled {
            return 0.0;
        }

        let p1 = if self.gui_pulse1_enabled { self.pulse1.output() } else { 0 };
        let p2 = if self.gui_pulse2_enabled { self.pulse2.output() } else { 0 };
        let triangle = if self.gui_triangle_enabled { self.triangle.output() } else { 0 };
        let noise = if self.gui_noise_enabled { self.noise.output() } else { 0 };
        let dmc = if self.gui_dmc_enabled { self.dmc.output() } else { 0 };

        // Mixing formula from NESdev Wiki
        let pulse_out = if p1 + p2 > 0 {
            95.88 / ((8128.0 / (p1 as f32 + p2 as f32)) + 100.0)
        } else {
            0.0
        };

        let tnd_denom = triangle as f32 / 8227.0 + noise as f32 / 12241.0 + dmc as f32 / 22638.0;
        let tnd_out = if tnd_denom > 0.0 {
            159.79 / ((1.0 / tnd_denom) + 100.0)
        } else {
            0.0
        };

        (pulse_out + tnd_out) / 2.0
    }

    /// Called at the end of each frame to expose samples to the UI/debug path.
    pub fn flush_samples(&mut self) -> Vec<f32> {
        if self.local_buffer.is_empty() {
            return Vec::new();
        }
        std::mem::take(&mut self.local_buffer)
    }

    pub fn set(&mut self, addr: u16, val: u8) {
        match addr {
            //
            // Registers
            //
            0x4000..=0x4003 => { self.pulse1.set(addr, val); }
            0x4004..=0x4007 => { self.pulse2.set(addr, val); }
            0x4008..=0x400b => { self.triangle.set(addr, val); }
            0x400c..=0x400f => { self.noise.set(addr, val); }
            0x4010..=0x4013 => { self.dmc.set(addr, val); }

            //
            // Status
            //
            0x4015 => {
                // info!("Setting 4015 to {val:02X}");
                self.pulse1.set_enabled((val & 0x01) != 0);
                self.pulse2.set_enabled((val & 0x02) != 0);
                self.triangle.set_enabled((val & 0x04) != 0);
                self.noise.set_enabled((val & 0x08) != 0);
                // Cycle parity for explicit-stop disable delay (even→2, odd→3).
                let cycle_odd = (self.cycle_count & 1) != 0;
                self.dmc.set_enabled((val & 0x10) != 0, cycle_odd);
            }

            // Frame counter
            0x4017 => {
                // Bit 6 = interrupt inhibit.
                self.irq_enabled = (val & 0x40) == 0;
                if (val & 0x40) != 0 {
                    self.frame_counter_irq = false;
                    self.frame_irq_line = false;
                    self.frame_irq_clear_pending = false;
                }
                // Mode/reset applied after 3–4 CPU cycles (Mesen / NESdev).
                // Put-cycle write → 3; get-cycle write → 4.
                self.frame_new_value = val as i16;
                self.frame_reset_delay = if self.is_get_phase { 4 } else { 3 };
            }
            _ => {}
        }
    }

    pub fn get(&mut self, addr: u16) -> u8 {
        let mut result = 0;

        match addr {
            0x4015 => {
                // $4015 read: IF-D NT21
                // DMC interrupt (I), frame interrupt (F), DMC active (D), length counters
                if self.pulse1.length_counter > 0 { result |= 0x01; }
                if self.pulse2.length_counter > 0 { result |= 0x02; }
                if self.triangle.length_counter > 0 { result |= 0x04; }
                if self.noise.length_counter > 0 { result |= 0x08; }
                if self.dmc.is_active() { result |= 0x10; }
                // Frame interrupt flag is visible whenever set (inhibit from $4017
                // prevents it from being *set*, but a set flag still reads as 1).
                if self.frame_counter_irq { result |= 0x40; }
                if self.dmc.irq_flag { result |= 0x80; }

                // Schedule clear on the next CPU get cycle (not immediate).
                // AccuracyCoin Frame Counter IRQ tests 6–7.
                self.frame_irq_clear_pending = true;
                self.irq_enabled_needs_to_be_cleared = false;
            }
            _ => {
                // Write only registers return open bus value
                result = ((addr & 0xff00) >> 8) as u8;
            }
        }

        result
    }

    pub fn set_irq_enabled(&mut self, enabled: bool) {
        if matches!(self.frame_counter_mode, Step4) {
            // info!("Setting irq_enabled to {enabled}");
            self.irq_enabled = enabled;
        }
    }

    /// Reset all APU channel/counter state while keeping the audio device alive.
    /// This is called on emulator reboot to avoid recreating the rodio device
    /// (which causes a click from the hardware discontinuity).
    pub fn reset_state(&mut self) {
        // Drain stale samples from the queue.
        // We do NOT push silence here: the ApuSource underrun path
        // (`last_sample *= 0.98`) will decay the existing DC level smoothly to
        // zero instead of causing an instant level-jump (which would click).
        while self.buffer.pop().is_some() {}

        // Reset all sound channels
        self.pulse1 = Pulse::default();
        self.pulse2 = Pulse::default();
        self.triangle = Triangle::default();
        self.noise = Noise::new();
        self.dmc = Dmc::default();

        // Reset frame counter
        self.frame_counter_mode = Step4;
        self.frame_counter = 0;
        self.frame_step = 0;
        self.frame_reset_delay = 0;
        self.frame_new_value = -1;
        self.block_frame_tick = 0;
        self.cycle_count = 0;
        self.irq_enabled = true;
        self.irq_enabled_needs_to_be_cleared = false;
        self.frame_counter_irq = false;
        self.frame_irq_clear_pending = false;
        self.is_get_phase = false;

        // Reset the audio sampler
        self.audio_sampler = AudioSampler::new(CPU_CLOCK_HZ, OUTPUT_SAMPLE_RATE);
        self.local_buffer.clear();
    }

    pub fn set_sound_enabled(&mut self, enabled: bool) {
        self.gui_sound_enabled = enabled;
    }
    pub fn set_pulse1_enabled(&mut self, enabled: bool) { self.gui_pulse1_enabled = enabled; }
    pub fn set_pulse2_enabled(&mut self, enabled: bool) {
        self.gui_pulse2_enabled = enabled;
    }
    pub fn set_triangle_enabled(&mut self, enabled: bool) {
        self.gui_triangle_enabled = enabled;
    }
    pub fn set_noise_enabled(&mut self, enabled: bool) {
        self.gui_noise_enabled = enabled;
    }
    pub fn set_dmc_enabled(&mut self, enabled: bool) {
        self.gui_dmc_enabled = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_master_gain_halves_sample() {
        assert!((apply_master_gain(1.0) - 0.1).abs() < f32::EPSILON);
        assert!((apply_master_gain(-1.0) + 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn test_apply_master_gain_clamps_range() {
        assert!((apply_master_gain(30.0) - 1.0).abs() < f32::EPSILON);
        assert!((apply_master_gain(-30.0) + 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn five_step_half_frame_at_14913() {
        let mut apu = Apu::new();
        // Simulate applied $80 write: 5-step, length 2, immediate half → length 1.
        apu.pulse1.set_enabled(true);
        apu.pulse1.set(0x4000, 0x10);
        apu.pulse1.set(0x4003, 0x18); // length = 2
        assert_eq!(apu.pulse1.length_counter, 2);
        apu.frame_counter_mode = Step5;
        apu.frame_step = 0;
        apu.frame_counter = 0;
        apu.block_frame_tick = 0;
        apu.clock_quarter_frame();
        apu.clock_half_frame();
        apu.block_frame_tick = 2;
        assert_eq!(apu.pulse1.length_counter, 1);

        // Run 14913 sequencer clocks (skip apply cycle).
        let mut half_at = None;
        for i in 1..=15000u32 {
            if apu.block_frame_tick > 0 {
                apu.block_frame_tick -= 1;
            }
            let len_before = apu.pulse1.length_counter;
            apu.clock_frame_counter();
            if len_before == 1 && apu.pulse1.length_counter == 0 {
                half_at = Some(i);
                break;
            }
        }
        assert_eq!(half_at, Some(14913), "5-step first half should be at cycle 14913, got {half_at:?}");
    }
}

// Length counter lookup table
const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20,  2, 40,  4, 80,  6, 160,  8, 60, 10, 14, 12, 26, 14,
    12,  16, 24, 18, 48, 20, 96, 22, 192, 24, 72, 26, 16, 28, 32, 30
];

#[derive(Clone)]
pub struct AudioSampler {
    accumulator: f32,
    sample_count: u32,
    cycles_per_sample: f32,
    cycle_counter: f32,
}

impl AudioSampler {
    pub fn new(cpu_clock_hz: f32, output_sample_rate: u32) -> Self {
        Self {
            accumulator: 0.0,
            sample_count: 0,
            cycles_per_sample: cpu_clock_hz / output_sample_rate as f32,
            cycle_counter: 0.0,
        }
    }

    /// Call this every CPU cycle with the current APU output value.
    /// Returns Some(sample) when it's time to emit a sample, None otherwise.
    pub fn clock(&mut self, apu_output: f32) -> Option<f32> {
        self.accumulator += apu_output;
        self.sample_count += 1;
        self.cycle_counter += 1.0;

        if self.cycle_counter >= self.cycles_per_sample {
            self.cycle_counter -= self.cycles_per_sample;

            let sample = if self.sample_count > 0 {
                self.accumulator / self.sample_count as f32
            } else {
                0.0
            };

            self.accumulator = 0.0;
            self.sample_count = 0;

            Some(sample)
        } else {
            None
        }
    }
}