//! Top-level application shell. Models the Korg TM-60: tuner and metronome run
//! independently and are shown at the same time, driven by the audio engine.

use serde::{Deserialize, Serialize};

use crate::audio::AudioEngine;
use crate::note::NoteReading;
use crate::{theme, ui};

pub struct TunerState {
    /// A4 reference frequency (calibration), typically 440 Hz.
    pub a4: f32,
    /// Latest pitch reading, if a note is currently detected.
    pub reading: Option<NoteReading>,
}

pub struct MetronomeState {
    pub bpm: u32,
    pub beats_per_bar: u32,
    pub running: bool,
    /// Which beat (0-based) is currently sounding / flashing.
    pub current_beat: u32,
}

/// Persisted user settings (bpm/beats/A4/tuner toggle), stored via eframe.
#[derive(Serialize, Deserialize)]
struct Settings {
    bpm: u32,
    beats_per_bar: u32,
    a4: f32,
    tuner_on: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bpm: 120,
            beats_per_bar: 4,
            a4: 440.0,
            tuner_on: true,
        }
    }
}

pub struct Tm2077App {
    /// Whether the tuner is listening (the TUNER ON toggle).
    pub tuner_on: bool,
    /// Whether the reference tone (SOUND) is playing.
    pub sound_on: bool,
    pub tuner: TunerState,
    pub metronome: MetronomeState,
    audio: AudioEngine,
    /// Recent TAP TEMPO timestamps (seconds).
    tap_times: Vec<f64>,
}

impl Tm2077App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let s = cc
            .storage
            .and_then(|s| eframe::get_value::<Settings>(s, eframe::APP_KEY))
            .unwrap_or_default();

        Self {
            tuner_on: s.tuner_on,
            sound_on: false,
            tuner: TunerState {
                a4: s.a4,
                reading: None,
            },
            metronome: MetronomeState {
                bpm: s.bpm,
                beats_per_bar: s.beats_per_bar,
                running: false,
                current_beat: 0,
            },
            audio: AudioEngine::new(),
            tap_times: Vec::new(),
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            bpm: self.metronome.bpm,
            beats_per_bar: self.metronome.beats_per_bar,
            a4: self.tuner.a4,
            tuner_on: self.tuner_on,
        }
    }

    /// Register a TAP TEMPO tap at time `now` (seconds) and update the bpm from
    /// the average of recent tap intervals.
    pub fn tap_tempo(&mut self, now: f64) {
        // Restart the average if the previous tap was long ago.
        if let Some(&last) = self.tap_times.last()
            && now - last > 2.0
        {
            self.tap_times.clear();
        }
        self.tap_times.push(now);
        let n = self.tap_times.len();
        if n > 4 {
            self.tap_times.drain(0..n - 4);
        }
        // Average the recent tap intervals. `first`/`last` are only `Some` once
        // there are at least two taps, which is exactly when an interval exists.
        if let (Some(&first), Some(&last)) = (self.tap_times.first(), self.tap_times.last()) {
            let intervals = self.tap_times.len() as f64 - 1.0;
            if intervals >= 1.0 {
                let avg = (last - first) / intervals;
                if avg > 0.0 {
                    self.metronome.bpm = ((60.0 / avg).round() as i64).clamp(30, 300) as u32;
                }
            }
        }
    }
}

impl eframe::App for Tm2077App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint();

        // Any click counts as a user gesture (needed to unlock web audio).
        if ui.input(|i| i.pointer.any_click()) {
            self.audio.on_user_gesture();
        }

        // Pull the latest audio state into the display model.
        self.audio.poll();
        self.metronome.current_beat = self.audio.metronome_beat();
        self.tuner.reading = if self.tuner_on {
            self.audio.tuner_reading()
        } else {
            None
        };

        // Draw + handle controls (may toggle running, change bpm, a4, tap, …).
        ui::draw_device(ui, self);

        // Push the (possibly updated) UI settings back to the engine.
        self.audio.metronome_set(
            self.metronome.bpm,
            self.metronome.beats_per_bar,
            self.metronome.running,
        );
        self.audio.tuner_set_enabled(self.tuner_on);
        self.audio.tuner_set_a4(self.tuner.a4);
        self.audio
            .set_reference_tone(self.sound_on.then_some(self.tuner.a4));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.settings());
    }

    /// Required on web so `WebRunner` can hand back a handle to the app.
    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(&mut *self)
    }
}
