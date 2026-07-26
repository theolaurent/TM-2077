//! Top-level application shell. Models the Korg TM-60: tuner and metronome run
//! independently and are shown at the same time, driven by the audio engine.

use rpds::Vector;
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
    /// Monotonic beat counter from the audio engine; the UI watches it change.
    pub beat_count: u32,
}

/// Persisted user settings (bpm/beats/A4/tuner toggle), stored via eframe.
#[derive(Serialize, Deserialize)]
struct Settings {
    bpm: u32,
    beats_per_bar: u32,
    a4: f32,
    tuner_on: bool,
    // `serde(default)` keeps older saved settings (without these fields) loadable.
    #[serde(default = "default_tap_count")]
    tap_count: u32,
    #[serde(default)]
    theme: theme::Theme,
}

fn default_tap_count() -> u32 {
    4
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bpm: 120,
            beats_per_bar: 4,
            a4: 440.0,
            tuner_on: false,
            tap_count: default_tap_count(),
            theme: theme::Theme::default(),
        }
    }
}

pub struct Tm2077App {
    /// Whether the tuner is listening (the TUNER ON toggle).
    pub tuner_on: bool,
    pub tuner: TunerState,
    pub metronome: MetronomeState,
    audio: AudioEngine,
    /// Recent TAP TEMPO timestamps (seconds), as a persistent vector.
    tap_times: Vector<f64>,
    /// How many recent taps TAP TEMPO averages into a bpm.
    tap_count: u32,
    /// Light/dark device theme.
    theme: theme::Theme,
    /// Whether the settings popup is open.
    pub settings_open: bool,
}

impl Tm2077App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let s = cc
            .storage
            .and_then(|s| eframe::get_value::<Settings>(s, eframe::APP_KEY))
            .unwrap_or_default();

        Self {
            tuner_on: s.tuner_on,
            tuner: TunerState {
                a4: s.a4,
                reading: None,
            },
            metronome: MetronomeState {
                bpm: s.bpm,
                beats_per_bar: s.beats_per_bar,
                running: false,
                beat_count: 0,
            },
            audio: AudioEngine::new(),
            tap_times: Vector::new(),
            tap_count: s.tap_count.clamp(2, 8),
            theme: s.theme,
            settings_open: false,
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            bpm: self.metronome.bpm,
            beats_per_bar: self.metronome.beats_per_bar,
            a4: self.tuner.a4,
            tuner_on: self.tuner_on,
            tap_count: self.tap_count,
            theme: self.theme,
        }
    }

    /// Register a TAP TEMPO tap at time `now` (seconds) and update the bpm from
    /// the average of recent tap intervals.
    pub fn tap_tempo(&mut self, now: f64) {
        // Start a fresh sequence if the previous tap was long ago, otherwise keep
        // the recent history — then append `now` and keep only the last 4 taps.
        // All of this builds new persistent vectors rather than mutating in place.
        let restart = matches!(self.tap_times.last(), Some(&last) if now - last > 2.0);
        let base = if restart { Vector::new() } else { self.tap_times.clone() };
        let taps = keep_last(&base.push_back(now), self.tap_count.max(2) as usize);

        if let Some(bpm) = tapped_bpm(&taps) {
            self.metronome.bpm = bpm;
        }
        self.tap_times = taps;
    }

    /// The settings popup (opened by the gear button), styled to match the
    /// device. Kept intentionally small for now.
    fn settings_ui(&mut self, ctx: &egui::Context) {
        let pal = self.theme.palette();
        let frame = egui::Frame::default()
            .inner_margin(egui::Margin::same(16))
            .fill(pal.body)
            .stroke(egui::Stroke::new(1.0, pal.body_edge_hi))
            .corner_radius(egui::CornerRadius::same(10));

        let mut open = self.settings_open;
        egui::Window::new("SETTINGS")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .frame(frame)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                // Amber accents / device labels for the inner widgets.
                let v = ui.visuals_mut();
                v.override_text_color = Some(pal.body_label);
                v.widgets.inactive.bg_fill = pal.btn;
                v.widgets.inactive.weak_bg_fill = pal.btn;
                v.widgets.hovered.bg_fill = pal.btn_hi;
                v.widgets.hovered.weak_bg_fill = pal.btn_hi;
                v.widgets.active.bg_fill = pal.btn_lo;
                v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, pal.btn_label);
                v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, pal.body_label);
                v.selection.bg_fill = pal.lcd_bg;
                v.selection.stroke = egui::Stroke::new(1.0, pal.lcd_ink);

                ui.label("THEME");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.theme, theme::Theme::Dark, "Dark");
                    ui.selectable_value(&mut self.theme, theme::Theme::Light, "Light");
                });

                ui.add_space(8.0);
                ui.label("TAP TEMPO");
                ui.add(egui::Slider::new(&mut self.tap_count, 2..=8).text("taps averaged"));
                ui.small("How many recent taps are averaged into the tempo.");
            });
        self.settings_open = open;
    }
}

