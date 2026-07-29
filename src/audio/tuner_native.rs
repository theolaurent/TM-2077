//! Native tuner backend: a cpal input stream forwards recent mic samples over a
//! bounded SPSC channel to `poll()`, which reassembles them into a rolling
//! window and runs the shared pitch detector. Message passing (not a shared,
//! locked buffer) keeps the audio callback lock-free and allocation-free.

use std::sync::mpsc::{Receiver, sync_channel};

use anyhow::{Context as _, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};

use crate::audio::pitch::{PitchTracker, WINDOW};
use crate::note::{NoteReading, Scale, Transposition};

/// How many recent samples the consumer keeps available to the detector (twice
/// the analysis window, for slack).
const RING_CAP: usize = WINDOW * 2;

/// Samples per message block handed from the audio callback to the consumer.
const BLOCK: usize = 128;

/// Channel depth in blocks (~170 ms at 48 kHz) — ample slack between UI frames,
/// so blocks are only ever dropped if `poll` stalls (e.g. a minimised window).
const CHANNEL_BLOCKS: usize = 64;

pub struct NativeTuner {
    enabled: bool,
    a4: f32,
    scale: Scale,
    transpose: Transposition,
    reading: Option<NoteReading>,
    /// Consumer half of the sample channel: fixed-size blocks sent by the
    /// audio-input callback. `None` until the stream is built.
    rx: Option<Receiver<[f32; BLOCK]>>,
    /// Consumer-owned rolling window (UI thread only) the detector reads from.
    window: Vec<f32>,
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
            transpose: Transposition::default(),
            reading: None,
            rx: None,
            window: Vec::with_capacity(RING_CAP),
            stream: None,
            tracker: None,
            started: false,
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        // Called every frame with the same value; only act on a real transition.
        if on == self.enabled {
            return;
        }
        self.enabled = on;
        if !on {
            self.reading = None;
            // Drop stale samples so a later re-enable starts from fresh audio.
            self.window.clear();
        }
        // Pause the mic capture while the tuner is off — frees the input device
        // and clears the OS "mic in use" indicator — and resume it when the
        // tuner comes back on. The stream only exists once a user gesture has
        // started it (`ensure_started`); before that there is nothing to pause.
        if let Some(stream) = &self.stream {
            // `play`/`pause` return distinct error types, so handle them apart.
            let res = if on {
                stream.play().map_err(|e| e.to_string())
            } else {
                stream.pause().map_err(|e| e.to_string())
            };
            if let Err(e) = res {
                log::error!(
                    "tuner: stream {} failed: {e}",
                    if on { "resume" } else { "pause" }
                );
            }
        }
    }

    pub fn set_a4(&mut self, a4: f32) {
        self.a4 = a4;
    }

    pub fn set_scale(&mut self, scale: Scale) {
        self.scale = scale;
    }

    pub fn set_transpose(&mut self, transpose: Transposition) {
        self.transpose = transpose;
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

        // Drain every block the audio callback has sent since the last frame into
        // the local window, then keep only the most recent `RING_CAP` samples.
        // This runs on the UI thread, so the trim's memmove (and any growth of
        // `window`) is harmless — the audio thread does none of it.
        if let Some(rx) = self.rx.as_ref() {
            while let Ok(block) = rx.try_recv() {
                self.window.extend_from_slice(&block);
            }
            let len = self.window.len();
            if len > RING_CAP {
                self.window.drain(0..len - RING_CAP);
            }
        }

        if let Some(tracker) = self.tracker.as_mut() {
            self.reading = tracker
                .detect(&self.window)
                .and_then(|f| NoteReading::from_freq(f, self.a4, self.scale, self.transpose));
        }
    }

    fn ensure_started(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        match self.build() {
            Ok((stream, sr, rx)) => {
                if let Err(e) = stream.play() {
                    log::error!("tuner: stream.play failed: {e}");
                }
                self.tracker = Some(PitchTracker::new(sr));
                self.rx = Some(rx);
                self.stream = Some(stream);
            }
            Err(e) => log::error!("tuner: mic init failed: {e:#}"),
        }
    }

    fn build(&self) -> Result<(cpal::Stream, u32, Receiver<[f32; BLOCK]>)> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no default input device")?;
        let config = device
            .default_input_config()
            .context("default_input_config")?;
        let sr = config.sample_rate().0;
        let cfg = config.config();

        let (stream, rx) = match config.sample_format() {
            cpal::SampleFormat::F32 => self.build_for::<f32>(&device, &cfg)?,
            cpal::SampleFormat::I16 => self.build_for::<i16>(&device, &cfg)?,
            cpal::SampleFormat::U16 => self.build_for::<u16>(&device, &cfg)?,
            other => anyhow::bail!("unsupported input format: {other:?}"),
        };
        Ok((stream, sr, rx))
    }

    fn build_for<T>(
        &self,
        device: &cpal::Device,
        cfg: &cpal::StreamConfig,
    ) -> Result<(cpal::Stream, Receiver<[f32; BLOCK]>)>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        // `.max(1)`: `chunks(0)` panics, and the audio callback must never panic
        // (see AGENTS.md). Also keeps the mono downmix divisor non-zero.
        let channels = (cfg.channels as usize).max(1);
        // Bounded channel: the ring of `[f32; BLOCK]` slots is pre-allocated here
        // (UI thread), so `try_send` in the callback never allocates.
        let (tx, rx) = sync_channel::<[f32; BLOCK]>(CHANNEL_BLOCKS);

        // Per-callback state owned by the closure: accumulate mono samples into a
        // fixed block, then hand each full block off. No locking, no allocation.
        let mut partial = [0.0f32; BLOCK];
        let mut filled = 0usize;

        let stream = device
            .build_input_stream(
                cfg,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    for frame in data.chunks(channels) {
                        let mono: f32 = frame.iter().map(|&s| f32::from_sample(s)).sum::<f32>()
                            / channels as f32;
                        if let Some(slot) = partial.get_mut(filled) {
                            *slot = mono;
                            filled += 1;
                        }
                        if filled == BLOCK {
                            // Never block the audio thread: drop this block if the
                            // consumer is behind. `partial` is `Copy`, so the send
                            // copies it and we refill from index 0.
                            let _ = tx.try_send(partial);
                            filled = 0;
                        }
                    }
                },
                |e| log::error!("tuner: stream error: {e}"),
                None,
            )
            .context("build_input_stream")?;
        Ok((stream, rx))
    }
}
