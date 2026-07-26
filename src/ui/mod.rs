//! Renders the TM-2077 as a Korg-TM-60-style device: matte-black landscape body,
//! amber LCD showing tuner + metronome at once, and a deck of hardware controls.

mod controls;
mod glyphs;
mod lcd;
mod seg;

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    pos2, vec2,
};

use crate::app::Tm2077App;
use crate::theme;

/// Aspect ratio (w:h) of the device body, close to the real TM-60.
const ASPECT: f32 = 1.62;

/// Draw the whole device and handle its controls.
pub fn draw_device(ui: &mut Ui, app: &mut Tm2077App) {
    let device = fit_aspect(ui.available_rect_before_wrap().shrink(8.0), ASPECT);
    let p = ui.painter().clone();

    paint_body(&p, device);

    // Tuning LEDs sit in the black bezel just above the LCD.
    leds(&p, device, app);

    // The amber LCD occupies the upper-centre of the face.
    let lcd = rel_rect(device, 0.225, 0.135, 0.775, 0.60);
    lcd::draw(&p, lcd, app);

    // All hardware controls (buttons/rockers) around it, with interaction.
    controls::draw(ui, &p, device, lcd, app);
}

// ---------------------------------------------------------------------------
// Body
// ---------------------------------------------------------------------------
fn paint_body(p: &egui::Painter, rect: Rect) {
    let t = theme::palette(p);
    p.rect_filled(
        rect.translate(vec2(0.0, 5.0)),
        CornerRadius::same(26),
        Color32::from_black_alpha(140),
    );
    fill_gradient_v(p, rect, t.body_edge_hi, t.body, 26);
    p.rect_stroke(
        rect,
        CornerRadius::same(26),
        Stroke::new(1.5, t.body_edge_lo),
        StrokeKind::Inside,
    );
    // Subtle moulded highlight along the top edge.
    let hi = Rect::from_min_size(rect.min + vec2(26.0, 7.0), vec2(rect.width() - 52.0, 2.0));
    p.rect_filled(hi, CornerRadius::same(2), Color32::from_white_alpha(14));
}

/// The three tuning LEDs (♭ flat · in-tune · ♯ sharp) above the LCD.
fn leds(p: &egui::Painter, d: Rect, app: &Tm2077App) {
    let t = theme::palette(p);
    let reading = if app.tuner_on { app.tuner.reading } else { None };
    let cents = reading.map(|r| r.cents);
    let flat = matches!(cents, Some(c) if c < -4.0);
    let sharp = matches!(cents, Some(c) if c > 4.0);
    let intune = matches!(cents, Some(c) if c.abs() <= 4.0);

    let y = d.min.y + d.height() * 0.085;
    let cx = d.center().x;
    let spacing = d.width() * 0.075;

    // Accidental glyphs flanking the outer LEDs.
    glyphs::flat(p, pos2(cx - spacing * 1.7, y), 8.0, t.body_label);
    glyphs::sharp(p, pos2(cx + spacing * 1.7, y), 8.0, t.body_label);

    led(p, pos2(cx - spacing, y), t.led_red_on, t.led_red_off, flat);
    led(p, pos2(cx, y), t.led_green_on, t.led_green_off, intune);
    led(p, pos2(cx + spacing, y), t.led_red_on, t.led_red_off, sharp);
}

fn led(p: &egui::Painter, c: Pos2, on: Color32, off: Color32, lit: bool) {
    if lit {
        p.circle_filled(c, 11.0, Color32::from_rgba_unmultiplied(on.r(), on.g(), on.b(), 55));
    }
    p.circle_filled(c, 6.5, if lit { on } else { off });
    p.circle_stroke(c, 6.5, Stroke::new(1.0, Color32::from_black_alpha(90)));
}

// ---------------------------------------------------------------------------
// Layout helpers
// ---------------------------------------------------------------------------

/// Fit a rectangle of the given aspect ratio centred inside `outer`.
fn fit_aspect(outer: Rect, aspect: f32) -> Rect {
    let (ow, oh) = (outer.width(), outer.height());
    let (w, h) = if ow / oh > aspect {
        (oh * aspect, oh)
    } else {
        (ow, ow / aspect)
    };
    Rect::from_center_size(outer.center(), vec2(w, h))
}

/// A sub-rectangle of `r` expressed in fractional [0,1] coordinates.
pub(crate) fn rel_rect(r: Rect, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect::from_min_max(
        pos2(r.min.x + r.width() * x0, r.min.y + r.height() * y0),
        pos2(r.min.x + r.width() * x1, r.min.y + r.height() * y1),
    )
}

// ---------------------------------------------------------------------------
// Shared widgets
// ---------------------------------------------------------------------------

/// A square rubber push-button chrome (gradient + hover/press states) with no
/// label; the caller paints an icon over the returned rect. Same look as the
/// rocker buttons. Returns the interaction response.
pub(crate) fn icon_button(ui: &mut Ui, p: &egui::Painter, rect: Rect, tag: &str) -> Response {
    let t = theme::palette(p);
    let resp = interact(ui, rect, tag);
    let (top, bot) = if resp.is_pointer_button_down_on() {
        (t.btn_lo, t.btn_lo)
    } else if resp.hovered() {
        (t.btn_hi, t.btn)
    } else {
        (t.btn, t.btn_lo)
    };
    fill_gradient_v(p, rect, top, bot, 6);
    p.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, t.body_edge_lo), StrokeKind::Inside);
    resp
}