/// The last `keep` elements as a new persistent vector. Structural sharing makes
/// both the `clone` and the rebuild cheap.
fn keep_last(taps: &Vector<f64>, keep: usize) -> Vector<f64> {
    match taps.len().checked_sub(keep) {
        Some(skip) if skip > 0 => taps.iter().skip(skip).copied().collect(),
        _ => taps.clone(),
    }
}

/// Average the consecutive tap intervals into a bpm clamped to 30..=300, or
/// `None` when there aren't yet enough taps for a well-defined tempo.
fn tapped_bpm(taps: &Vector<f64>) -> Option<u32> {
    let (&first, &last) = (taps.first()?, taps.last()?);
    let intervals = taps.len().checked_sub(1)? as f64;
    (intervals >= 1.0)
        .then_some((last - first) / intervals)
        .filter(|&avg| avg > 0.0)
        .map(|avg| ((60.0 / avg).round() as i64).clamp(30, 300) as u32)
}

impl eframe::App for Tm2077App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Apply the active theme (device palette + egui base visuals) up front.
        theme::apply(ui.ctx(), self.theme);

        // Only drive continuous repaints when something is actually animating
        // (beat dots / needle). Otherwise let egui idle and repaint on input,
        // saving CPU/GPU — especially in a backgrounded web tab. The metronome
        // audio is unaffected; it runs on the audio thread.
        if self.metronome.running || self.tuner_on {
            ui.ctx().request_repaint();
        }

        // Any click counts as a user gesture (needed to unlock web audio / mic).
        // Handled at the end of the frame, *after* controls and settings run, so
        // the same click that turns the tuner on requests the mic in that gesture
        // instead of needing a second click.
        let clicked = ui.input(|i| i.pointer.any_click());

        // Pull the latest audio state into the display model.
        self.audio.poll();
        self.metronome.beat_count = self.audio.metronome_beat_count();
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

        // Now that this frame's settings are live, act on any user gesture — so a
        // click that just enabled the tuner unlocks audio and prompts for the mic.
        if clicked {
            self.audio.on_user_gesture();
        }

        self.settings_ui(ui.ctx());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[f64]) -> Vector<f64> {
        xs.iter().copied().collect()
    }

    #[test]
    fn keep_last_trims_to_cap() {
        assert_eq!(keep_last(&v(&[1.0, 2.0, 3.0, 4.0, 5.0]), 3), v(&[3.0, 4.0, 5.0]));
        assert_eq!(keep_last(&v(&[1.0, 2.0]), 3), v(&[1.0, 2.0]));
        assert_eq!(keep_last(&v(&[]), 3), v(&[]));
    }

    #[test]
    fn tapped_bpm_needs_two_taps() {
        assert_eq!(tapped_bpm(&v(&[])), None);
        assert_eq!(tapped_bpm(&v(&[1.0])), None);
    }

    #[test]
    fn tapped_bpm_averages_intervals() {
        // 0.5 s between taps → 120 bpm.
        assert_eq!(tapped_bpm(&v(&[0.0, 0.5, 1.0, 1.5])), Some(120));
    }

    #[test]
    fn tapped_bpm_clamps_to_range() {
        assert_eq!(tapped_bpm(&v(&[0.0, 3.0])), Some(30)); // 20 bpm → clamped up
        assert_eq!(tapped_bpm(&v(&[0.0, 0.1])), Some(300)); // 600 bpm → clamped down
    }
}
