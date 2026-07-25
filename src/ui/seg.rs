//! A minimal 7-segment display renderer for the amber LCD readouts.
//!
//! Segment layout:
//! ```text
//!  aaa
//! f   b
//!  ggg
//! e   c
//!  ddd
//! ```

use egui::{Color32, Pos2, Rect, Shape, Stroke, pos2, vec2};

// Segment table for digits 0-9: [a, b, c, d, e, f, g].
const DIGITS: [[bool; 7]; 10] = [
    [true, true, true, true, true, true, false],    // 0
    [false, true, true, false, false, false, false], // 1
    [true, true, false, true, true, false, true],    // 2
    [true, true, true, true, false, false, true],    // 3
    [false, true, true, false, false, true, true],   // 4
    [true, false, true, true, false, true, true],    // 5
    [true, false, true, true, true, true, true],     // 6
    [true, true, true, false, false, false, false],  // 7
    [true, true, true, true, true, true, true],      // 8
    [true, true, true, true, false, true, true],     // 9
];

/// Draw a single digit (or, for `None`, only the faint ghost segments) filling
/// `rect`. `ink` draws lit segments; `ghost` draws unlit ones (LCD look).
pub fn digit(p: &egui::Painter, rect: Rect, d: Option<u8>, ink: Color32, ghost: Color32) {
    let on = d.and_then(|d| DIGITS.get(d as usize).copied()).unwrap_or([false; 7]);
    let (x0, y0, w, h) = (rect.min.x, rect.min.y, rect.width(), rect.height());
    let th = w * 0.17;
    let lh = w - th * 1.1; // horizontal segment length
    let lv = h * 0.5 - th * 0.9; // vertical segment length

    let cx = x0 + w * 0.5;
    let positions: [(Pos2, bool); 7] = [
        (pos2(cx, y0 + th * 0.5), true),               // a
        (pos2(x0 + w - th * 0.5, y0 + h * 0.25), false), // b
        (pos2(x0 + w - th * 0.5, y0 + h * 0.75), false), // c
        (pos2(cx, y0 + h - th * 0.5), true),           // d
        (pos2(x0 + th * 0.5, y0 + h * 0.75), false),   // e
        (pos2(x0 + th * 0.5, y0 + h * 0.25), false),   // f
        (pos2(cx, y0 + h * 0.5), true),                // g
    ];

    for (i, (c, horizontal)) in positions.iter().enumerate() {
        let len = if *horizontal { lh } else { lv };
        let poly = if *horizontal { hseg(*c, len, th) } else { vseg(*c, len, th) };
        let color = if on[i] { ink } else { ghost };
        p.add(Shape::convex_polygon(poly, color, Stroke::NONE));
    }
}

/// Draw a right-aligned integer within `rect` using `count` digit cells.
/// Leading positions are blank (only ghost segments).
pub fn number(p: &egui::Painter, rect: Rect, value: u32, count: usize, ink: Color32, ghost: Color32) {
    let gap = rect.width() * 0.06 / count as f32;
    let cell_w = (rect.width() - gap * (count as f32 - 1.0)) / count as f32;

    // Decompose into digits, least-significant first.
    let mut digits = [None; 16];
    let mut v = value;
    let mut n = 0;
    loop {
        digits[n] = Some((v % 10) as u8);
        v /= 10;
        n += 1;
        if v == 0 || n >= count {
            break;
        }
    }

    for i in 0..count {
        // Column i from the left maps to digit place (count-1-i).
        let place = count - 1 - i;
        let d = digits.get(place).copied().flatten();
        let cell = Rect::from_min_size(
            pos2(rect.min.x + i as f32 * (cell_w + gap), rect.min.y),
            vec2(cell_w, rect.height()),
        );
        digit(p, cell, d, ink, ghost);
    }
}

fn hseg(c: Pos2, len: f32, th: f32) -> Vec<Pos2> {
    let (hl, ht) = (len * 0.5, th * 0.5);
    vec![
        c + vec2(-hl, 0.0),
        c + vec2(-hl + ht, -ht),
        c + vec2(hl - ht, -ht),
        c + vec2(hl, 0.0),
        c + vec2(hl - ht, ht),
        c + vec2(-hl + ht, ht),
    ]
}

fn vseg(c: Pos2, len: f32, th: f32) -> Vec<Pos2> {
    let (hl, ht) = (len * 0.5, th * 0.5);
    vec![
        c + vec2(0.0, -hl),
        c + vec2(ht, -hl + ht),
        c + vec2(ht, hl - ht),
        c + vec2(0.0, hl),
        c + vec2(-ht, hl - ht),
        c + vec2(-ht, -hl + ht),
    ]
}
