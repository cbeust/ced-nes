mod envelope;
mod pulse;
mod triangle;
mod noise;

use self::noise::Noise;
use self::pulse::Pulse;
use self::triangle::Triangle;
use self::FrameCounterMode::{Step4, Step5};
use rodio::source::Source;
use rodio::{nz, ChannelCount, DeviceSinkBuilder, SampleRate};
use std::collections::VecDeque;
use std::num::NonZero;
use std::sync::{Arc, Mutex};

// length counter lookup table
const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20,  2, 40,  4, 80,  6, 160,  8, 60, 10, 14, 12, 26, 14,
    12,  16, 24, 18, 48, 20, 96, 22, 192, 24, 72, 26, 16, 28, 32, 30
];



#[derive(Clone)]
enum FrameCounterMode {
    Step4,
    Step5,
}

#[derive(Clone)]
pub struct Apu {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    local_buffer: Vec<f32>,
    _stream: Arc<dyn Send + Sync>,
    _player: Arc<rodio::Player>,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    // frame counter
    frame_counter_mode: FrameCounterMode,
    frame_counter: u32,
    cycle_count: u64,
    output_sample: f32,
    last_sample: f32,
    frame_irq_inhibit: bool,

    gui_pulse1_enabled: bool,
    gui_pulse2_enabled: bool,
    gui_triangle_enabled: bool,
    gui_noise_enabled: bool,
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
            frame_counter_mode: Step4,
            frame_counter: 0,
            cycle_count: 0,
            output_sample: 0.0,
            last_sample: 0.0,
            frame_irq_inhibit: false,

            gui_pulse1_enabled: true,
            gui_pulse2_enabled: true,
            gui_triangle_enabled: true,
            gui_noise_enabled: true,
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
        let mut step = -1;
        if matches!(self.frame_counter_mode, Step4) {
            // 4 step mode
            if self.frame_counter == 3729 { step = 0; }
            else if self.frame_counter == 7457 { step = 1; }
            else if self.frame_counter == 11186 { step = 2; }
            else if self.frame_counter == 14915 {
                step = 3;
                self.frame_counter = 0;
            }
        } else {
            // 5 step mode
            if self.frame_counter == 3729 { step = 0; }
            else if self.frame_counter == 7457 { step = 1; }
            else if self.frame_counter == 11186 { step = 2; }
            else if self.frame_counter == 14915 { step = 3; }
            else if self.frame_counter == 18641 {
                step = 4;
                self.frame_counter = 0;
            }
        }

        self.frame_counter += 1;
        if step < 0 { return }

        // Clock all channels
        self.pulse1.clock_envelope();
        self.pulse2.clock_envelope();
        self.noise.clock_envelope();
        self.triangle.clock_linear_counter();

        // length counter and sweep clock on steps 1 and 3 (4-step) or 0,2,4 (5-step)
        let clock_length =
            if matches!(self.frame_counter_mode, Step4) {
                step == 1 || step == 3
            } else {
                step == 0 || step == 2 || step == 4
            };

