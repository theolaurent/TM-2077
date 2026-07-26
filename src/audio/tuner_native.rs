//! Native tuner backend: a cpal input stream feeds recent mic samples into a
//! shared ring buffer that `poll()` drains through the pitch detector.

use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};

use crate::audio::pitch::{PitchTracker, WINDOW};
use crate::note::{NoteReading, Scale};

const RING_CAP: usize = WINDOW * 2;

pub struct NativeTuner {
    enabled: bool,
    a4: f32,
    scale: Scale,
    reading: Option<NoteReading>,
    ring: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
    tracker: Option<PitchTracker>,
    started: bool,
}

impl NativeTuner {
    pub fn new() -> Self {
        Self {
            enabled: false,
            a4: 440.0,
            scale: Scale::default(),
            reading: None,
            ring: Arc::new(Mutex::new(Vec::with_capacity(RING_CAP))),
            stream: None,
            tracker: None,
            started: false,
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.reading = None;
        }
    }

    pub fn set_a4(&mut self, a4: f32) {
        self.a4 = a4;
    }

    pub fn set_scale(&mut self, scale: Scale) {
        self.scale = scale;
    }

    pub fn reading(&self) -> Option<NoteReading> {
        self.reading
    }

    pub fn on_user_gesture(&mut self) {
        if self.enabled {
            self.ensure_started();
        }
    }

    pub fn poll(&mut self) {
        if !self.enabled {
            self.reading = None;
            return;
        }
        self.ensure_started();

        if let Some(tracker) = self.tracker.as_mut() {
            // Skip this frame (keep the last reading) if the lock is poisoned,
            // rather than panicking.
            let Ok(buf) = self.ring.lock().map(|r| r.clone()) else {
                return;
            };
            self.reading = tracker
                .detect(&buf)
                .and_then(|f| NoteReading::from_freq(f, self.a4, self.scale));
        }
    }

    fn ensure_started(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        match self.build() {
            Ok((stream, sr)) => {
                if let Err(e) = stream.play() {
                    log::error!("tuner: stream.play failed: {e}");
                }
                self.tracker = Some(PitchTracker::new(sr));
                self.stream = Some(stream);
            }
            Err(e) => log::error!("tuner: mic init failed: {e:#}"),
        }
    }

    fn build(&self) -> Result<(cpal::Stream, u32)> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no default input device")?;
        let config = device
            .default_input_config()
            .context("default_input_config")?;
        let sr = config.sample_rate().0;
        let cfg = config.config();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => self.build_for::<f32>(&device, &cfg)?,
            cpal::SampleFormat::I16 => self.build_for::<i16>(&device, &cfg)?,
            cpal::SampleFormat::U16 => self.build_for::<u16>(&device, &cfg)?,
            other => anyhow::bail!("unsupported input format: {other:?}"),
        };
        Ok((stream, sr))
    }

    fn build_for<T>(&self, device: &cpal::Device, cfg: &cpal::StreamConfig) -> Result<cpal::Stream>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        // `.max(1)`: `chunks(0)` panics, and the audio callback must never panic
        // (see AGENTS.md). Also keeps the mono downmix divisor non-zero.
        let channels = (cfg.channels as usize).max(1);
        let ring = self.ring.clone();
        device
            .build_input_stream(
                cfg,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    // Never panic in the audio callback: drop this buffer if the
                    // lock is poisoned.
                    let Ok(mut r) = ring.lock() else {
                        return;
                    };
                    for frame in data.chunks(channels) {
                        let mono: f32 =
                            frame.iter().map(|&s| f32::from_sample(s)).sum::<f32>() / channels as f32;
                        r.push(mono);
                    }
                    let len = r.len();
                    if len > RING_CAP {
                        r.drain(0..len - RING_CAP);
                    }
                },
                |e| log::error!("tuner: stream error: {e}"),
                None,
            )
            .context("build_input_stream")
    }
}
