mod envelope;
mod pulse;
mod triangle;
mod noise;
mod dmc;
mod apu_rodio;

use self::noise::Noise;
use self::pulse::Pulse;
use self::triangle::Triangle;
use self::FrameCounterMode::{Step4, Step5};
use std::sync::Arc;
use crossbeam_queue::ArrayQueue;
use tracing::info;
use cpu::cpu2::CYCLES;
use crate::apu::apu_rodio::{create_buffer, OUTPUT_SAMPLE_RATE};
use crate::apu::dmc::Dmc;
use crate::nes_memory::NesMemory;

const CPU_CLOCK_HZ: f32 = 1_789_773.0;
const AUDIO_THROTTLE_TARGET_SAMPLES: usize = (OUTPUT_SAMPLE_RATE as usize) / 10;

#[derive(Clone)]
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
    frame_counter: u32,
    irq_enabled: bool,
    irq_enabled_needs_to_be_cleared: bool,
    frame_counter_irq: bool,
    cycle_count: u64,

    gui_pulse1_enabled: bool,
    gui_pulse2_enabled: bool,
    gui_triangle_enabled: bool,
    gui_noise_enabled: bool,
    gui_dmc_enabled: bool,

    audio_sampler: AudioSampler,
    last_cycles: u128
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
            cycle_count: 0,
            irq_enabled: true,
            irq_enabled_needs_to_be_cleared: false,
            frame_counter_irq: false,

            gui_pulse1_enabled: true,
            gui_pulse2_enabled: true,
            gui_triangle_enabled: true,
            gui_noise_enabled: true,
            gui_dmc_enabled: true,
            audio_sampler: AudioSampler::new(CPU_CLOCK_HZ, OUTPUT_SAMPLE_RATE),
            last_cycles: 0,
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

    fn clock_frame_counter(&mut self) {
        self.frame_counter += 1;

        let mut clock_env = false;
        let mut clock_len = false;

        if matches!(self.frame_counter_mode, Step4) {
            // 4-step mode (60 Hz)
            if self.frame_counter == 3729 {
                clock_env = true;
            } else if self.frame_counter == 7457 {
                clock_env = true;
                clock_len = true;
            } else if self.frame_counter == 11186 {
                clock_env = true;
            } else if self.frame_counter == 14915 {
                clock_env = true;
                clock_len = true;
                self.frame_counter = 0;
                if self.irq_enabled {
                    self.frame_counter_irq = true;
                }
            }
        } else {
            // 5-step mode (48 Hz / 192 Hz)
            self.irq_enabled = false;
            if self.frame_counter == 3729 {
                clock_env = true;
            } else if self.frame_counter == 7457 {
                clock_env = true;
                clock_len = true;
            } else if self.frame_counter == 11186 {
                clock_env = true;
            } else if self.frame_counter == 14915 {
                // Step 4: nothing
            } else if self.frame_counter == 18641 {
                clock_env = true;
                clock_len = true;
                self.frame_counter = 0;
            }
        }

        if clock_env {
            self.pulse1.clock_envelope();
            self.pulse2.clock_envelope();
            self.noise.clock_envelope();
            self.triangle.clock_linear_counter();
        }

        if clock_len {
            self.clock_length_counters();
            self.pulse1.clock_sweep(true);
            self.pulse2.clock_sweep(false);
        }
    }

    /// Return true if IRQ
    pub fn step(&mut self, memory: &mut NesMemory) -> bool {
        self.cycle_count += 1;

        // frame counter runs every cycle
        // self.clock_frame_counter();

        // triangle timer clocks every CPU cycle
        self.triangle.step();
        // The DMC will actually not always step, depends on its rate
        let result = self.dmc.step(memory);

        // pulse and noise timers clock every other CPU cycle (APU cycle)
        if self.cycle_count % 2 == 0 {
            self.clock_frame_counter();
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
            self.noise.clock_timer();
        }
        let mixed = self.calculate_output_sample().clamp(-1.0, 1.0);
        if let Some(sample) = self.audio_sampler.clock(mixed) {
            self.local_buffer.push(sample);

            // Keep the device queue bounded: on overflow, drop the oldest queued sample.
            if self.buffer.push(sample).is_err() {
                let _ = self.buffer.pop();
                let _ = self.buffer.push(sample);
            }
        }

        result
    }

    pub fn audio_queue_depth(&self) -> usize {
        self.buffer.len()
    }

    pub fn can_sleep_for_video_throttle(&self) -> bool {
        self.audio_queue_depth() >= AUDIO_THROTTLE_TARGET_SAMPLES
    }

    fn calculate_output_sample(&self) -> f32 {
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
                self.pulse1.set_enabled((val & 0x01) != 0);
                self.pulse2.set_enabled((val & 0x02) != 0);
                self.triangle.set_enabled((val & 0x04) != 0);
                self.noise.set_enabled((val & 0x08) != 0);
                self.dmc.set_enabled((val & 0x10) != 0);
            }

            // Frame counter
            0x4017 => {
                self.frame_counter_mode = if (val & 0x80) == 0 { Step4 } else { Step5 };
                self.irq_enabled = (val & 0x40) == 0;
                if ! self.irq_enabled {
                    // info!(target: "asm", "Write 4017={:02X}, disabling frame_counter_irq", val);
                    self.frame_counter_irq = false;
                }
                self.frame_counter = 0;

                // If Step5, the units are clocked immediately.
                // In Step4, it just resets the counter (clocks happen at next steps).
                if matches!(self.frame_counter_mode, Step5) {
                    self.pulse1.clock_envelope();
                    self.pulse2.clock_envelope();
                    self.noise.clock_envelope();
                    self.triangle.clock_linear_counter();

                    self.clock_length_counters();
                    self.pulse1.clock_sweep(true);
                    self.pulse2.clock_sweep(false);
                }
            }
            _ => {}
        }
    }

    pub fn get(&mut self, addr: u16) -> u8 {
        let mut result = 0;

        match addr {
            0x4015 => {
                // $4015 read	IF-D NT21
                // 	DMC interrupt (I), frame interrupt (F), DMC active (D), length counter > 0 (N/T/2/1)
                if self.pulse1.length_counter > 0 { result |= 0x01; }
                if self.pulse2.length_counter > 0 { result |= 0x02; }
                if self.triangle.length_counter > 0 { result |= 0x04; }
                if self.noise.length_counter > 0 { result |= 0x08; }
                if self.dmc.is_active() { result |= 0x10; }
                if self.frame_counter_irq & self.irq_enabled{ result |= 0x40; }
                // if self.irq_enabled_needs_to_be_cleared {
                //     result |= 0x40;
                //     self.irq_enabled_needs_to_be_cleared = false;
                // } else {
                // }
                if self.dmc.irq_flag && self.irq_enabled { result |= 0x80; }
                info!(target: "asm",
                    "Read 4015, disabling frame_counter_irq, status: {} val: {:02X} irq_enabled:{}",
                        self.frame_counter_irq, result, self.irq_enabled);
                self.frame_counter_irq = false;
                // 	;;; Test 5 [APU Frame Counter IRQ]: Reading the IRQ flag clears the IRQ flag.
                self.irq_enabled = false;
                // if *CYCLES.read().unwrap() != self.last_cycles {
                //     info!("1 IRQ needs_to_be_cleared = true");
                //     self.irq_enabled_needs_to_be_cleared = true;
                //     self.last_cycles = *CYCLES.read().unwrap();
                // } else if self.irq_enabled_needs_to_be_cleared && *CYCLES.read().unwrap() == self.last_cycles {
                //     self.irq_enabled = false;
                //     self.irq_enabled_needs_to_be_cleared = false;
                // } else if self.irq_enabled_needs_to_be_cleared {
                //     self.irq_enabled_needs_to_be_cleared = false;
                //     info!("3 Clearing now");
                // }
                // self.last_cycles = *CYCLES.read().unwrap();
                info!("cycles:{}, Read [4015] result: {:02X}", *CYCLES.read().unwrap(), result)
            }
            _ => {}
        }

        result
    }

    pub fn set_irq_enabled(&mut self, enabled: bool) {
        self.irq_enabled = enabled;
    }

    pub fn set_pulse1_enabled(&mut self, enabled: bool) {
        self.gui_pulse1_enabled = enabled;
    }

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