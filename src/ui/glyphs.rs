//! Small vector glyphs drawn with the painter (the default font lacks musical
//! symbols): rocker arrows, ♭ / ♯ accidentals, and a gear.

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

/// A gear / cog icon (settings), centred at `c` with outer `radius`.
pub fn gear(p: &egui::Painter, c: Pos2, radius: f32, color: Color32) {
    const TEETH: usize = 8;
    let rim = radius * 0.62;
    let sw = (radius * 0.16).max(1.3);
    for k in 0..TEETH {
        let a = k as f32 / TEETH as f32 * std::f32::consts::TAU;
        let d = vec2(a.cos(), a.sin());
        p.line_segment([c + d * rim, c + d * radius], Stroke::new(sw * 1.5, color));
    }
    p.circle_stroke(c, rim, Stroke::new(sw, color));
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

/// A flat (♭) glyph centred at `c`; `h` is the half-height. A thin tall stem
/// with a hollow bowl that has calligraphic weight — the loop swells at its
/// belly and tapers where it meets the stem, enclosing an open hole.
pub fn flat(p: &egui::Painter, c: Pos2, h: f32, color: Color32) {
    let w = h * 0.85;
    let stem_w = (h * 0.15).max(1.1);
    let stem_x = c.x - w * 0.3;
    // Tall, thin vertical stem.
    p.line_segment([pos2(stem_x, c.y - h), pos2(stem_x, c.y + h * 0.9)], Stroke::new(stem_w, color));

    // Bowl outline as a cubic bézier from the mid-stem, out to the right and
    // back to a point low on the stem. Sampled into overlapping dots whose
    // radius swells at the belly and tapers to the stem, so the loop is heavier
    // in the middle and leaves an open hole against the stem.
    let (p0, p1, p2, p3) = (
        pos2(stem_x, c.y - h * 0.1),
        pos2(stem_x + w * 1.15, c.y - h * 0.15),
        pos2(stem_x + w * 0.95, c.y + h * 0.55),
        pos2(stem_x, c.y + h * 0.9),
    );
    let (r_min, r_max) = (stem_w * 0.3, stem_w * 0.8);
    const N: usize = 32;
    for i in 0..=N {
        let t = i as f32 / N as f32;
        let mt = 1.0 - t;
        let (b0, b1, b2, b3) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
        let at = pos2(
            b0 * p0.x + b1 * p1.x + b2 * p2.x + b3 * p3.x,
            b0 * p0.y + b1 * p1.y + b2 * p2.y + b3 * p3.y,
        );
        // Thin at both stem attaches (t=0,1), heaviest across the belly.
        let r = r_min + (r_max - r_min) * (std::f32::consts::PI * t).sin();
        p.circle_filled(at, r, color);
    }
}

/// A sharp (♯) glyph centred at `c`; `h` is the half-height. Two upright bars
/// crossed by two thicker upward-slanting bars, both pairs overshooting the
/// crossing as in a real music sharp.
pub fn sharp(p: &egui::Painter, c: Pos2, h: f32, color: Color32) {
    let w = h * 0.5;
    // Horizontals are distinctly heavier than the uprights, as in a real sharp.
    let v_stroke = Stroke::new((h * 0.11).max(1.0), color);
    let h_stroke = Stroke::new((h * 0.30).max(1.8), color);
    // Two vertical bars, overshooting top and bottom.
    for dx in [-w * 0.5, w * 0.5] {
        p.line_segment([pos2(c.x + dx, c.y - h * 1.05), pos2(c.x + dx, c.y + h * 1.05)], v_stroke);
    }
    // Two rising horizontal bars.
    for dy in [-h * 0.36, h * 0.36] {
        p.line_segment(
            [pos2(c.x - w, c.y + dy + h * 0.16), pos2(c.x + w, c.y + dy - h * 0.16)],
            h_stroke,
        );
    }
}
