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

/// Fixed design width of the device (in points). The whole drawing is a fixed
/// size; the user scales it with scroll-to-zoom, so fonts/strokes (also in
/// points) scale uniformly with the body instead of staying a fixed pixel size.
const BASE_WIDTH: f32 = 820.0;

/// Draw the whole device and handle its controls.
pub fn draw_device(ui: &mut Ui, app: &mut Tm2077App) {
    // Scroll-to-zoom: a plain vertical scroll adjusts egui's zoom factor, which
    // scales the entire UI (device + text) together.
    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if scroll != 0.0 {
        let z = (ui.ctx().zoom_factor() * (scroll * 0.0015).exp()).clamp(0.4, 4.0);
        ui.ctx().set_zoom_factor(z);
    }

    // Fixed-size device, centred in the available area.
    let avail = ui.available_rect_before_wrap();
    let device = Rect::from_center_size(avail.center(), vec2(BASE_WIDTH, BASE_WIDTH / ASPECT));
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

/// A pill (fully-rounded) toggle button with a centred label; the whole button
/// lights amber when `on`.
pub(crate) fn pill(ui: &mut Ui, p: &egui::Painter, rect: Rect, label: &str, on: bool) -> Response {
    let t = theme::palette(p);
    let resp = interact(ui, rect, label);
    let r = (rect.height() * 0.5) as u8;
    let (top, bot) = if on {
        (t.btn_on, t.lcd_bg_edge)
    } else if resp.is_pointer_button_down_on() {
        (t.btn_lo, t.btn_lo)
    } else if resp.hovered() {
        (t.btn_hi, t.btn)
    } else {
        (t.btn, t.btn_lo)
    };
    fill_gradient_v_cr(p, rect, top, bot, CornerRadius::same(r));
    p.rect_stroke(rect, CornerRadius::same(r), Stroke::new(1.0, t.body_edge_lo), StrokeKind::Inside);
    let col = if on { t.bezel } else { t.btn_label };
    p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(11.0), col);
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
    use egui::epaint::{Mesh, Vertex, WHITE_UV};
    // Fill a rounded-rect mesh with a smooth vertical gradient (per-vertex
    // colour). A triangle fan from the centre fills the convex rounded shape, so
    // the fill's corners match the rounded outline exactly — no square nubs.
    let h = rect.height().max(1.0);
    let col = |y: f32| lerp_color(top, bottom, ((y - rect.min.y) / h).clamp(0.0, 1.0));
    let ring = rounded_rect_ring(rect, cr);
    let mut mesh = Mesh::default();
    let center = rect.center();
    mesh.vertices.push(Vertex { pos: center, uv: WHITE_UV, color: col(center.y) });
    for &pt in &ring {
        mesh.vertices.push(Vertex { pos: pt, uv: WHITE_UV, color: col(pt.y) });
    }
    let n = ring.len() as u32;
    for i in 0..n {
        mesh.indices.extend_from_slice(&[0, 1 + i, 1 + (i + 1) % n]);
    }
    p.add(mesh);
}

/// Perimeter points of a rounded rectangle, clockwise (for meshing).
fn rounded_rect_ring(rect: Rect, cr: CornerRadius) -> Vec<Pos2> {
    use std::f32::consts::{FRAC_PI_2, PI};
    let max_r = (rect.width().min(rect.height()) * 0.5).max(0.0);
    let (nw, ne, se, sw) = (
        (cr.nw as f32).min(max_r),
        (cr.ne as f32).min(max_r),
        (cr.se as f32).min(max_r),
        (cr.sw as f32).min(max_r),
    );
    let mut pts = Vec::new();
    arc(rect.min.x + nw, rect.min.y + nw, nw, PI, PI + FRAC_PI_2, &mut pts);
    arc(rect.max.x - ne, rect.min.y + ne, ne, PI + FRAC_PI_2, 2.0 * PI, &mut pts);
    arc(rect.max.x - se, rect.max.y - se, se, 0.0, FRAC_PI_2, &mut pts);
    arc(rect.min.x + sw, rect.max.y - sw, sw, FRAC_PI_2, PI, &mut pts);
    pts
}

/// Append points along a quarter-circle arc to `out`.
fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, out: &mut Vec<Pos2>) {
    const SEG: usize = 5;
    for i in 0..=SEG {
        let a = a0 + (a1 - a0) * (i as f32 / SEG as f32);
        out.push(pos2(cx + r * a.cos(), cy + r * a.sin()));
    }
}

pub(crate) fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}
