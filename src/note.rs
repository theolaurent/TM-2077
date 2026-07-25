//! Musical pitch helpers: converting a detected frequency into a note name,
//! octave and cents deviation, relative to a configurable A4 reference.

pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A pitch reading derived from a detected frequency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteReading {
    /// The raw detected frequency in Hz.
    pub freq: f32,
    /// Note letter, e.g. "A" or "C#".
    pub name: &'static str,
    /// Scientific-pitch octave number (A4 = octave 4).
    pub octave: i32,
    /// Deviation from the nearest note, in cents (-50.0..=50.0).
    pub cents: f32,
}

impl NoteReading {
    /// Convert a frequency to the nearest note given an `a4` reference (e.g. 440).
    pub fn from_freq(freq: f32, a4: f32) -> Option<Self> {
        if !(freq.is_finite()) || freq <= 0.0 {
            return None;
        }
        // MIDI note number (A4 = 69), fractional.
        let midi = 69.0 + 12.0 * (freq / a4).log2();
        let nearest = midi.round();
        let cents = (midi - nearest) * 100.0;

        let idx = (nearest as i32).rem_euclid(12) as usize;
        let octave = (nearest as i32) / 12 - 1;

        Some(Self {
            freq,
            name: NOTE_NAMES[idx],
            octave,
            cents: cents as f32,
        })
    }

    /// True when within a tight tolerance of the target pitch.
    pub fn in_tune(&self) -> bool {
        self.cents.abs() <= 4.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_is_exact() {
        let r = NoteReading::from_freq(440.0, 440.0).unwrap();
        assert_eq!(r.name, "A");
        assert_eq!(r.octave, 4);
        assert!(r.cents.abs() < 0.01);
        assert!(r.in_tune());
    }

    #[test]
    fn middle_c() {
        let r = NoteReading::from_freq(261.63, 440.0).unwrap();
        assert_eq!(r.name, "C");
        assert_eq!(r.octave, 4);
        assert!(r.cents.abs() < 1.0);
    }

    #[test]
    fn slightly_flat() {
        // ~30 cents flat of A4.
        let f = 440.0 * 2f32.powf(-30.0 / 1200.0);
        let r = NoteReading::from_freq(f, 440.0).unwrap();
        assert_eq!(r.name, "A");
        assert!(r.cents < -25.0 && r.cents > -35.0);
        assert!(!r.in_tune());
    }
}
