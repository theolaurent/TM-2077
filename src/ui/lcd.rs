//! The amber LCD: a single screen showing the tuner (left/centre) and the
//! metronome (right) at the same time, like the Korg TM-60.

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Stroke, StrokeKind, pos2, vec2};

use super::{fill_gradient_v, glyphs, rel_rect, seg};
use crate::app::Tm2077App;
use crate::note::QuarterTone;
use crate::theme;

/// Shrink factor applied to the 7-segment readouts (calib / BPM / beat).
const SEG_SCALE: f32 = 0.65;

/// Scale a rect about its centre — used to fine-tune readout sizes without
/// moving where they sit on the LCD.
fn shrunk(rect: Rect, f: f32) -> Rect {
    Rect::from_center_size(rect.center(), rect.size() * f)
}

pub fn draw(p: &egui::Painter, rect: Rect, app: &Tm2077App) {
    let t = theme::palette(p);
    // Recessed frame: dark outer, grey inner bezel, amber screen.
    p.rect_filled(rect.expand(7.0), CornerRadius::same(10), t.bezel);
    p.rect_filled(rect.expand(3.0), CornerRadius::same(7), t.lcd_frame);
    fill_gradient_v(p, rect, t.lcd_bg, t.lcd_bg_edge, 6);
    p.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_black_alpha(50)), StrokeKind::Inside);

    tabs(p, rect);
    tuner(p, rect, app);
    metronome(p, rect, app);
    // Shared analog needle (tuner + metronome), drawn on top of both halves.
    needle_meter(p, rect, app);
}

fn tabs(p: &egui::Painter, r: Rect) {
    let t = theme::palette(p);
    for (frac, text) in [((0.02f32, 0.30f32), "TUNER"), ((0.70, 0.98), "METRONOME")] {
        let tab = shrunk(rel_rect(r, frac.0, 0.03, frac.1, 0.155), 0.8);
        p.rect_filled(tab, CornerRadius::same(4), t.lcd_ink);
        p.text(tab.center(), Align2::CENTER_CENTER, text, FontId::proportional(10.0), t.lcd_bg);
    }
}

// ---------------------------------------------------------------------------
// Tuner half (Hz, note letter, needle meter)
// ---------------------------------------------------------------------------
fn tuner(p: &egui::Painter, r: Rect, app: &Tm2077App) {
    let t = theme::palette(p);
    let reading = if app.tuner_on { app.tuner.reading } else { None };

    // A4 calibration readout (7-seg), mirroring the metronome's TEMPO field on
    // the far side of the LCD. The calib range (410-480 Hz) is always three
    // digits, so this is a fixed 3-cell field, symmetric with BPM.
    let hz_rect = shrunk(rel_rect(r, 0.03, 0.15, 0.26, 0.39), SEG_SCALE);
    let a4 = app.tuner.a4.round() as u32;
    seg::number(p, hz_rect, a4, 3, t.lcd_ink);
    p.text(
        pos2(hz_rect.max.x + 6.0, hz_rect.center().y),
        Align2::LEFT_CENTER,
        "Hz",
        FontId::proportional(13.0),
        t.lcd_ink,
    );

    // Note letter, upper-centre.
    let note_c = pos2(r.center().x, r.min.y + r.height() * 0.30);
    let note_size = r.height() * 0.23;
    if let Some(rd) = reading {
        // Note names are ASCII ("A".."G", optionally "#"); take the letter
        // without risking a panic on a bad byte boundary.
        let letter = rd.name.get(0..1).unwrap_or(rd.name);
        p.text(note_c, Align2::CENTER_CENTER, letter, FontId::proportional(note_size), t.lcd_ink);
        if rd.name.ends_with('#') {
            glyphs::sharp(p, note_c + vec2(note_size * 0.55, -note_size * 0.28), note_size * 0.23, t.lcd_ink);
        }
        // Quarter-tone (24-TET) accidental.
        let q_at = note_c + vec2(note_size * 0.55, -note_size * 0.28);
        match rd.quarter {
            QuarterTone::HalfSharp => glyphs::half_sharp(p, q_at, note_size * 0.23, t.lcd_ink),
            QuarterTone::HalfFlat => glyphs::half_flat(p, q_at, note_size * 0.23, t.lcd_ink),
            QuarterTone::None => {}
        }
    }
}

