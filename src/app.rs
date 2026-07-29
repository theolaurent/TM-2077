//! Top-level application shell. Models the Korg TM-60: tuner and metronome run
//! independently and are shown at the same time, driven by the audio engine.

use rpds::Vector;
use serde::{Deserialize, Serialize};

use crate::audio::{AudioEngine, Sound};
use crate::note::{NoteReading, Scale, Transposition};
use crate::{theme, ui};

pub struct TunerState {
    /// A4 reference frequency (calibration), typically 440 Hz.
    pub a4: f32,
    /// Which scale the tuner snaps readings to.
    pub scale: Scale,
    /// Instrument transposition (shows the written note).
    pub transpose: Transposition,
    /// Latest pitch reading, if a note is currently detected.
    pub reading: Option<NoteReading>,
}

pub struct MetronomeState {
    pub bpm: u32,
    pub beats_per_bar: u32,
    pub running: bool,
    /// Monotonic beat counter from the audio engine; the UI watches it change.
    pub beat_count: u32,
    /// TEMPO up/down steps through old-school graduations instead of by 1 bpm.
    pub graduated: bool,
    /// Which click sound the metronome plays.
    pub sound: Sound,
}

/// Persisted user settings (bpm/beats/A4/tuner toggle), stored via eframe.
#[derive(Serialize, Deserialize)]
struct Settings {
    bpm: u32,
    beats_per_bar: u32,
    a4: f32,
    tuner_on: bool,
    // `serde(default)` keeps older saved settings (without these fields) loadable.
    #[serde(default)]
    theme: theme::Theme,
    #[serde(default)]
    scale: Scale,
    #[serde(default)]
    transpose: Transposition,
    #[serde(default)]
    tempo_graduated: bool,
    #[serde(default)]
    sound: Sound,
}

/// How many recent taps TAP TEMPO averages into a bpm.
const TAP_COUNT: usize = 4;

// Shared value ranges, so one bound lives in one place instead of being repeated
// (and drifting) across the UI, the tap-tempo maths and the audio engine.

/// Tempo bounds enforced by every UI path (manual TEMPO rocker, TAP TEMPO). The
/// audio engine clamps to the same range as a defensive net.
pub(crate) const BPM_MIN: u32 = 30;
pub(crate) const BPM_MAX: u32 = 300;

/// Beats-per-bar bounds.
pub(crate) const BEATS_MIN: u32 = 1;
pub(crate) const BEATS_MAX: u32 = 12;

/// A4 calibration bounds (Hz) for the CALIB rocker.
pub(crate) const A4_MIN: f32 = 410.0;
pub(crate) const A4_MAX: f32 = 480.0;

