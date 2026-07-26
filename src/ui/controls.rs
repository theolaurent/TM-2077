//! The hardware controls around the LCD: TUNER/METRONOME ON pills, CALIB·NOTE
//! and BEAT/TEMPO rockers, the red TAP TEMPO button, and the wordmark. Handles
//! all interaction.

use egui::{Align2, FontId, Rect, pos2};

use super::{glyphs, icon_button, label, pill, rel_rect, rocker, round_button};
use crate::app::Tm2077App;
use crate::theme;

pub fn draw(ui: &mut egui::Ui, p: &egui::Painter, d: Rect, _lcd: Rect, app: &mut Tm2077App) {
    left_column(ui, p, d, app);
    right_column(ui, p, d, app);
    centre(ui, p, d, app);
}

/// Fires once when a button is pressed, then repeatedly while it is held (after
/// a short delay) — used to make the rockers auto-repeat. The hold is latched
/// when the press starts on the button, then kept alive by the *global* pointer
/// state until release (egui drops a click-only widget's own "down" flag once it
/// treats the press as a drag).
fn repeat_fire(ui: &egui::Ui, resp: &egui::Response) -> bool {
    const DELAY: f64 = 0.35; // hold time before auto-repeat starts
    const INTERVAL: f64 = 0.05; // repeat period once going
    let id = resp.id.with("repeat");
    let (primary_down, now) = ui.input(|i| (i.pointer.primary_down(), i.time));
    let state = ui.ctx().memory(|m| m.data.get_temp::<(f64, f64)>(id));

    if !primary_down {
        if state.is_some() {
            ui.ctx().memory_mut(|m| m.data.remove::<(f64, f64)>(id));
        }
        return false;
    }
    ui.ctx().request_repaint();

    match state {
        // Begin a hold only if the press actually started on this button.
        None => {
            if resp.is_pointer_button_down_on() {
                ui.ctx().memory_mut(|m| m.data.insert_temp(id, (now, now)));
                true
            } else {
                false
            }
        }
        // Latched: repeat purely on the clock until the button is released.
        Some((press, last_fire)) => {
            let fire = now - press >= DELAY && now - last_fire >= INTERVAL;
            if fire {
                ui.ctx().memory_mut(|m| m.data.insert_temp(id, (press, now)));
            }
            fire
        }
    }
}

fn left_column(ui: &mut egui::Ui, p: &egui::Painter, d: Rect, app: &mut Tm2077App) {
    // TUNER toggle pill (label inside; whole button lights amber when on).
    let tuner_pill = rel_rect(d, 0.035, 0.06, 0.195, 0.15);
    if pill(ui, p, tuner_pill, "TUNER", app.tuner_on).clicked() {
        app.tuner_on = !app.tuner_on;
    }

    // CALIB·NOTE rocker (A4 calibration).
    label(p, pos2(d.min.x + d.width() * 0.115, d.min.y + d.height() * 0.22), Align2::CENTER_CENTER, "CALIB · NOTE", 10.0, false);
    let calib = rel_rect(d, 0.075, 0.255, 0.155, 0.47);
    let (up, dn) = rocker(ui, p, calib, "calib");
    if repeat_fire(ui, &up) {
        app.tuner.a4 = (app.tuner.a4 + 1.0).clamp(410.0, 480.0);
    }
    if repeat_fire(ui, &dn) {
        app.tuner.a4 = (app.tuner.a4 - 1.0).clamp(410.0, 480.0);
    }
}

fn right_column(ui: &mut egui::Ui, p: &egui::Painter, d: Rect, app: &mut Tm2077App) {
    // METRONOME toggle pill (starts/stops; label inside, lights amber when on).
    let metro_pill = rel_rect(d, 0.805, 0.06, 0.965, 0.15);
    if pill(ui, p, metro_pill, "METRONOME", app.metronome.running).clicked() {
        app.metronome.running = !app.metronome.running;
    }

    // BEAT and TEMPO rockers, side by side.
    label(p, pos2(d.min.x + d.width() * 0.845, d.min.y + d.height() * 0.235), Align2::CENTER_CENTER, "BEAT", 10.0, false);
    label(p, pos2(d.min.x + d.width() * 0.925, d.min.y + d.height() * 0.235), Align2::CENTER_CENTER, "TEMPO", 10.0, false);
    let beat = rel_rect(d, 0.815, 0.27, 0.875, 0.49);
    let tempo = rel_rect(d, 0.895, 0.27, 0.955, 0.49);
    let (bu, bd) = rocker(ui, p, beat, "beat");
    if repeat_fire(ui, &bu) {
        app.metronome.beats_per_bar = (app.metronome.beats_per_bar + 1).min(12);
    }
    if repeat_fire(ui, &bd) {
        app.metronome.beats_per_bar = app.metronome.beats_per_bar.saturating_sub(1).max(1);
    }
    let (tu, td) = rocker(ui, p, tempo, "tempo");
    if repeat_fire(ui, &tu) {
        app.metronome.bpm = (app.metronome.bpm + 1).min(300);
    }
    if repeat_fire(ui, &td) {
        app.metronome.bpm = app.metronome.bpm.saturating_sub(1).max(30);
    }

    // Settings gear — a push-button in the bottom-right corner that opens the
    // settings popup.
    let t = theme::palette(p);
    let gear_rect = rel_rect(d, 0.89, 0.85, 0.965, 0.95);
    let resp = icon_button(ui, p, gear_rect, "settings");
    glyphs::gear(p, gear_rect.center(), gear_rect.height() * 0.34, t.btn_label);
    if resp.clicked() {
        app.settings_open = !app.settings_open;
    }
}

fn centre(ui: &mut egui::Ui, p: &egui::Painter, d: Rect, app: &mut Tm2077App) {
    let t = theme::palette(p);
    // TAP TEMPO (red round button).
    let tap_c = pos2(d.min.x + d.width() * 0.66, d.min.y + d.height() * 0.79);
    label(p, pos2(tap_c.x, tap_c.y - d.height() * 0.135), Align2::CENTER_CENTER, "TAP TEMPO", 10.0, false);
    let tap = round_button(ui, p, tap_c, d.height() * 0.085, "", t.tap, t.tap_lo);
    if tap.clicked() {
        let now = ui.input(|i| i.time);
        app.tap_tempo(now);
    }

    // Wordmark, bottom-left of centre.
    p.text(
        pos2(d.min.x + d.width() * 0.235, d.min.y + d.height() * 0.90),
        Align2::LEFT_CENTER,
        "TM-2077",
        FontId::proportional(22.0),
        t.body_label,
    );
    p.text(
        pos2(d.min.x + d.width() * 0.235, d.min.y + d.height() * 0.955),
        Align2::LEFT_CENTER,
        "COMBO TUNER · METRONOME",
        FontId::proportional(10.0),
        t.body_label_dim,
    );
}
