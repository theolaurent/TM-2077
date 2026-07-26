//! Musical pitch helpers: converting a detected frequency into a note name,
//! octave and cents deviation, relative to a configurable A4 reference and a
//! selectable scale (chromatic, guitar strings, or quarter tones).

use serde::{Deserialize, Serialize};

pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Which set of pitches the tuner snaps a reading to.
#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Scale {
    /// All twelve semitones (12-TET).
    #[default]
    Chromatic,
    /// Standard guitar open strings only: E A D G B.
    Guitar,
    /// Quarter tones (24-TET).
    QuarterTone,
}

/// Instrument transposition: the tuner shows the *written* note for a
/// transposing instrument (written = concert pitch + this many semitones).
#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Transposition {
    /// Concert pitch (C instruments), no transposition.
    #[default]
    Concert,
    /// B♭ instruments (clarinet, trumpet, tenor/soprano sax): +2.
    BFlat,
    /// E♭ instruments (alto/baritone sax): +9.
    EFlat,
    /// F instruments (French horn, English horn): +7.
    F,
}

impl Transposition {
    fn semitones(self) -> i32 {
        match self {
            Transposition::Concert => 0,
            Transposition::BFlat => 2,
            Transposition::EFlat => 9,
            Transposition::F => 7,
        }
    }
}

/// A quarter-tone accidental on top of `name` (QuarterTone scale only).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QuarterTone {
    #[default]
    None,
    /// A quarter tone above `name` (half-sharp).
    HalfSharp,
    /// A quarter tone below `name` (half-flat).
    HalfFlat,
}

/// A pitch reading derived from a detected frequency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteReading {
    /// The raw detected frequency in Hz.
    pub freq: f32,
    /// Note letter, e.g. "A" or "C#".
    pub name: &'static str,
    /// Scientific-pitch octave number (A4 = octave 4).
    pub octave: i32,
    /// Deviation from the nearest target pitch, in cents.
    pub cents: f32,
    /// Quarter-tone accidental relative to `name` (QuarterTone scale only).
    pub quarter: QuarterTone,
}

impl NoteReading {
    /// Convert a frequency to the nearest pitch of `scale`, given an `a4`
    /// reference (e.g. 440) and instrument `transpose`. For a transposing
    /// instrument the returned note is the *written* note.
    pub fn from_freq(freq: f32, a4: f32, scale: Scale, transpose: Transposition) -> Option<Self> {
        if !freq.is_finite() || freq <= 0.0 || !a4.is_finite() || a4 <= 0.0 {
            return None;
        }
        // MIDI note number (A4 = 69), fractional; shift to the written pitch.
        let midi = 69.0 + 12.0 * (freq / a4).log2() + transpose.semitones() as f32;
        match scale {
            Scale::Chromatic => Self::at_semitone(freq, midi, midi.round() as i32),
            Scale::Guitar => Self::guitar(freq, midi),
            Scale::QuarterTone => Self::quarter(freq, midi),
        }
    }

    /// Reading for the integer semitone `nearest` (a MIDI number).
    fn at_semitone(freq: f32, midi: f32, nearest: i32) -> Option<Self> {
        let name = *NOTE_NAMES.get(nearest.rem_euclid(12) as usize)?;
        Some(Self {
            freq,
            name,
            octave: nearest.div_euclid(12) - 1,
            cents: (midi - nearest as f32) * 100.0,
            quarter: QuarterTone::None,
        })
    }

    /// Snap to the nearest standard guitar string pitch class (E A D G B).
    fn guitar(freq: f32, midi: f32) -> Option<Self> {
        const CLASSES: [i32; 5] = [2, 4, 7, 9, 11]; // D E G A B
        let nearest = CLASSES
            .iter()
            .map(|&cls| cls + ((midi - cls as f32) / 12.0).round() as i32 * 12)
            .min_by(|&x, &y| (midi - x as f32).abs().total_cmp(&(midi - y as f32).abs()))?;
        Self::at_semitone(freq, midi, nearest)
    }