/// A pill (fully-rounded) toggle button with an indicator dot when `on`.
pub(crate) fn pill(ui: &mut Ui, p: &egui::Painter, rect: Rect, on: bool) -> Response {
    let t = theme::palette(p);
    let resp = interact(ui, rect, "pill");
    let r = (rect.height() * 0.5) as u8;
    let (top, bot) = if resp.is_pointer_button_down_on() {
        (t.btn_lo, t.btn_lo)
    } else if resp.hovered() {
        (t.btn_hi, t.btn)
    } else {
        (t.btn, t.btn_lo)
    };
    fill_gradient_v_cr(p, rect, top, bot, CornerRadius::same(r));
    p.rect_stroke(rect, CornerRadius::same(r), Stroke::new(1.0, t.body_edge_lo), StrokeKind::Inside);
    // Indicator dot near the left, amber when on.
    let dot = pos2(rect.left() + rect.height() * 0.55, rect.center().y);
    let col = if on { t.btn_on } else { t.btn_lo };
    if on {
        p.circle_filled(dot, rect.height() * 0.28, Color32::from_rgba_unmultiplied(0xf6, 0xac, 0x1e, 60));
    }
    p.circle_filled(dot, rect.height() * 0.16, col);
    resp
}

/// A round push-button (used for TAP TEMPO). `base/hi/lo` set its colour.
pub(crate) fn round_button(
    ui: &mut Ui,
    p: &egui::Painter,
    center: Pos2,
    radius: f32,
    label: &str,
    base: Color32,
    hi: Color32,
    lo: Color32,
) -> Response {
    let rect = Rect::from_center_size(center, vec2(radius * 2.0, radius * 2.0));
    let resp = interact(ui, rect, label);
    let down = resp.is_pointer_button_down_on();
    p.circle_filled(center + vec2(0.0, 2.0), radius, Color32::from_black_alpha(120));
    p.circle_filled(center, radius, if down { lo } else { base });
    p.circle_stroke(center, radius, Stroke::new(1.5, lo));
    // Glossy top highlight.
    p.circle_filled(center - vec2(0.0, radius * 0.35), radius * 0.5, hi.gamma_multiply(if down { 0.2 } else { 0.5 }));
    if !label.is_empty() {
        p.text(center, Align2::CENTER_CENTER, label, FontId::proportional(11.0), Color32::WHITE);
    }
    resp
}

/// A vertical rocker: two stacked arrow buttons. Returns (up, down) responses.
pub(crate) fn rocker(ui: &mut Ui, p: &egui::Painter, rect: Rect, id: &str) -> (Response, Response) {
    let t = theme::palette(p);
    let up_rect = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.center().y - 1.0));
    let dn_rect = Rect::from_min_max(pos2(rect.min.x, rect.center().y + 1.0), rect.max);
    let up = interact(ui, up_rect, &format!("{id}-up"));
    let dn = interact(ui, dn_rect, &format!("{id}-dn"));
    for (r, resp, up_arrow) in [(up_rect, &up, true), (dn_rect, &dn, false)] {
        let (top, bot) = if resp.is_pointer_button_down_on() {
            (t.btn_lo, t.btn_lo)
        } else if resp.hovered() {
            (t.btn_hi, t.btn)
        } else {
            (t.btn, t.btn_lo)
        };
        fill_gradient_v(p, r, top, bot, 6);
        p.rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, t.body_edge_lo), StrokeKind::Inside);
        glyphs::arrow(p, r.center(), r.height().min(r.width()) * 0.28, up_arrow, t.btn_label);
    }
    (up, dn)
}

/// Small text label drawn on the body.
pub(crate) fn label(p: &egui::Painter, pos: Pos2, align: Align2, text: &str, size: f32, dim: bool) {
    let t = theme::palette(p);
    let col = if dim { t.body_label_dim } else { t.body_label };
    p.text(pos, align, text, FontId::proportional(size), col);
}

fn interact(ui: &mut Ui, rect: Rect, tag: &str) -> Response {
    let id = ui.id().with((tag, rect.min.x as i32, rect.min.y as i32));
    ui.interact(rect, id, Sense::click())
}

// ---------------------------------------------------------------------------
// Painting primitives
// ---------------------------------------------------------------------------

pub(crate) fn fill_gradient_v(p: &egui::Painter, rect: Rect, top: Color32, bottom: Color32, radius: u8) {
    fill_gradient_v_cr(p, rect, top, bottom, CornerRadius::same(radius));
}

pub(crate) fn fill_gradient_v_cr(p: &egui::Painter, rect: Rect, top: Color32, bottom: Color32, cr: CornerRadius) {
    const STEPS: usize = 24;
    for i in 0..STEPS {
        let t0 = i as f32 / STEPS as f32;
        let t1 = (i + 1) as f32 / STEPS as f32;
        let strip = Rect::from_min_max(
            pos2(rect.min.x, rect.min.y + rect.height() * t0),
            pos2(rect.max.x, rect.min.y + rect.height() * t1),
        );
        let scr = if i == 0 {
            CornerRadius { nw: cr.nw, ne: cr.ne, sw: 0, se: 0 }
        } else if i == STEPS - 1 {
            CornerRadius { nw: 0, ne: 0, sw: cr.sw, se: cr.se }
        } else {
            CornerRadius::same(0)
        };
        p.rect_filled(strip, scr, lerp_color(top, bottom, (t0 + t1) * 0.5));
    }
}

pub(crate) fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}
