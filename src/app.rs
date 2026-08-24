//! Top-level application shell. Models the Korg TM-60: tuner and metronome run
//! independently and are shown at the same time, driven by the audio engine.

use imbl::Vector;
use serde::{Deserialize, Serialize};

use crate::audio::{AudioEngine, Sound};
use crate::note::{Accidentals, NoteReading, Scale, Transposition};
use crate::{theme, ui};

pub struct TunerState {
    /// A4 reference frequency (calibration), typically 440 Hz.
    pub a4: f32,
    /// Which scale the tuner snaps readings to.
    pub scale: Scale,
    /// How the tuner spells the chromatic black keys (sharps / flats / mixed).
    pub accidentals: Accidentals,
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

/// Persisted user settings (bpm/beats/A4/…), stored via eframe.
///
/// `tuner_on` is deliberately *not* persisted: it's a live transport toggle, not
/// a preference, and can't take effect until a user gesture unlocks the mic
/// (autoplay policy) — so restoring it "on" would show an armed-but-silent tuner.
/// The tuner always boots off.
#[derive(Serialize, Deserialize)]
struct Settings {
    bpm: u32,
    beats_per_bar: u32,
    a4: f32,
    // `serde(default)` keeps older saves (lacking these fields) loadable.
    #[serde(default)]
    theme: theme::Theme,
    #[serde(default)]
    scale: Scale,
    #[serde(default)]
    accidentals: Accidentals,
    #[serde(default)]
    transpose: Transposition,
    #[serde(default)]
    tempo_graduated: bool,
    #[serde(default)]
    sound: Sound,
    // Which of `(tuner, metronome)` were last on — what the space bar should
    // bring back. A preference (persisted), not the live transport state.
    #[serde(default = "default_last_on")]
    last_on: (bool, bool),
    // UI zoom factor (scroll / pinch), persisted so the device keeps its size.
    #[serde(default = "default_zoom")]
    zoom: f32,
}

/// Seed for `last_on`: metronome only, so a fresh install's first space press is
/// a plain metronome play/pause.
fn default_last_on() -> (bool, bool) {
    (false, true)
}

/// Seed for `zoom`: 1× (egui's default zoom factor).
fn default_zoom() -> f32 {
    1.0
}

/// How many recent taps TAP TEMPO averages into a bpm.
const TAP_COUNT: usize = 4;

// Shared value ranges: one bound in one place, so the UI, the tap-tempo maths
// and the audio engine can't drift apart.

/// Tempo bounds, enforced by every UI path (TEMPO rocker, TAP TEMPO); the audio
/// engine re-clamps as a defensive net.
pub(crate) const BPM_MIN: u32 = 30;
pub(crate) const BPM_MAX: u32 = 300;

/// Beats-per-bar bounds. 0 = no accented downbeat, just a steady uniform tick.
pub(crate) const BEATS_MIN: u32 = 0;
pub(crate) const BEATS_MAX: u32 = 12;

/// A4 calibration bounds (Hz) for the CALIB rocker.
pub(crate) const A4_MIN: f32 = 410.0;
pub(crate) const A4_MAX: f32 = 480.0;

/// UI zoom bounds (scroll / pinch), also used to sanitise a persisted value.
const ZOOM_MIN: f32 = 0.4;
const ZOOM_MAX: f32 = 4.0;

impl Default for Settings {
    fn default() -> Self {
        Self {
            bpm: 120,
            beats_per_bar: 4,
            a4: 440.0,
            theme: theme::Theme::default(),
            scale: Scale::default(),
            accidentals: Accidentals::default(),
            transpose: Transposition::default(),
            tempo_graduated: false,
            sound: Sound::default(),
            last_on: default_last_on(),
            zoom: default_zoom(),
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
    /// Light/dark device theme.
    theme: theme::Theme,
    /// Whether the settings popup is open.
    pub settings_open: bool,
    /// The last non-empty on-set — which of `(tuner, metronome)` were running —
    /// so the space bar can restore it after stopping both. See `Settings`.
    last_on: (bool, bool),
    /// Live UI zoom, mirrored from the egui context each frame so `save` (which
    /// has no context) can persist it.
    zoom: f32,
}

impl Tm2077App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let s = cc
            .storage
            .and_then(|s| eframe::get_value::<Settings>(s, eframe::APP_KEY))
            .unwrap_or_default();

        // Restore the persisted zoom (sanitised) so the device opens at its size.
        let zoom = if s.zoom.is_finite() {
            s.zoom.clamp(ZOOM_MIN, ZOOM_MAX)
        } else {
            default_zoom()
        };
        cc.egui_ctx.set_zoom_factor(zoom);

        Self {
            // The tuner always boots off (not persisted — see `Settings`).
            tuner_on: false,
            tuner: TunerState {
                // Clamp on load: an old or hand-edited save must not seed an
                // out-of-range (or non-finite) calibration.
                a4: if s.a4.is_finite() {
                    s.a4.clamp(A4_MIN, A4_MAX)
                } else {
                    440.0
                },
                scale: s.scale,
                accidentals: s.accidentals,
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
            // Never restore a both-off on-set (nothing for space to bring back)
            // — fall back to the metronome default.
            last_on: match s.last_on {
                (false, false) => default_last_on(),
                on => on,
            },
            zoom,
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            bpm: self.metronome.bpm,
            beats_per_bar: self.metronome.beats_per_bar,
            a4: self.tuner.a4,
            theme: self.theme,
            scale: self.tuner.scale,
            accidentals: self.tuner.accidentals,
            transpose: self.tuner.transpose,
            tempo_graduated: self.metronome.graduated,
            sound: self.metronome.sound,
            last_on: self.last_on,
            zoom: self.zoom,
        }
    }

    /// Handle all frame-global input in one pass: scroll/pinch zoom, the space
    /// bar (play/pause), and the continuous-repaint decision. Returns whether a
    /// user gesture (click or space) happened this frame, so the caller can unlock
    /// web audio *after* the frame's settings are live. Per-widget interaction
    /// lives with the widgets in `ui/controls.rs`; the settings popup handles its
    /// own Escape / click-outside.
    fn handle_input(&mut self, ui: &mut egui::Ui) -> bool {
        let (scroll, pinch, any_down, clicked) = ui.input(|i| {
            (
                i.smooth_scroll_delta.y,
                i.zoom_delta(),
                i.pointer.any_down(),
                i.pointer.any_click(),
            )
        });

        // Space is play/pause and *only* that: consume the event so it can't also
        // actuate a focused widget (e.g. a settings option). `repeat: false` so a
        // held space doesn't toggle every frame; no modifiers so Ctrl+Space etc.
        // fall through untouched.
        let space = ui.input_mut(|i| {
            let mut pressed = false;
            i.events.retain(|e| {
                let toggle = matches!(
                    e,
                    egui::Event::Key {
                        key: egui::Key::Space,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } if modifiers.is_none()
                );
                pressed |= toggle;
                !toggle
            });
            pressed
        });

        // Scroll-to-zoom (wheel / trackpad) and pinch-to-zoom (two-finger touch,
        // ctrl+scroll on desktop) both scale the whole UI together.
        if scroll != 0.0 || pinch != 1.0 {
            let z = (ui.ctx().zoom_factor() * pinch * (scroll * 0.0015).exp())
                .clamp(ZOOM_MIN, ZOOM_MAX);
            ui.ctx().set_zoom_factor(z);
        }
        // Mirror the live zoom so `save` can persist it (it has no context).
        self.zoom = ui.ctx().zoom_factor();

        // Global play/pause across both instruments (even with settings open):
        // if either is on, stop both; if both off, restore `last_on`.
        if space {
            if self.tuner_on || self.metronome.running {
                self.tuner_on = false;
                self.metronome.running = false;
            } else {
                (self.tuner_on, self.metronome.running) = self.last_on;
            }
        }

        // Repaint continuously only while something animates (needle) or a pointer
        // is held (so rockers' press-and-hold auto-repeat keeps ticking);
        // otherwise idle and repaint on input. Audio is unaffected (own thread).
        if self.metronome.running || self.tuner_on || any_down {
            ui.ctx().request_repaint();
        }

        // A click or space is a user gesture (needed to unlock web audio / mic);
        // reported so the caller can act at end of frame.
        clicked || space
    }

    /// Register a TAP TEMPO tap at time `now` (seconds) and update the bpm from
    /// the average of recent tap intervals.
    pub fn tap_tempo(&mut self, now: f64) {
        // Restart the sequence if the previous tap was long ago, else keep history;
        // then append `now` and keep the last `TAP_COUNT`. `base` is a cheap
        // structural-sharing clone, so `push_back` copies-on-write.
        let restart = matches!(self.tap_times.last(), Some(&last) if now - last > 2.0);
        let mut base = if restart {
            Vector::new()
        } else {
            self.tap_times.clone()
        };
        base.push_back(now);
        let taps = keep_last(&base, TAP_COUNT);

        if let Some(bpm) = tapped_bpm(&taps) {
            self.metronome.bpm = bpm;
        }
        self.tap_times = taps;
    }

    /// The settings popup (opened by the gear button), styled to match the device.
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

                // THEME label with the close cross on the same row's right edge.
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
                ui.label("ACCIDENTALS");
                ui.horizontal_wrapped(|ui| {
                    let a = &mut self.tuner.accidentals;
                    ui.selectable_value(a, Accidentals::Sharps, "Sharps");
                    ui.selectable_value(a, Accidentals::Flats, "Flats");
                    ui.selectable_value(a, Accidentals::Mixed, "Mixed");
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

        // A click outside the popup closes it — but not on its opening frame, so
        // the opening click doesn't immediately dismiss it.
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

/// The last `keep` elements as a new persistent vector (structural sharing keeps
/// both the `clone` and the rebuild cheap).
fn keep_last(taps: &Vector<f64>, keep: usize) -> Vector<f64> {
    match taps.len().checked_sub(keep) {
        Some(skip) if skip > 0 => taps.iter().skip(skip).copied().collect(),
        _ => taps.clone(),
    }
}

/// Average the consecutive tap intervals into a bpm clamped to 30..=300, or
/// `None` when there aren't yet enough taps for a well-defined tempo.
fn tapped_bpm(taps: &Vector<f64>) -> Option<u32> {
    let (&first, &last) = (taps.front()?, taps.last()?);
    let intervals = taps.len().checked_sub(1)? as f64;
    (intervals >= 1.0)
        .then_some((last - first) / intervals)
        .filter(|&avg| avg > 0.0)
        .map(|avg| ((60.0 / avg).round() as i64).clamp(BPM_MIN as i64, BPM_MAX as i64) as u32)
}

/// Traditional Maelzel graduations (40–208 bpm): dense low, coarser as tempo rises.
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

        // Paint the backdrop ourselves: eframe's bare root has no panel background,
        // so `panel_fill` alone never shows — fill the viewport with the theme's
        // panel colour (else the Light theme's surround stays the default dark).
        let screen = ui.ctx().input(|i| i.viewport_rect());
        ui.painter()
            .rect_filled(screen, egui::CornerRadius::ZERO, self.theme.palette().panel);

        // All frame-global input (zoom, space play/pause, repaint/gesture) in one
        // pass; per-widget interaction stays in `ui/controls.rs`.
        let gesture = self.handle_input(ui);
        // Was the popup open *before* this frame's clicks — so the opening click
        // doesn't also close it.
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

        // Record the current on-set whenever at least one instrument is on, so a
        // later space press can restore it. Both-off is never recorded.
        if self.tuner_on || self.metronome.running {
            self.last_on = (self.tuner_on, self.metronome.running);
        }

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
        self.audio.tuner_set_accidentals(self.tuner.accidentals);
        self.audio.tuner_set_transpose(self.tuner.transpose);

        // Settings are now live, so act on any gesture — a click/space that just
        // started an instrument unlocks audio and prompts for the mic.
        if gesture {
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
    fn tap_stays_off_grid_then_snaps_to_graduation() {
        // TAP TEMPO ignores graduated mode: it yields the exact averaged tempo,
        // even one that sits between graduations (e.g. 0.42 s → ~143 bpm).
        let tapped = tapped_bpm(&v(&[0.0, 0.42, 0.84, 1.26])).expect("two+ taps");
        assert_eq!(tapped, 143);
        assert!(!GRADUATIONS.contains(&tapped));
        // The next ▲/▼ then snaps onto the surrounding graduations.
        assert_eq!(next_graduation(tapped), 144);
        assert_eq!(prev_graduation(tapped), 138);
    }

    #[test]
    fn tapped_bpm_clamps_to_range() {
        assert_eq!(tapped_bpm(&v(&[0.0, 3.0])), Some(30)); // 20 bpm → clamped up
        assert_eq!(tapped_bpm(&v(&[0.0, 0.1])), Some(300)); // 600 bpm → clamped down
    }
}
