//! Cross-platform audio: one `AudioEngine` the app owns, dispatching to a
//! cpal-based metronome (native + web) and a platform-specific tuner.

mod metronome;
mod pitch;

#[cfg(not(target_arch = "wasm32"))]
mod tuner_native;
#[cfg(target_arch = "wasm32")]
mod tuner_web;

// Platform tuner backend used directly (cpal input on native, getUserMedia +
// AnalyserNode on web); `AudioEngine` is the sole facade over it and the metronome.
#[cfg(not(target_arch = "wasm32"))]
use tuner_native::NativeTuner as Tuner;
#[cfg(target_arch = "wasm32")]
use tuner_web::WebTuner as Tuner;

pub use metronome::Sound;

use crate::note::NoteReading;

pub struct AudioEngine {
    metronome: metronome::Metronome,
    tuner: Tuner,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            metronome: metronome::Metronome::new(),
            tuner: Tuner::new(),
        }
    }

    // --- Metronome ---
    pub fn metronome_set(&self, bpm: u32, beats: u32, running: bool) {
        self.metronome.set(bpm, beats, running);
    }

    pub fn metronome_set_sound(&self, sound: Sound) {
        self.metronome.set_sound(sound);
    }

    pub fn metronome_beat_count(&self) -> u32 {
        self.metronome.beat_count()
    }

    // --- Tuner ---
    pub fn tuner_set_enabled(&mut self, on: bool) {
        self.tuner.set_enabled(on);
    }

    pub fn tuner_set_a4(&mut self, a4: f32) {
        self.tuner.set_a4(a4);
    }

    pub fn tuner_set_scale(&mut self, scale: crate::note::Scale) {
        self.tuner.set_scale(scale);
    }

    pub fn tuner_set_transpose(&mut self, transpose: crate::note::Transposition) {
        self.tuner.set_transpose(transpose);
    }

    pub fn tuner_reading(&self) -> Option<NoteReading> {
        self.tuner.reading()
    }

    // --- Lifecycle ---
    /// Advance any polling-based work (web tuner). Called once per frame.
    pub fn poll(&mut self) {
        self.tuner.poll();
    }

    /// Unlock/resume audio. Required on web from within a user gesture.
    pub fn on_user_gesture(&mut self) {
        self.metronome.ensure_started();
        self.tuner.on_user_gesture();
    }
}
