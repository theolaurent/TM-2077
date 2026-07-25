//! The amber LCD: a single screen showing the tuner (left/centre) and the
//! metronome (right) at the same time, like the Korg TM-60.

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Stroke, StrokeKind, pos2, vec2};

use super::{fill_gradient_v, glyphs, rel_rect, seg};
use crate::app::Tm2077App;
use crate::theme::Palette;

pub fn draw(p: &egui::Painter, rect: Rect, app: &Tm2077App) {
    // Recessed frame: dark outer, grey inner bezel, amber screen.
    p.rect_filled(rect.expand(7.0), CornerRadius::same(10), Palette::BEZEL);
    p.rect_filled(rect.expand(3.0), CornerRadius::same(7), Palette::LCD_FRAME);
    fill_gradient_v(p, rect, Palette::LCD_BG, Palette::LCD_BG_EDGE, 6);
    p.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_black_alpha(50)), StrokeKind::Inside);

    tabs(p, rect);
    tuner(p, rect, app);
    metronome(p, rect, app);
}

fn tabs(p: &egui::Painter, r: Rect) {
    for (frac, text) in [((0.02f32, 0.30f32), "TUNER"), ((0.70, 0.98), "METRONOME")] {
        let tab = rel_rect(r, frac.0, 0.03, frac.1, 0.155);
        p.rect_filled(tab, CornerRadius::same(4), Palette::LCD_INK);
        p.text(tab.center(), Align2::CENTER_CENTER, text, FontId::proportional(12.0), Palette::LCD_BG);
    }
}

// ---------------------------------------------------------------------------
// Tuner half (Hz, note letter, needle meter)
// ---------------------------------------------------------------------------
fn tuner(p: &egui::Painter, r: Rect, app: &Tm2077App) {
    let reading = if app.tuner_on { app.tuner.reading } else { None };

    // Frequency readout (7-seg) top-left.
    let hz_rect = rel_rect(r, 0.04, 0.20, 0.26, 0.35);
    let hz = reading.map(|x| x.freq.round() as u32).unwrap_or(0);
    seg::number(p, hz_rect, hz, 3, Palette::LCD_INK, Palette::LCD_INK_DIM);
    p.text(
        pos2(hz_rect.max.x + 6.0, hz_rect.center().y),
        Align2::LEFT_CENTER,
        "Hz",
        FontId::proportional(13.0),
        Palette::LCD_INK,
    );

    // Big note letter, upper-centre.
    let note_c = pos2(r.center().x, r.min.y + r.height() * 0.30);
    if let Some(rd) = reading {
        // Note names are ASCII ("A".."G", optionally "#"); take the letter
        // without risking a panic on a bad byte boundary.
        let letter = rd.name.get(0..1).unwrap_or(rd.name);
        p.text(note_c, Align2::CENTER_CENTER, letter, FontId::proportional(r.height() * 0.30), Palette::LCD_INK);
        if rd.name.ends_with('#') {
            glyphs::sharp(p, note_c + vec2(r.height() * 0.16, -r.height() * 0.08), r.height() * 0.07, Palette::LCD_INK);
        }
    } else {
        p.text(note_c, Align2::CENTER_CENTER, "-", FontId::proportional(r.height() * 0.30), Palette::LCD_INK_DIM);
    }

    needle_meter(p, r, reading.map(|x| x.cents), reading.is_some());
}

fn needle_meter(p: &egui::Painter, r: Rect, cents: Option<f32>, active: bool) {
    // Clip everything to the screen so the (off-screen) pivot doesn't spill.
    let p = p.with_clip_rect(r);
    let h = r.height();
    let cx = r.center().x;
    // Pivot sits below the LCD so the visible arc is a shallow analog-meter curve.
    let pivot = pos2(cx, r.max.y + h * 0.62);
    let radius = h * 1.28;
    let max = 32f32.to_radians();
    let dir = |ang: f32| vec2(ang.sin(), -ang.cos());

    // Dotted arc.
    for i in 0..=40 {
        let frac = i as f32 / 20.0 - 1.0; // -1..1
        let ang = frac * max;
        let rad = if i % 5 == 0 { 2.6 } else { 1.5 };
        p.circle_filled(pivot + dir(ang) * radius, rad, Palette::LCD_INK);
    }

    // Fixed triangle reference markers (left, centre, right).
    for frac in [-0.5f32, 0.0, 0.5] {
        glyphs::marker_down(&p, pivot + dir(frac * max) * (radius + 11.0), 5.5, Palette::LCD_INK);
    }

    // Scale end labels near the arc ends.
    p.text(pivot + dir(-max) * radius + vec2(-2.0, 10.0), Align2::CENTER_CENTER, "-50", FontId::proportional(11.0), Palette::LCD_INK);
    p.text(pivot + dir(max) * radius + vec2(2.0, 10.0), Align2::CENTER_CENTER, "+50", FontId::proportional(11.0), Palette::LCD_INK);

    // Hatched fan at the bottom-centre where the needle emerges.
    let base = pos2(cx, r.max.y - 1.0);
    for k in -4..=4 {
        let ang = (k as f32) * 3.4f32.to_radians();
        p.line_segment([base, base + dir(ang) * h * 0.14], Stroke::new(1.3, Palette::LCD_INK));
    }

    // The needle (base off-screen at the pivot; clip hides the part below the LCD).
    let c = cents.unwrap_or(0.0).clamp(-50.0, 50.0) / 50.0;
    let col = if !active { Palette::LCD_INK_DIM } else { Palette::LCD_INK };
    p.line_segment([pivot, pivot + dir(c * max) * radius], Stroke::new(3.6, col));
}

// ---------------------------------------------------------------------------
// Metronome half (tempo, beat)
// ---------------------------------------------------------------------------
fn metronome(p: &egui::Painter, r: Rect, app: &Tm2077App) {
    let m = &app.metronome;

    // "TEMPO" + metronome icon.
    p.text(pos2(r.min.x + r.width() * 0.66, r.min.y + r.height() * 0.23), Align2::LEFT_CENTER, "TEMPO", FontId::proportional(11.0), Palette::LCD_INK);
    glyphs::metronome(p, pos2(r.min.x + r.width() * 0.68, r.min.y + r.height() * 0.42), r.height() * 0.09, Palette::LCD_INK);

    // Tempo number (7-seg), large.
    let bpm_rect = rel_rect(r, 0.74, 0.20, 0.97, 0.44);
    seg::number(p, bpm_rect, m.bpm, 3, Palette::LCD_INK, Palette::LCD_INK_DIM);

    // "BEAT" + beats-per-bar.
    p.text(pos2(r.min.x + r.width() * 0.72, r.min.y + r.height() * 0.60), Align2::LEFT_CENTER, "BEAT", FontId::proportional(11.0), Palette::LCD_INK);
    let beat_rect = rel_rect(r, 0.88, 0.52, 0.97, 0.72);
    seg::number(p, beat_rect, m.beats_per_bar, 1, Palette::LCD_INK, Palette::LCD_INK_DIM);

    // Running beat indicator dots along the bottom-right.
    let n = m.beats_per_bar.max(1);
    let y = r.min.y + r.height() * 0.86;
    let gap = (r.width() * 0.26) / (n.max(1) as f32);
    let start_x = r.min.x + r.width() * 0.70;
    for b in 0..n {
        let x = start_x + b as f32 * gap;
        let on = m.running && b == m.current_beat;
        let rad = if b == 0 { 4.5 } else { 3.5 };
        let col = if on { Palette::LCD_INK } else { Palette::LCD_INK_DIM };
        p.circle_filled(pos2(x, y), rad, col);
    }
}
