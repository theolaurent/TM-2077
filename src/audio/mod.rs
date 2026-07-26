//! Cross-platform audio: one `AudioEngine` the app owns, dispatching to a
//! cpal-based metronome (native + web) and a platform-specific tuner.

mod metronome;
mod pitch;
mod tuner;

#[cfg(not(target_arch = "wasm32"))]
mod tuner_native;
#[cfg(target_arch = "wasm32")]
mod tuner_web;

use crate::note::NoteReading;

pub struct AudioEngine {
    metronome: metronome::Metronome,
    tuner: tuner::Tuner,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            metronome: metronome::Metronome::new(),
            tuner: tuner::Tuner::new(),
        }
    }

    // --- Metronome ---
    pub fn metronome_set(&self, bpm: u32, beats: u32, running: bool) {
        self.metronome.set(bpm, beats, running);
    }

    pub fn metronome_beat_count(&self) -> u32 {
        self.metronome.beat_count()
    }

    /// Play (or stop) a continuous reference tone through the output stream.
    pub fn set_reference_tone(&self, freq: Option<f32>) {
        self.metronome.set_tone(freq);
    }

    // --- Tuner ---
    pub fn tuner_set_enabled(&mut self, on: bool) {
        self.tuner.set_enabled(on);
    }

    pub fn tuner_set_a4(&mut self, a4: f32) {
        self.tuner.set_a4(a4);
    }

    pub fn tuner_reading(&self) -> Option<NoteReading> {
        self.tuner.reading()
    }

    // --- Lifecycle ---
    /// Called once per frame to advance any polling-based work (web tuner).
    pub fn poll(&mut self) {
        self.tuner.poll();
    }

    /// Unlock/resume audio. Required on web from within a user gesture.
    pub fn on_user_gesture(&mut self) {
        self.metronome.ensure_started();
        self.tuner.on_user_gesture();
    }
}