    /// Snap to the nearest 24-TET quarter tone.
    fn quarter(freq: f32, midi: f32) -> Option<Self> {
        let step = (midi * 2.0).round() as i32; // quarter-tone steps (0.5 semitone each)
        if step.rem_euclid(2) == 0 {
            // Lands on a semitone — an ordinary natural/sharp note.
            return Self::at_semitone(freq, midi, step.div_euclid(2));
        }
        // A quarter tone between two semitones. Notate it on the nearby natural
        // note: half-sharp above it, or half-flat below the upper natural.
        let lower = step.div_euclid(2);
        let lower_name = *NOTE_NAMES.get(lower.rem_euclid(12) as usize)?;
        let (base, quarter) = if lower_name.ends_with('#') {
            (lower + 1, QuarterTone::HalfFlat)
        } else {
            (lower, QuarterTone::HalfSharp)
        };
        Some(Self {
            freq,
            name: *NOTE_NAMES.get(base.rem_euclid(12) as usize)?,
            octave: base.div_euclid(12) - 1,
            cents: (midi - step as f32 * 0.5) * 100.0,
            quarter,
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
        let r = NoteReading::from_freq(440.0, 440.0, Scale::Chromatic, Transposition::Concert).unwrap();
        assert_eq!(r.name, "A");
        assert_eq!(r.octave, 4);
        assert!(r.cents.abs() < 0.01);
        assert!(r.in_tune());
    }

    #[test]
    fn middle_c() {
        let r = NoteReading::from_freq(261.63, 440.0, Scale::Chromatic, Transposition::Concert).unwrap();
        assert_eq!(r.name, "C");
        assert_eq!(r.octave, 4);
        assert!(r.cents.abs() < 1.0);
    }

    #[test]
    fn slightly_flat() {
        // ~30 cents flat of A4.
        let f = 440.0 * 2f32.powf(-30.0 / 1200.0);
        let r = NoteReading::from_freq(f, 440.0, Scale::Chromatic, Transposition::Concert).unwrap();
        assert_eq!(r.name, "A");
        assert!(r.cents < -25.0 && r.cents > -35.0);
        assert!(!r.in_tune());
    }

    #[test]
    fn guitar_keeps_string_notes() {
        // A2 (110 Hz) is an open string — snaps to A, in tune.
        let r = NoteReading::from_freq(110.0, 440.0, Scale::Guitar, Transposition::Concert).unwrap();
        assert_eq!(r.name, "A");
        assert!(r.cents.abs() < 1.0);
    }

    #[test]
    fn guitar_snaps_c_to_b() {
        // C4 isn't a string; the nearest string note is B (a semitone below).
        let r = NoteReading::from_freq(261.63, 440.0, Scale::Guitar, Transposition::Concert).unwrap();
        assert_eq!(r.name, "B");
        assert!((r.cents - 100.0).abs() < 2.0);
    }

    #[test]
    fn quarter_tone_half_sharp() {
        // +50 cents above A4 → A half-sharp.
        let f = 440.0 * 2f32.powf(0.5 / 12.0);
        let r = NoteReading::from_freq(f, 440.0, Scale::QuarterTone, Transposition::Concert).unwrap();
        assert_eq!(r.name, "A");
        assert_eq!(r.quarter, QuarterTone::HalfSharp);
        assert!(r.cents.abs() < 1.0);
    }

    #[test]
    fn quarter_tone_half_flat() {
        // Quarter tone between A# and B → B half-flat.
        let f = 440.0 * 2f32.powf(1.5 / 12.0);
        let r = NoteReading::from_freq(f, 440.0, Scale::QuarterTone, Transposition::Concert).unwrap();
        assert_eq!(r.name, "B");
        assert_eq!(r.quarter, QuarterTone::HalfFlat);
        assert!(r.cents.abs() < 1.0);
    }

    #[test]
    fn bflat_transposes_up_a_tone() {
        // Concert A4 on a B♭ instrument reads as written B4.
        let r = NoteReading::from_freq(440.0, 440.0, Scale::Chromatic, Transposition::BFlat).unwrap();
        assert_eq!(r.name, "B");
        assert_eq!(r.octave, 4);
        assert!(r.cents.abs() < 0.01);
    }
}
