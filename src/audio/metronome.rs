//! Metronome audio engine. Uses cpal's output stream on both native and web:
//! a per-sample beat clock in the audio callback emits a short decaying sine
//! "click" at each beat boundary and reports the current beat back to the UI.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

#[derive(Clone, Copy)]
struct Control {
    bpm: u32,
    beats: u32,
    running: bool,
    /// Continuous reference tone frequency (Hz), or `None` when silent.
    tone: Option<f32>,
}

pub struct Metronome {
    control: Arc<Mutex<Control>>,
    /// Beat currently sounding (0-based), written by the audio callback.
    current_beat: Arc<AtomicU32>,
    stream: Option<cpal::Stream>,
    started: bool,
}

impl Metronome {
    pub fn new() -> Self {
        Self {
            control: Arc::new(Mutex::new(Control {
                bpm: 120,
                beats: 4,
                running: false,
                tone: None,
            })),
            current_beat: Arc::new(AtomicU32::new(0)),
            stream: None,
            started: false,
        }
    }

    /// Push the latest UI settings to the audio thread.
    pub fn set(&self, bpm: u32, beats: u32, running: bool) {
        if let Ok(mut c) = self.control.lock() {
            c.bpm = bpm.clamp(20, 400);
            c.beats = beats.max(1);
            c.running = running;
        }
    }

    /// Set (or clear) the continuous reference tone.
    pub fn set_tone(&self, freq: Option<f32>) {
        if let Ok(mut c) = self.control.lock() {
            c.tone = freq;
        }
    }

    pub fn beat(&self) -> u32 {
        self.current_beat.load(Ordering::Relaxed)
    }

    /// Lazily open the audio stream. Must be triggered by a user gesture on web
    /// (autoplay policy); harmless to call repeatedly.
    pub fn ensure_started(&mut self) {
        if self.started {
            return;
        }
        self.started = true; // don't retry every frame on failure
        match self.build_stream() {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    log::error!("metronome: stream.play failed: {e}");
                }
                self.stream = Some(stream);
            }
            Err(e) => log::error!("metronome: audio init failed: {e:#}"),
        }
    }

    fn build_stream(&self) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no default output device")?;
        let config = device
            .default_output_config()
            .context("default_output_config")?;

        let sample_format = config.sample_format();
        let cfg = config.config();
        match sample_format {
            cpal::SampleFormat::F32 => self.build_for::<f32>(&device, &cfg),
            cpal::SampleFormat::I16 => self.build_for::<i16>(&device, &cfg),
            cpal::SampleFormat::U16 => self.build_for::<u16>(&device, &cfg),
            other => anyhow::bail!("unsupported sample format: {other:?}"),
        }
    }

    fn build_for<T>(&self, device: &cpal::Device, cfg: &cpal::StreamConfig) -> Result<cpal::Stream>
    where
        T: SizedSample + FromSample<f32>,
    {
        let sample_rate = cfg.sample_rate.0 as f32;
        let channels = cfg.channels as usize;
        let control = self.control.clone();
        let current_beat = self.current_beat.clone();

        // DSP state owned by the callback.
        let mut samples_since_beat = f64::MAX; // trigger immediately when running
        let mut beat_index: u32 = 0;
        let mut prev_running = false;
        let mut env: f32 = 0.0;
        let mut phase: f32 = 0.0;
        let mut freq: f32 = 0.0;
        // Continuous reference-tone state (with a smoothed gain to avoid pops).
        let mut tone_phase: f32 = 0.0;
        let mut tone_gain: f32 = 0.0;

        let err_fn = |e| log::error!("metronome: stream error: {e}");
        let decay_per_sample = 1.0 / (0.035 * sample_rate); // ~35 ms click
        let gain_step = 1.0 / (0.01 * sample_rate); // ~10 ms attack/release

        device
            .build_output_stream(
                cfg,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    // Never panic in the audio callback: if the lock is poisoned,
                    // emit silence for this buffer rather than crashing the thread.
                    let ctl = match control.lock() {
                        Ok(guard) => *guard,
                        Err(_) => {
                            for s in data.iter_mut() {
                                *s = T::from_sample(0.0);
                            }
                            return;
                        }
                    };
                    let spb = (sample_rate as f64) * 60.0 / ctl.bpm.max(1) as f64;

                    if ctl.running && !prev_running {
                        samples_since_beat = spb; // downbeat on the next sample
                        beat_index = 0;
                    }
                    prev_running = ctl.running;

                    for frame in data.chunks_mut(channels) {
                        if ctl.running {
                            samples_since_beat += 1.0;
                            if samples_since_beat >= spb {
                                samples_since_beat -= spb;
                                let downbeat = beat_index == 0;
                                freq = if downbeat { 1000.0 } else { 800.0 };
                                env = 1.0;
                                phase = 0.0;
                                current_beat.store(beat_index, Ordering::Relaxed);
                                beat_index = (beat_index + 1) % ctl.beats.max(1);
                            }
                        }

                        let click = if env > 0.0 {
                            phase += freq / sample_rate;
                            let v = (phase * std::f32::consts::TAU).sin() * env * 0.6;
                            env -= decay_per_sample;
                            v
                        } else {
                            0.0
                        };

                        // Continuous reference tone, gain-ramped in/out.
                        let target_gain = if ctl.tone.is_some() { 0.18 } else { 0.0 };
                        tone_gain += (target_gain - tone_gain).clamp(-gain_step, gain_step);
                        let tone = if let Some(tf) = ctl.tone {
                            tone_phase += tf / sample_rate;
                            (tone_phase * std::f32::consts::TAU).sin() * tone_gain
                        } else if tone_gain > 0.0 {
                            tone_phase += 440.0 / sample_rate;
                            (tone_phase * std::f32::consts::TAU).sin() * tone_gain
                        } else {
                            0.0
                        };

                        if tone_phase > 1.0 {
                            tone_phase -= tone_phase.floor();
                        }

                        let out = T::from_sample(click + tone);
                        for ch in frame.iter_mut() {
                            *ch = out;
                        }
                    }
                },
                err_fn,
                None,
            )
            .context("build_output_stream")
    }
}