impl Default for Settings {
    fn default() -> Self {
        Self {
            bpm: 120,
            beats_per_bar: 4,
            a4: 440.0,
            tuner_on: false,
            theme: theme::Theme::default(),
            scale: Scale::default(),
            transpose: Transposition::default(),
            tempo_graduated: false,
            sound: Sound::default(),
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
    tap_times: Vector<f64>, // TODO: should that be a rpds::Queue?
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
                // Clamp persisted values on load: an older or hand-edited save
                // must not seed an out-of-range (or non-finite) calibration.
                a4: if s.a4.is_finite() {
                    s.a4.clamp(A4_MIN, A4_MAX)
                } else {
                    440.0
                },
                scale: s.scale,
                transpose: s.transpose,
                reading: None,
            },
            metronome: MetronomeState {
                bpm: s.bpm.clamp(BPM_MIN, BPM_MAX),
                beats_per_bar: s.beats_per_bar.clamp(BEATS_MIN, BEATS_MAX),
                running: false,
                beat_count: 0,
                graduated: s.tempo_graduated,
                sound: s.sound,
            },
            audio: AudioEngine::new(),
            tap_times: Vector::new(),
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
            theme: self.theme,
            scale: self.tuner.scale,
            transpose: self.tuner.transpose,
            tempo_graduated: self.metronome.graduated,
            sound: self.metronome.sound,
        }
    }

    /// Register a TAP TEMPO tap at time `now` (seconds) and update the bpm from
    /// the average of recent tap intervals.
    pub fn tap_tempo(&mut self, now: f64) {
        // Start a fresh sequence if the previous tap was long ago, otherwise keep
        // the recent history — then append `now` and keep only the last 4 taps.
        // All of this builds new persistent vectors rather than mutating in place.
        let restart = matches!(self.tap_times.last(), Some(&last) if now - last > 2.0);
        let base = if restart {
            Vector::new()
        } else {
            self.tap_times.clone()
        };
        let taps = keep_last(&base.push_back(now), TAP_COUNT);

        if let Some(bpm) = tapped_bpm(&taps) {
            self.metronome.bpm = bpm;
        }
        self.tap_times = taps;
    }

    /// The settings popup (opened by the gear button), styled to match the
    /// device. Kept intentionally small for now.
    fn settings_ui(&mut self, ctx: &egui::Context, was_open: bool) {
        if !self.settings_open {
            return;
        }
        let pal = self.theme.palette();
        let frame = egui::Frame::default()
            .inner_margin(egui::Margin::same(16))
            .fill(pal.body)
            .stroke(egui::Stroke::new(1.0, pal.body_edge_hi))
            .corner_radius(egui::CornerRadius::same(10));

        let mut close = false;
        let inner = egui::Window::new("settings")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
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

                // THEME label with the close cross pushed to the top-right of the
                // same row, so the cross doesn't claim a row of its own.
                ui.horizontal(|ui| {
                    ui.label("THEME");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rect, resp) =
                            ui.allocate_exact_size(egui::vec2(15.0, 15.0), egui::Sense::click());
                        let col = if resp.hovered() {
                            pal.body_label
                        } else {
                            pal.body_label_dim
                        };
                        let stroke = egui::Stroke::new(1.6, col);
                        let pad = egui::vec2(4.0, 4.0);
                        ui.painter().line_segment(
                            [rect.left_top() + pad, rect.right_bottom() - pad],
                            stroke,
                        );
                        ui.painter().line_segment(
                            [
                                rect.right_top() + egui::vec2(-pad.x, pad.y),
                                rect.left_bottom() + egui::vec2(pad.x, -pad.y),
                            ],
                            stroke,
                        );
                        close |= resp.clicked();
                    });
                });
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.theme, theme::Theme::Dark, "Dark");
                    ui.selectable_value(&mut self.theme, theme::Theme::Light, "Light");
                });

                ui.add_space(8.0);
                ui.label("SCALE");
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.tuner.scale, Scale::Chromatic, "Chromatic");
                    ui.selectable_value(&mut self.tuner.scale, Scale::Guitar, "Guitar");
                    ui.selectable_value(&mut self.tuner.scale, Scale::QuarterTone, "Quarter Tones");
                });

                ui.add_space(8.0);
                ui.label("TRANSPOSITION");
                ui.horizontal_wrapped(|ui| {
                    let tr = &mut self.tuner.transpose;
                    ui.selectable_value(tr, Transposition::Concert, "Concert");
                    ui.selectable_value(tr, Transposition::BFlat, "Bb");
                    ui.selectable_value(tr, Transposition::EFlat, "Eb");
                    ui.selectable_value(tr, Transposition::F, "F");
                });

                ui.add_space(8.0);
                ui.label("TEMPO STEP");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.metronome.graduated, false, "1 bpm");
                    ui.selectable_value(&mut self.metronome.graduated, true, "Graduated");
                });

                ui.add_space(8.0);
                ui.label("SOUND");
                ui.horizontal(|ui| {
                    let s = &mut self.metronome.sound;
                    ui.selectable_value(s, Sound::Electronic, "Electronic");
                    ui.selectable_value(s, Sound::Mechanical, "Vintage");
                });
            });

        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
        }

        // A click anywhere outside the popup closes it — but not on the frame it
        // was opened, so the opening click doesn't immediately dismiss it.
        if was_open {
            let win_rect = inner.map(|r| r.response.rect);
            let close_it = ctx.input(|i| {
                i.pointer.any_click()
                    && win_rect.is_none_or(|wr| {
                        i.pointer.interact_pos().is_none_or(|pos| !wr.contains(pos))
                    })
            });
            if close_it {
                self.settings_open = false;
            }
        }
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
        .map(|avg| ((60.0 / avg).round() as i64).clamp(BPM_MIN as i64, BPM_MAX as i64) as u32)
}