fn needle_meter(p: &egui::Painter, r: Rect, app: &Tm2077App) {
    let t = theme::palette(p);
    // Clip everything to the screen so the (off-screen) pivot doesn't spill.
    let p = p.with_clip_rect(r);
    let h = r.height();
    let cx = r.center().x;
    // Pivot sits below the LCD so the visible arc is a shallow analog-meter
    // curve. The offset places the arc vertically; the radius sets its size.
    let pivot = pos2(cx, r.max.y + h * 0.57);
    let radius = h * 1.05;
    let max = 32f32.to_radians();
    let dir = |ang: f32| vec2(ang.sin(), -ang.cos());

    // Dotted arc.
    for i in 0..=40 {
        let frac = i as f32 / 20.0 - 1.0; // -1..1
        let ang = frac * max;
        let rad = if i % 5 == 0 { 2.6 } else { 1.5 };
        p.circle_filled(pivot + dir(ang) * radius, rad, t.lcd_ink);
    }

    // Fixed centre reference marker.
    glyphs::marker_down(&p, pivot + dir(0.0) * (radius + 11.0), 5.5, t.lcd_ink);

    // Scale end labels near the arc ends.
    p.text(pivot + dir(-max) * radius + vec2(-2.0, 10.0), Align2::CENTER_CENTER, "-50", FontId::proportional(11.0), t.lcd_ink);
    p.text(pivot + dir(max) * radius + vec2(2.0, 10.0), Align2::CENTER_CENTER, "+50", FontId::proportional(11.0), t.lcd_ink);

    // --- Shared needle ---
    // The tuner deflects the needle by cents; the metronome swings it side to
    // side, reaching an extreme on every beat. With both active the needle
    // hinges at its visible midpoint — the metronome drives the lower half, the
    // tuner the upper half; with only one active both halves share that angle so
    // the whole needle moves as one.
    let ctx = p.ctx();

    let tuner_on = app.tuner_on;
    let tuner_ang = if tuner_on {
        app.tuner
            .reading
            .map(|rd| rd.cents.clamp(-50.0, 50.0) / 50.0 * max)
            .unwrap_or(0.0)
    } else {
        0.0
    };

    let metro_on = app.metronome.running;
    // Flip the swing side every time the audio beat advances (works for any
    // metre), then let egui ease the needle toward that extreme over one beat.
    let side_id = egui::Id::new("metro_swing_side");
    let last_id = egui::Id::new("metro_last_beat");
    let swing_id = egui::Id::new("metro_swing");
    let beat = app.metronome.beat_count;
    let side = ctx.memory_mut(|m| {
        if !metro_on {
            // Idle: park on the left and record the current beat, so the first
            // beat sweeps fully right instead of a half-swing from centre.
            m.data.insert_temp(last_id, beat);
            m.data.insert_temp(side_id, -1.0f32);
            return -1.0f32;
        }
        let last = m.data.get_temp::<u32>(last_id);
        let mut side = m.data.get_temp::<f32>(side_id).unwrap_or(-1.0);
        if last != Some(beat) {
            // Flip on every beat change; the first observation just records it.
            if last.is_some() {
                side = -side;
            }
            m.data.insert_temp(last_id, beat);
            m.data.insert_temp(side_id, side);
        }
        side
    });
    let beat_secs = (60.0 / app.metronome.bpm.max(1) as f32).clamp(0.05, 1.0);
    let metro_ang = if metro_on {
        ctx.animate_value_with_time(swing_id, side * max, beat_secs)
    } else {
        // Snap to the left extreme while idle so a run begins there.
        ctx.animate_value_with_time(swing_id, -max, 0.0)
    };

    // Two independent needles sharing the pivot: the outer (top) band is the
    // tuner's needle, the inner (bottom) band the metronome's — each just the
    // usual full-scale needle, truncated to its half. When only one instrument
    // is active it owns both bands, i.e. the whole needle.
    let top_ang = if tuner_on { tuner_ang } else { metro_ang };
    let bottom_ang = if metro_on { metro_ang } else { tuner_ang };

    // Only draw the needle when an instrument is active (no idle resting line).
    if tuner_on || metro_on {
        // Radial split: ~30% of the visible needle for the metronome (inner),
        // ~70% for the tuner (outer). The visible needle starts near s=0.54
        // (pivot is off-screen), so 0.68 gives the 30/70 division.
        const SPLIT: f32 = 0.68;
        let split_r = radius * SPLIT;
        // Small gaps: between the needle tip and the arc, and — only when both
        // instruments drive the needle — between the two bands at the split.
        let tip_gap = radius * 0.04;
        let split_gap = if tuner_on && metro_on { radius * 0.007 } else { 0.0 };
        // Bottom band (pivot → below the split): metronome.
        p.line_segment(
            [pivot, pivot + dir(bottom_ang) * (split_r - split_gap)],
            Stroke::new(3.6, t.lcd_ink),
        );
        // Top band (above the split → just short of the arc): tuner.
        p.line_segment(
            [
                pivot + dir(top_ang) * (split_r + split_gap),
                pivot + dir(top_ang) * (radius - tip_gap),
            ],
            Stroke::new(3.6, t.lcd_ink),
        );
    }
}

// ---------------------------------------------------------------------------
// Metronome half (tempo, beat)
// ---------------------------------------------------------------------------
fn metronome(p: &egui::Painter, r: Rect, app: &Tm2077App) {
    let t = theme::palette(p);
    let m = &app.metronome;

    // Tempo number (7-seg) with a "BPM" label — same gap / middle-line
    // alignment as the other readouts.
    let bpm_rect = shrunk(rel_rect(r, 0.74, 0.15, 0.97, 0.39), SEG_SCALE);
    seg::number(p, bpm_rect, m.bpm, 3, t.lcd_ink);
    p.text(
        pos2(bpm_rect.min.x - 6.0, bpm_rect.center().y),
        Align2::RIGHT_CENTER,
        "BPM",
        FontId::proportional(11.0),
        t.lcd_ink,
    );

    // "BEAT" + beats-per-bar: right-aligned to the BPM display and tucked just
    // below it. Two cells: BEAT goes up to 12 (a single cell would truncate).
    let beat_w = bpm_rect.width() * 0.40;
    let beat_h = bpm_rect.height() * 0.60;
    let beat_rect = Rect::from_min_size(
        pos2(bpm_rect.max.x - beat_w, bpm_rect.max.y + r.height() * 0.03),
        vec2(beat_w, beat_h),
    );
    seg::number(p, beat_rect, m.beats_per_bar, 2, t.lcd_ink);
    p.text(
        pos2(beat_rect.min.x - 6.0, beat_rect.center().y),
        Align2::RIGHT_CENTER,
        "BEAT",
        FontId::proportional(11.0),
        t.lcd_ink,
    );
}
