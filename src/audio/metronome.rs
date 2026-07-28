//! Metronome audio engine. Uses cpal's output stream on both native and web:
//! a per-sample beat clock in the audio callback emits a short decaying sine
//! "click" at each beat boundary and reports the current beat back to the UI.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use anyhow::{Context as _, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use serde::{Deserialize, Serialize};

/// Which click sound the metronome makes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Sound {
    /// Synthesized electronic "beep" — a short decaying sine.
    #[default]
    Electronic,
    /// Old-school mechanical metronome "tick" — a short wooden knock.
    Mechanical,
}

impl Sound {
    /// Total decode of the atomic representation (`self as u8`); an unknown value
    /// falls back to the default, so the audio thread never sees an invalid one.
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Sound::Mechanical,
            _ => Sound::Electronic,
        }
    }
}

/// The primary per-beat click. Chosen when a beat fires and held until it
/// finishes, so a mid-click settings change never switches the waveform partway.
/// The mechanical downbeat rings a separate bell layer *on top* of the tick.
#[derive(Clone, Copy)]
enum Voice {
    /// Electronic beep — a decaying sine.
    Beep,
    /// Mechanical wooden tick — noise + body. Plays on every mechanical beat.
    Tick,
}

/// Shared metronome settings: independent atomics the UI writes and the audio
/// callback reads, all lock-free. The fields carry no cross-field invariant, so
/// a per-field read that briefly mixes a new value with an old one is harmless —
/// it self-corrects on the next buffer.
struct Control {
    bpm: AtomicU32,
    beats: AtomicU32,
    running: AtomicBool,
}

pub struct Metronome {
    /// Latest UI settings, shared as a bundle of atomics: the UI `store`s each
    /// field, the audio callback `load`s them — a lock-free read on the
    /// real-time thread, with no mutex to block on or poison.
    control: Arc<Control>,
    /// Monotonic beat counter, incremented once per beat by the audio callback.
    /// The UI watches it for beat changes (e.g. to swing the needle).
    beat_count: Arc<AtomicU32>,
    /// Click sound — a user *setting*, not a live transport control, so it lives
    /// in its own atomic rather than in `Control`. Read by the audio callback.
    sound: Arc<AtomicU8>,
    stream: Option<cpal::Stream>,
    started: bool,
}

impl Metronome {
    pub fn new() -> Self {
        Self {
            control: Arc::new(Control {
                bpm: AtomicU32::new(120),
                beats: AtomicU32::new(4),
                running: AtomicBool::new(false),
            }),
            beat_count: Arc::new(AtomicU32::new(0)),
            sound: Arc::new(AtomicU8::new(Sound::Electronic as u8)),
            stream: None,
            started: false,
        }
    }

    /// Push the latest UI settings to the audio thread. The clamps are a
    /// defensive net against an out-of-range value from any caller; the UI paths
    /// already keep these within range (see `crate::app` bounds).
    pub fn set(&self, bpm: u32, beats: u32, running: bool) {
        use crate::app::{BEATS_MAX, BEATS_MIN, BPM_MAX, BPM_MIN};
        self.control
            .bpm
            .store(bpm.clamp(BPM_MIN, BPM_MAX), Ordering::Relaxed);
        self.control
            .beats
            .store(beats.clamp(BEATS_MIN, BEATS_MAX), Ordering::Relaxed);
        self.control.running.store(running, Ordering::Relaxed);
    }

    /// Set the click sound. A setting rather than a live transport control, so it
    /// has its own setter (like the tuner's `set_scale` / `set_a4`).
    pub fn set_sound(&self, sound: Sound) {
        self.sound.store(sound as u8, Ordering::Relaxed);
    }

