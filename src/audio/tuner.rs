//! Tuner front-end: delegates to a platform backend (cpal input on native,
//! getUserMedia + AnalyserNode on web) and exposes the latest note reading.

use crate::note::NoteReading;

#[cfg(not(target_arch = "wasm32"))]
use crate::audio::tuner_native::NativeTuner as Backend;
#[cfg(target_arch = "wasm32")]
use crate::audio::tuner_web::WebTuner as Backend;

pub struct Tuner {
    backend: Backend,
}

impl Tuner {
    pub fn new() -> Self {
        Self {
            backend: Backend::new(),
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.backend.set_enabled(on);
    }

    pub fn set_a4(&mut self, a4: f32) {
        self.backend.set_a4(a4);
    }

    pub fn set_scale(&mut self, scale: crate::note::Scale) {
        self.backend.set_scale(scale);
    }

    pub fn reading(&self) -> Option<NoteReading> {
        self.backend.reading()
    }

    pub fn poll(&mut self) {
        self.backend.poll();
    }

    pub fn on_user_gesture(&mut self) {
        self.backend.on_user_gesture();
    }
}