        if clock_length {
            self.clock_length_counters();
            self.pulse1.clock_sweep(true);
            self.pulse2.clock_sweep(false);
        }
    }

    pub fn step(&mut self) {
        self.cycle_count += 1;

        // frame counter runs every cycle
        self.clock_frame_counter();

        // triangle timer clocks every CPU cycle
        self.triangle.step();

        // pulse and noise timers clock every other CPU cycle (APU cycle)
        if self.cycle_count % 2 == 0 {
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
            self.noise.clock_timer();
        }


        // mix channels and generate sample
        // we don't output every cycle, just sample periodically
        // CPU 1.789773 MHz / 44100 Hz = 40.5844 cycles per sample
        // Using an accumulator to handle the fractional cycle count
        if self.cycle_count % 40 == 0 {
            let p1 = if self.gui_pulse1_enabled { self.pulse1.output() } else { 0 };
            let p2 = if self.gui_pulse2_enabled { self.pulse2.output() } else { 0 };
            let tri = if self.gui_triangle_enabled { self.triangle.output() } else { 0 };
            let noise = if self.gui_triangle_enabled { self.noise.output() } else { 0 };

            // Mixing formula from NESdev Wiki
            let pulse_out = if p1 + p2 > 0 {
                95.88 / ((8128.0 / (p1 as f32 + p2 as f32)) + 100.0)
            } else {
                0.0
            };

            let tnd_denom = tri as f32 / 8227.0 + noise as f32 / 12241.0; // + dmc / 22638.0
            let tnd_out = if tnd_denom > 0.0 {
                159.79 / ((1.0 / tnd_denom) + 100.0)
            } else {
                0.0
            };

            self.output_sample = pulse_out + tnd_out;

            // Scale and center. Max output_sample is around 0.15-0.2.
            let s = self.output_sample; // (self.output_sample * 2.0) - 0.5;
            self.last_sample = s;

            // Push to local buffer instead of shared buffer immediately
            self.local_buffer.push(s);

            // self.buffer.lock().unwrap().push_back(s);
        }
    }

    /// Called at the end of each frame to send samples to the audio device
    pub fn flush_samples(&mut self) {
        if self.local_buffer.is_empty() {
            return;
        }

        // println!("Sending {} samples", self.local_buffer.len());
        // Limit buffer size to 2048 samples (~90ms) to avoid massive latency/memory use,
        // there should only be 730 samples per frame
        self.buffer.lock().unwrap().extend(self.local_buffer.drain(..));
    }

    pub fn set(&mut self, addr: u16, val: u8) {
        match addr {
            0x4000..=0x4003 => { self.pulse1.set(addr, val); }
            0x4004..=0x4007 => { self.pulse2.set(addr, val); }
            0x4008..=0x400b => { self.triangle.set(addr, val); }
            0x400c..=0x400f => { self.noise.set(addr, val); }

            //
            // Status
            //
            0x4015 => {
                self.pulse1.enabled = (val & 0x01) != 0;
                self.pulse2.enabled = (val & 0x02) != 0;
                self.triangle.enabled = (val & 0x04) != 0;
                self.noise.enabled = (val & 0x08) != 0;

                if ! self.pulse1.enabled { self.pulse1.length_counter = 0; }
                if ! self.pulse2.enabled { self.pulse2.length_counter = 0; }
                if ! self.triangle.enabled { self.triangle.length_counter = 0;}
                if ! self.noise.enabled { self.noise.length_counter = 0; }
            }

            // Frame counter
            0x4017 => {
                self.frame_counter_mode = if (val & 0x80) == 0 { Step4 } else { Step5 };
                self.frame_irq_inhibit = (val & 0x40) != 0;
                self.frame_counter = 0;
                // If Step5, clock everything immediately
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

    pub fn get(&self, addr: u16) -> u8 {
        if addr == 0x4015 {
            let mut status = 0;
            if self.pulse1.length_counter > 0 {
                status |= 0x01;
            }
            if self.pulse2.length_counter > 0 {
                status |= 0x02;
            }
            if self.triangle.length_counter > 0 {
                status |= 0x04;
            }
            if self.noise.length_counter > 0 {
                status |= 0x08;
            }
            return status;
        }

        0
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

}

//
// rodio details
//

pub struct ApuSource {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
    last_sample: f32,
}

impl ApuSource {
    pub fn new(buffer: Arc<Mutex<VecDeque<f32>>>, sample_rate: u32) -> Self {
        Self { buffer, sample_rate, last_sample: 0.0 }
    }
}

impl Iterator for ApuSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let mut buf = self.buffer.lock().unwrap();
        // If buffer is empty, return the last sample to reduce sharp pops
        if let Some(s) = buf.pop_front() {
            self.last_sample = s;
        }
        Some(self.last_sample)
    }
}

impl Source for ApuSource {
    fn current_span_len(&self) -> Option<usize> { None }
    fn channels(&self) -> ChannelCount { nz!(1) }
    fn sample_rate(&self) -> SampleRate { NonZero::new(self.sample_rate).unwrap() }
    fn total_duration(&self) -> Option<std::time::Duration> { None }
}

pub fn create_buffer() -> (Arc<Mutex<VecDeque<f32>>>, Box<dyn Send + Sync>, rodio::Player) {
    let stream = DeviceSinkBuilder::open_default_sink().unwrap();
    let player = rodio::Player::connect_new(stream.mixer());

    let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
    let sample_rate = 44100;
    let source = ApuSource::new(Arc::clone(&buffer), sample_rate);
    player.append(source);
    player.play();

    // Your emulator loop pushes samples like:
    // buffer.lock().unwrap().push_back(sample);

    // let freq = 440.0;
    // let mut phase: f32 = 0.0;
    // let phase_inc = 2.0 * std::f32::consts::PI * freq / sample_rate as f32;
    //
    // for _ in 0..sample_rate * 5 {
    //     let sample = phase.sin();
    //     buffer.lock().unwrap().push_back(sample);
    //     phase = (phase + phase_inc) % (2.0 * std::f32::consts::PI);
    // }

    (buffer, Box::new(stream), player)
    // std::thread::sleep(std::time::Duration::from_secs(1));
}