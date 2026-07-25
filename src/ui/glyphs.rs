//! Small vector glyphs drawn with the painter (the default font lacks musical
//! symbols): rocker arrows, ♭ / ♯ accidentals, and a metronome icon.

use egui::{Color32, Pos2, Shape, Stroke, pos2, vec2};

/// A solid triangle arrow, pointing up or down, centred at `c`.
pub fn arrow(p: &egui::Painter, c: Pos2, size: f32, up: bool, color: Color32) {
    let dy = if up { -1.0 } else { 1.0 };
    let pts = vec![
        c + vec2(0.0, dy * size),
        c + vec2(-size, -dy * size * 0.8),
        c + vec2(size, -dy * size * 0.8),
    ];
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// A solid triangle marker pointing down (used on the tuner arc).
pub fn marker_down(p: &egui::Painter, c: Pos2, size: f32, color: Color32) {
    let pts = vec![
        c + vec2(0.0, size),
        c + vec2(-size, -size * 0.7),
        c + vec2(size, -size * 0.7),
    ];
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// A flat (♭) glyph centred at `c`; `h` is the half-height.
pub fn flat(p: &egui::Painter, c: Pos2, h: f32, color: Color32) {
    let w = h * 0.7;
    let stroke = Stroke::new((h * 0.18).max(1.2), color);
    let stem_x = c.x - w * 0.35;
    // Vertical stem.
    p.line_segment([pos2(stem_x, c.y - h), pos2(stem_x, c.y + h)], stroke);
    // Bowl on the lower right.
    let bowl = vec![
        pos2(stem_x, c.y - h * 0.05),
        pos2(c.x + w * 0.55, c.y + h * 0.35),
        pos2(stem_x, c.y + h),
    ];
    p.add(Shape::convex_polygon(bowl, color, Stroke::NONE));
}

/// A sharp (♯) glyph centred at `c`; `h` is the half-height.
pub fn sharp(p: &egui::Painter, c: Pos2, h: f32, color: Color32) {
    let w = h * 0.62;
    let stroke = Stroke::new((h * 0.16).max(1.1), color);
    // Two (slightly angled) verticals.
    for dx in [-w * 0.35, w * 0.35] {
        p.line_segment([pos2(c.x + dx, c.y - h * 1.05), pos2(c.x + dx, c.y + h * 0.9)], stroke);
    }
    // Two rising horizontals.
    for dy in [-h * 0.32, h * 0.32] {
        p.line_segment(
            [pos2(c.x - w, c.y + dy + h * 0.12), pos2(c.x + w, c.y + dy - h * 0.12)],
            Stroke::new((h * 0.22).max(1.4), color),
        );
    }
}

/// A little metronome icon (trapezoid body + pendulum) centred at `c`.
pub fn metronome(p: &egui::Painter, c: Pos2, h: f32, color: Color32) {
    let w = h * 0.8;
    let body = vec![
        pos2(c.x - w * 0.35, c.y - h),
        pos2(c.x + w * 0.35, c.y - h),
        pos2(c.x + w, c.y + h),
        pos2(c.x - w, c.y + h),
    ];
    p.add(Shape::convex_polygon(body, Color32::TRANSPARENT, Stroke::new(1.4, color)));
    // Pendulum rod.
    p.line_segment([pos2(c.x + w * 0.25, c.y + h * 0.7), pos2(c.x - w * 0.1, c.y - h * 0.7)], Stroke::new(1.4, color));
}
