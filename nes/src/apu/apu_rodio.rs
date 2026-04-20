use crossbeam_queue::ArrayQueue;
use std::num::NonZero;
use std::sync::Arc;
use rodio::{nz, ChannelCount, DeviceSinkBuilder, SampleRate, Source};

pub const OUTPUT_SAMPLE_RATE: u32 = 44_100;
pub const AUDIO_QUEUE_CAPACITY: usize = 16_384;
pub const AUDIO_PREROLL_SAMPLES: usize = 2_048;
const RECOVERY_RAMP_SAMPLES: u8 = 32;

pub struct ApuSource {
    buffer: Arc<ArrayQueue<f32>>,
    sample_rate: u32,
    last_sample: f32,
    underrun_streak: u32,
    recovery_blend_remaining: u8,
}

impl ApuSource {
    pub fn new(buffer: Arc<ArrayQueue<f32>>, sample_rate: u32) -> Self {
        Self {
            buffer,
            sample_rate,
            last_sample: 0.0,
            underrun_streak: 0,
            recovery_blend_remaining: 0,
        }
    }
}

impl Iterator for ApuSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(s) = self.buffer.pop() {
            if self.underrun_streak > 0 {
                self.underrun_streak = 0;
                self.recovery_blend_remaining = RECOVERY_RAMP_SAMPLES;
            }

            if self.recovery_blend_remaining > 0 {
                let progress = 1.0
                    - (self.recovery_blend_remaining as f32 / RECOVERY_RAMP_SAMPLES as f32);
                let blend = progress.clamp(0.2, 1.0);
                self.last_sample += (s - self.last_sample) * blend;
                self.recovery_blend_remaining -= 1;
            } else {
                self.last_sample = s;
            }
        } else {
            // Gently decay toward silence during underruns instead of holding a DC level.
            self.underrun_streak = self.underrun_streak.saturating_add(1);
            self.recovery_blend_remaining = 0;
            self.last_sample *= 0.98;
            if self.last_sample.abs() < 0.0001 {
                self.last_sample = 0.0;
            }
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

pub fn create_buffer() -> (Arc<ArrayQueue<f32>>, Box<dyn Send + Sync>, rodio::Player) {
    let mut stream = DeviceSinkBuilder::open_default_sink().unwrap();
    stream.log_on_drop(false);
    let player = rodio::Player::connect_new(stream.mixer());

    let buffer = Arc::new(ArrayQueue::new(AUDIO_QUEUE_CAPACITY));
    for _ in 0..AUDIO_PREROLL_SAMPLES {
        let _ = buffer.push(0.0);
    }

    let sample_rate = OUTPUT_SAMPLE_RATE;
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