    pub fn beat_count(&self) -> u32 {
        self.beat_count.load(Ordering::Relaxed)
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
        // `.max(1)`: `chunks_mut(0)` panics, and the audio callback must never
        // panic (see AGENTS.md). Guards a device that reports zero channels.
        let channels = (cfg.channels as usize).max(1);
        let control = self.control.clone();
        let beat_count = self.beat_count.clone();
        let sound = self.sound.clone();

        // DSP state owned by the callback.
        let mut samples_since_beat = f64::MAX; // trigger immediately when running
        let mut beat_index: u32 = 0;
        let mut tick: u32 = 0; // monotonic beat counter reported to the UI
        let mut prev_running = false;
        let mut env: f32 = 0.0;
        let mut phase: f32 = 0.0;
        let mut freq: f32 = 0.0;
        let mut voice = Voice::Beep; // which click is currently sounding
        let mut rng: u32 = 0x9E37_79B9; // xorshift state for the mechanical noise
        // Bell accent layer (mechanical downbeat), summed on top of the tick.
        let mut bell_env: f32 = 0.0;
        let mut bell_phase: f32 = 0.0;

        let err_fn = |e| log::error!("metronome: stream error: {e}");
        let decay_elec = 1.0 / (0.035 * sample_rate); // ~35 ms electronic click
        let decay_mech = 1.0 / (0.012 * sample_rate); // ~12 ms mechanical tick
        let bell_freq = 2000.0f32; // bright, metallic bike-bell fundamental
        // Bell: multiplicative (exponential) decay that rings for ~450 ms.
        let bell_decay = 0.0008f32.powf(1.0 / (0.45 * sample_rate));

        device
            .build_output_stream(
                cfg,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    // Lock-free reads of the latest UI settings — no lock to
                    // block on or poison, so nothing to fail and no silence
                    // fallback. Read once per buffer into locals.
                    let running = control.running.load(Ordering::Relaxed);
                    let bpm = control.bpm.load(Ordering::Relaxed);
                    let beats = control.beats.load(Ordering::Relaxed);
                    let sound = Sound::from_u8(sound.load(Ordering::Relaxed));
                    let spb = (sample_rate as f64) * 60.0 / bpm.max(1) as f64;

                    if running && !prev_running {
                        samples_since_beat = spb; // downbeat on the next sample
                        beat_index = 0;
                    }
                    prev_running = running;

                    for frame in data.chunks_mut(channels) {
                        if running {
                            samples_since_beat += 1.0;
                            if samples_since_beat >= spb {
                                samples_since_beat -= spb;
                                let downbeat = beat_index == 0;
                                let (beat_voice, beat_freq) = match (sound, downbeat) {
                                    (Sound::Electronic, true) => (Voice::Beep, 1000.0),
                                    (Sound::Electronic, false) => (Voice::Beep, 800.0),
                                    // Mechanical: the wooden tick is the same on
                                    // every beat; the downbeat's accent is the
                                    // bell layer triggered below.
                                    (Sound::Mechanical, _) => (Voice::Tick, 1500.0),
                                };
                                voice = beat_voice;
                                freq = beat_freq;
                                env = 1.0;
                                phase = 0.0;

                                // Ring the bike bell on the mechanical downbeat,
                                // mixed on top of the tick.
                                if downbeat && matches!(sound, Sound::Mechanical) {
                                    bell_env = 1.0;
                                    bell_phase = 0.0;
                                }
                                tick = tick.wrapping_add(1);
                                beat_count.store(tick, Ordering::Relaxed);
                                beat_index = (beat_index + 1) % beats.max(1);
                            }
                        }

                        // Primary click (the "regular" sound): beep or wooden tick.
                        let click = if env > 0.0 {
                            phase += freq / sample_rate;
                            let ph = phase * std::f32::consts::TAU;
                            let v = match voice {
                                Voice::Beep => ph.sin() * env * 0.6,
                                Voice::Tick => {
                                    // Broadband noise gives the tick its "wooden
                                    // knock" character; the pitched body adds a
                                    // little resonance.
                                    let noise = noise_sample(&mut rng);
                                    (ph.sin() * 0.5 + noise * 0.5) * env * 0.7
                                }
                            };
                            match voice {
                                Voice::Beep => env -= decay_elec,
                                Voice::Tick => env -= decay_mech,
                            }
                            v
                        } else {
                            0.0
                        };

                        // Bike-bell accent, summed on top of the mechanical
                        // downbeat: bright inharmonic partials with a slow-beating
                        // shimmer (the near-unison 5.40/5.405 pair), ringing out
                        // over the tick.
                        let bell = if bell_env > 0.0 {
                            bell_phase += bell_freq / sample_rate;
                            let bp = bell_phase * std::f32::consts::TAU;
                            let ring = bp.sin()
                                + 0.7 * (bp * 2.76).sin()
                                + 0.5 * (bp * 5.40).sin()
                                + 0.5 * (bp * 5.405).sin()
                                + 0.3 * (bp * 8.93).sin();
                            let v = ring * 0.15 * bell_env;
                            bell_env *= bell_decay;
                            if bell_env < 0.0008 {
                                bell_env = 0.0;
                            }
                            v
                        } else {
                            0.0
                        };

                        // Sum the layers; clamp guards the rare transient overlap.
                        let out = T::from_sample((click + bell).clamp(-1.0, 1.0));
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

/// One white-noise sample in [-1, 1] from a fast xorshift PRNG — allocation-free
/// and audio-thread safe. `state` must stay non-zero.
fn noise_sample(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{BEATS_MAX, BPM_MAX};

    #[test]
    fn set_stores_clamped_control() {
        let m = Metronome::new();
        m.set(150, 3, true);
        assert_eq!(m.control.bpm.load(Ordering::Relaxed), 150);
        assert_eq!(m.control.beats.load(Ordering::Relaxed), 3);
        assert!(m.control.running.load(Ordering::Relaxed));

        // Out-of-range inputs are clamped before being stored.
        m.set(9999, 99, false);
        assert_eq!(m.control.bpm.load(Ordering::Relaxed), BPM_MAX);
        assert_eq!(m.control.beats.load(Ordering::Relaxed), BEATS_MAX);
        assert!(!m.control.running.load(Ordering::Relaxed));

        // Sound is a separate setting with its own setter.
        m.set_sound(Sound::Mechanical);
        assert_eq!(
            Sound::from_u8(m.sound.load(Ordering::Relaxed)),
            Sound::Mechanical
        );
    }
}
