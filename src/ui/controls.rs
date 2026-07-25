//! The hardware controls around the LCD: TUNER/METRONOME ON pills, CALIB·NOTE
//! and BEAT/TEMPO rockers, SOUND, START/STOP, the red TAP TEMPO button, plus the
//! speaker grille and wordmark. Handles all interaction.

use egui::{Align2, Color32, FontId, Rect, pos2, vec2};

use super::{button, label, pill, rel_rect, rocker, round_button};
use crate::app::Tm2077App;
use crate::theme::Palette;

pub fn draw(ui: &mut egui::Ui, p: &egui::Painter, d: Rect, _lcd: Rect, app: &mut Tm2077App) {
    left_column(ui, p, d, app);
    right_column(ui, p, d, app);
    centre(ui, p, d, app);
}

fn left_column(ui: &mut egui::Ui, p: &egui::Painter, d: Rect, app: &mut Tm2077App) {
    // TUNER ON pill.
    label(p, pos2(d.min.x + d.width() * 0.115, d.min.y + d.height() * 0.055), Align2::CENTER_CENTER, "TUNER ON", 11.0, false);
    let tuner_pill = rel_rect(d, 0.035, 0.09, 0.195, 0.155);
    if pill(ui, p, tuner_pill, app.tuner_on).clicked() {
        app.tuner_on = !app.tuner_on;
    }

    // CALIB·NOTE rocker (A4 calibration).
    label(p, pos2(d.min.x + d.width() * 0.115, d.min.y + d.height() * 0.22), Align2::CENTER_CENTER, "CALIB · NOTE", 10.0, false);
    let calib = rel_rect(d, 0.075, 0.255, 0.155, 0.47);
    let (up, dn) = rocker(ui, p, calib, "calib");
    if up.clicked() {
        app.tuner.a4 = (app.tuner.a4 + 1.0).clamp(410.0, 480.0);
    }
    if dn.clicked() {
        app.tuner.a4 = (app.tuner.a4 - 1.0).clamp(410.0, 480.0);
    }

    // SOUND: toggles a continuous A4 reference tone for tuning by ear.
    label(p, pos2(d.min.x + d.width() * 0.115, d.min.y + d.height() * 0.53), Align2::CENTER_CENTER, "SOUND", 10.0, false);
    let sound = rel_rect(d, 0.045, 0.56, 0.185, 0.64);
    if button(ui, p, sound, "SOUND", app.sound_on).clicked() {
        app.sound_on = !app.sound_on;
    }

    // MIC label + dots (decorative).
    label(p, pos2(d.min.x + d.width() * 0.10, d.min.y + d.height() * 0.80), Align2::CENTER_CENTER, "MIC", 10.0, true);
    for row in 0..2 {
        for col in 0..3 {
            let c = pos2(
                d.min.x + d.width() * 0.075 + col as f32 * 6.0,
                d.min.y + d.height() * 0.85 + row as f32 * 6.0,
            );
            p.circle_filled(c, 1.3, Palette::BODY_LABEL_DIM);
        }
    }
}

fn right_column(ui: &mut egui::Ui, p: &egui::Painter, d: Rect, app: &mut Tm2077App) {
    // METRONOME ON pill (starts/stops the metronome).
    label(p, pos2(d.min.x + d.width() * 0.885, d.min.y + d.height() * 0.055), Align2::CENTER_CENTER, "METRONOME ON", 10.0, false);
    let metro_pill = rel_rect(d, 0.805, 0.09, 0.965, 0.155);
    if pill(ui, p, metro_pill, app.metronome.running).clicked() {
        app.metronome.running = !app.metronome.running;
    }

    // BEAT and TEMPO rockers, side by side.
    label(p, pos2(d.min.x + d.width() * 0.845, d.min.y + d.height() * 0.235), Align2::CENTER_CENTER, "BEAT", 10.0, false);
    label(p, pos2(d.min.x + d.width() * 0.925, d.min.y + d.height() * 0.235), Align2::CENTER_CENTER, "TEMPO", 10.0, false);
    let beat = rel_rect(d, 0.815, 0.27, 0.875, 0.49);
    let tempo = rel_rect(d, 0.895, 0.27, 0.955, 0.49);
    let (bu, bd) = rocker(ui, p, beat, "beat");
    if bu.clicked() {
        app.metronome.beats_per_bar = (app.metronome.beats_per_bar + 1).min(12);
    }
    if bd.clicked() {
        app.metronome.beats_per_bar = app.metronome.beats_per_bar.saturating_sub(1).max(1);
    }
    let (tu, td) = rocker(ui, p, tempo, "tempo");
    if tu.clicked() {
        app.metronome.bpm = (app.metronome.bpm + 1).min(300);
    }
    if td.clicked() {
        app.metronome.bpm = app.metronome.bpm.saturating_sub(1).max(30);
    }

    // START / STOP.
    let start = rel_rect(d, 0.805, 0.55, 0.965, 0.64);
    if button(ui, p, start, if app.metronome.running { "STOP" } else { "START" }, app.metronome.running).clicked() {
        app.metronome.running = !app.metronome.running;
    }
}

fn centre(ui: &mut egui::Ui, p: &egui::Painter, d: Rect, app: &mut Tm2077App) {
    // Speaker grille (dot grid) lower-centre-left.
    let grille_c = pos2(d.min.x + d.width() * 0.42, d.min.y + d.height() * 0.80);
    speaker_grille(p, grille_c, d.height() * 0.11);

    // TAP TEMPO (red round button).
    let tap_c = pos2(d.min.x + d.width() * 0.66, d.min.y + d.height() * 0.79);
    label(p, pos2(tap_c.x, tap_c.y - d.height() * 0.135), Align2::CENTER_CENTER, "TAP TEMPO", 10.0, false);
    let tap = round_button(ui, p, tap_c, d.height() * 0.085, "", Palette::TAP, Palette::TAP_HI, Palette::TAP_LO);
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
        Palette::BODY_LABEL,
    );
    p.text(
        pos2(d.min.x + d.width() * 0.235, d.min.y + d.height() * 0.955),
        Align2::LEFT_CENTER,
        "COMBO TUNER · METRONOME",
        FontId::proportional(10.0),
        Palette::BODY_LABEL_DIM,
    );
}

fn speaker_grille(p: &egui::Painter, c: egui::Pos2, radius: f32) {
    let step = radius * 0.24;
    let n = (radius / step) as i32;
    for gy in -n..=n {
        for gx in -n..=n {
            let off = vec2(gx as f32 * step, gy as f32 * step);
            if off.length() <= radius {
                p.circle_filled(c + off, 1.4, Color32::from_black_alpha(150));
            }
        }
    }
    p.circle_stroke(c, radius + step, egui::Stroke::new(1.0, Palette::BODY_EDGE_LO));
}