/// Traditional Maelzel metronome graduations (40–208 bpm): dense at the low end,
/// coarser as the tempo rises.
const GRADUATIONS: [u32; 39] = [
    40, 42, 44, 46, 48, 50, 52, 54, 56, 58, 60, 63, 66, 69, 72, 76, 80, 84, 88, 92, 96, 100, 104,
    108, 112, 116, 120, 126, 132, 138, 144, 152, 160, 168, 176, 184, 192, 200, 208,
];

/// The next graduation strictly above `bpm` (unchanged if already at/above the top).
pub(crate) fn next_graduation(bpm: u32) -> u32 {
    GRADUATIONS
        .iter()
        .copied()
        .find(|&g| g > bpm)
        .unwrap_or(bpm)
}

/// The previous graduation strictly below `bpm` (unchanged if already at/below the bottom).
pub(crate) fn prev_graduation(bpm: u32) -> u32 {
    GRADUATIONS
        .iter()
        .rev()
        .copied()
        .find(|&g| g < bpm)
        .unwrap_or(bpm)
}

impl eframe::App for Tm2077App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Apply the active theme (device palette + egui base visuals) up front.
        theme::apply(ui.ctx(), self.theme);

        // Only drive continuous repaints when something is actually animating
        // (beat dots / needle). Otherwise let egui idle and repaint on input,
        // saving CPU/GPU — especially in a backgrounded web tab. The metronome
        // audio is unaffected; it runs on the audio thread.
        // Repaint continuously while animating, and while a pointer button is
        // held so the rockers' press-and-hold auto-repeat keeps ticking.
        if self.metronome.running || self.tuner_on || ui.input(|i| i.pointer.any_down()) {
            ui.ctx().request_repaint();
        }

        // Any click counts as a user gesture (needed to unlock web audio / mic).
        // Handled at the end of the frame, *after* controls and settings run, so
        // the same click that turns the tuner on requests the mic in that gesture
        // instead of needing a second click.
        let clicked = ui.input(|i| i.pointer.any_click());
        // Whether the settings popup was already open *before* this frame's
        // clicks — so the click that opens it doesn't also close it.
        let settings_was_open = self.settings_open;

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
        self.audio.metronome_set_sound(self.metronome.sound);
        self.audio.tuner_set_enabled(self.tuner_on);
        self.audio.tuner_set_a4(self.tuner.a4);
        self.audio.tuner_set_scale(self.tuner.scale);
        self.audio.tuner_set_transpose(self.tuner.transpose);

        // Now that this frame's settings are live, act on any user gesture — so a
        // click that just enabled the tuner unlocks audio and prompts for the mic.
        if clicked {
            self.audio.on_user_gesture();
        }

        self.settings_ui(ui.ctx(), settings_was_open);
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
        assert_eq!(
            keep_last(&v(&[1.0, 2.0, 3.0, 4.0, 5.0]), 3),
            v(&[3.0, 4.0, 5.0])
        );
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
    fn graduations_step_up_and_down() {
        assert_eq!(next_graduation(120), 126);
        assert_eq!(prev_graduation(120), 116);
        assert_eq!(next_graduation(60), 63);
        assert_eq!(prev_graduation(63), 60);
        assert_eq!(next_graduation(150), 152); // between graduations
        assert_eq!(prev_graduation(150), 144);
        assert_eq!(next_graduation(208), 208); // stays at the top
        assert_eq!(prev_graduation(40), 40); // stays at the bottom
        assert_eq!(next_graduation(250), 250); // above the scale: up stays
        assert_eq!(prev_graduation(250), 208); // above the scale: down enters
    }

    #[test]
    fn tapped_bpm_clamps_to_range() {
        assert_eq!(tapped_bpm(&v(&[0.0, 3.0])), Some(30)); // 20 bpm → clamped up
        assert_eq!(tapped_bpm(&v(&[0.0, 0.1])), Some(300)); // 600 bpm → clamped down
    }
}
