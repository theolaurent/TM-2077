//! Native tuner backend: a cpal input stream feeds recent mic samples into a
//! shared ring buffer that `poll()` drains through the pitch detector.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};

use crate::audio::pitch::{PitchTracker, WINDOW};
use crate::note::NoteReading;

const RING_CAP: usize = WINDOW * 2;

pub struct NativeTuner {
    enabled: bool,
    a4: f32,
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
            let buf = {
                let r = self.ring.lock().unwrap();
                r.clone()
            };
            self.reading = tracker
                .detect(&buf)
                .and_then(|f| NoteReading::from_freq(f, self.a4));
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
            Err(e) => log::error!("tuner: mic init failed: {e}"),
        }
    }

    fn build(&self) -> Result<(cpal::Stream, u32), String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("no default input device")?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("default_input_config: {e}"))?;
        let sr = config.sample_rate().0;
        let cfg = config.config();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => self.build_for::<f32>(&device, &cfg)?,
            cpal::SampleFormat::I16 => self.build_for::<i16>(&device, &cfg)?,
            cpal::SampleFormat::U16 => self.build_for::<u16>(&device, &cfg)?,
            other => return Err(format!("unsupported input format: {other:?}")),
        };
        Ok((stream, sr))
    }

    fn build_for<T>(&self, device: &cpal::Device, cfg: &cpal::StreamConfig) -> Result<cpal::Stream, String>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let channels = cfg.channels as usize;
        let ring = self.ring.clone();
        device
            .build_input_stream(
                cfg,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    let mut r = ring.lock().unwrap();
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
            .map_err(|e| format!("build_input_stream: {e}"))
    }
}
