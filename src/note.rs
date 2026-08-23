//! Musical notation and pitch mapping. A `Note` is a natural name plus an
//! optional accidental (including quarter-tone demi-accidentals). A `Scale` is a
//! list of spelled degrees, each with a frequency ratio to the tonic (Scala
//! `.scl`-style). Detected frequencies snap to the nearest degree, anchored so
//! natural A in octave 4 sounds at the `a4` reference.

use imbl::Vector;
use serde::{Deserialize, Serialize};

/// One of the seven natural note names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Name {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

impl Name {
    /// Semitones above C within an octave (C = 0 … B = 11).
    fn semitones_from_c(self) -> f64 {
        match self {
            Name::C => 0.0,
            Name::D => 2.0,
            Name::E => 4.0,
            Name::F => 5.0,
            Name::G => 7.0,
            Name::A => 9.0,
            Name::B => 11.0,
        }
    }

    // Terse spelling constructors so the degree tables read `A.sharp()`,
    // `B.dflat()`, etc. Only the accidentals the built-in scales use exist.
    const fn nat(self) -> Note {
        Note {
            name: self,
            accidental: None,
        }
    }
    const fn sharp(self) -> Note {
        Note {
            name: self,
            accidental: Some(Accidental::Sharp),
        }
    }
    const fn dsharp(self) -> Note {
        Note {
            name: self,
            accidental: Some(Accidental::Demisharp),
        }
    }
    const fn dflat(self) -> Note {
        Note {
            name: self,
            accidental: Some(Accidental::Demiflat),
        }
    }
}

/// An accidental applied to a natural name. Demiflat/Demisharp are the
/// quarter-tone (24-TET) accidentals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accidental {
    Flat,
    Demiflat,
    Demisharp,
    Sharp,
}

impl Accidental {
    /// Semitone offset the accidental applies to its natural name.
    fn semitones(self) -> f64 {
        match self {
            Accidental::Flat => -1.0,
            Accidental::Demiflat => -0.5,
            Accidental::Demisharp => 0.5,
            Accidental::Sharp => 1.0,
        }
    }
}

/// A spelled note: a natural name plus an optional accidental.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    pub name: Name,
    pub accidental: Option<Accidental>,
}

impl Note {
    /// This note's pitch, in semitones above natural A, reduced into `[0, 12)`.
    /// Used to derive each degree's frequency ratio for the built-in 12-/24-TET
    /// scales.
    fn semitones_from_a(self) -> f64 {
        let acc = self.accidental.map_or(0.0, Accidental::semitones);
        (self.name.semitones_from_c() - Name::A.semitones_from_c() + acc).rem_euclid(12.0)
    }
}

/// One scale degree: a spelled note and its frequency ratio to the tonic (`1/1`
/// = natural A). Storing the ratio explicitly leaves room for arbitrary (Scala
/// `.scl`) scales later; the built-ins derive it from the note.
#[derive(Clone, Copy)]
struct Degree {
    note: Note,
    ratio: f64,
}

/// A degree whose ratio is the 12-/24-TET ratio of its spelled note.
fn degree(note: Note) -> Degree {
    Degree {
        note,
        ratio: 2f64.powf(note.semitones_from_a() / 12.0),
    }
}

/// Which set of pitches the tuner snaps a reading to.
#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Scale {
    /// All twelve semitones (12-TET), spelled with sharps only.
    #[default]
    Chromatic,
    /// Standard guitar open strings only: E A D G B.
    Guitar,
    /// Quarter tones (24-TET).
    QuarterTone,
}

impl Scale {
    /// The scale's degrees, ascending within one octave, each with its spelled
    /// note and ratio. Spelling lives here: Chromatic is sharp-only, Guitar is
    /// naturals, QuarterTone uses demi-accidentals.
    ///
    /// **Why `1/1` = A:** the app is anchored on A4 = `a4` Hz (the one calibration
    /// knob), so rooting every scale at natural A makes A's ratio `1.0` and a
    /// degree's pitch is just `a4 · ratio · 2^octave` — no per-scale tonic offset.
    /// (Scales are conventionally written from C; starting at A is invisible to
    /// the user, who sees the spelled note.)
    ///
    /// **Simplifying assumptions (revisit for imported `.scl`):** period fixed at
    /// `2/1` with the octave degree implied (no non-octave scales); anchoring
    /// assumes a natural-A degree exists (all three built-ins have one); ratios
    /// are derived from the spelling via [`Note::semitones_from_a`], so they can't
    /// drift for 12-/24-TET (imported ratios would be stored on [`Degree`]).
    ///
    /// Rebuilds the `Vector` per call (~60 fps); fine for these small tables.
    fn notes(self) -> Vector<Degree> {
        use Name::{A, B, C, D, E, F, G};
        let notes: &[Note] = match self {
            Scale::Chromatic => &[
                A.nat(),
                A.sharp(),
                B.nat(),
                C.nat(),
                C.sharp(),
                D.nat(),
                D.sharp(),
                E.nat(),
                F.nat(),
                F.sharp(),
                G.nat(),
                G.sharp(),
            ],
            Scale::Guitar => &[A.nat(), B.nat(), D.nat(), E.nat(), G.nat()],
            Scale::QuarterTone => &[
                A.nat(),
                A.dsharp(),
                A.sharp(),
                B.dflat(),
                B.nat(),
                B.dsharp(),
                C.nat(),
                C.dsharp(),
                C.sharp(),
                D.dflat(),
                D.nat(),
                D.dsharp(),
                D.sharp(),
                E.dflat(),
                E.nat(),
                E.dsharp(),
                F.nat(),
                F.dsharp(),
                F.sharp(),
                G.dflat(),
                G.nat(),
                G.dsharp(),
                G.sharp(),
                A.dflat(),
            ],
        };
        notes.iter().copied().map(degree).collect()
    }
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
    fn semitones(self) -> i8 {
        match self {
            Transposition::Concert => 0,
            Transposition::BFlat => 2,
            Transposition::EFlat => 9,
            Transposition::F => 7,
        }
    }
}

/// In-tune tolerance in cents. Single source of truth for both `in_tune()` and
/// the LCD's flat/in-tune/sharp LEDs (`ui::leds`), so they can't drift apart.
pub const IN_TUNE_CENTS: f32 = 4.0;

/// A pitch reading derived from a detected frequency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteReading {
    /// The raw detected frequency in Hz.
    pub freq: f32,
    /// The nearest scale degree's spelled note.
    pub note: Note,
    /// Scientific-pitch octave number (A4 = octave 4).
    pub octave: i32,
    /// Deviation from that degree's pitch, in cents.
    pub cents: f32,
}

impl NoteReading {
    /// Convert a frequency to the nearest degree of `scale`, given an `a4`
    /// reference (e.g. 440) and instrument `transpose`. For a transposing
    /// instrument the returned note is the *written* note. Octaves repeat at
    /// `2/1`, anchored so natural A in octave 4 sounds at `a4`.
    pub fn from_freq(freq: f32, a4: f32, scale: Scale, transpose: Transposition) -> Option<Self> {
        if !freq.is_finite() || freq <= 0.0 || !a4.is_finite() || a4 <= 0.0 {
            return None;
        }
        // Shift to the written pitch, then measure in octaves above A4 (x = 0 → A4).
        let written = freq as f64 * 2f64.powf(transpose.semitones() as f64 / 12.0);
        let x = (written / a4 as f64).log2();

        // Pick the degree (in any octave) closest to x in log-frequency.
        let (note, pos) = scale
            .notes()
            .iter()
            .map(|d| {
                let rel = d.ratio.log2(); // degree offset from A, in [0, 1)
                let pos = rel + (x - rel).round(); // nearest octave copy
                (d.note, pos, (x - pos).abs())
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(note, pos, _)| (note, pos))?;

        let cents = ((x - pos) * 1200.0) as f32;
        // Scientific octaves change at C, so the octave must follow the *letter*,
        // not the raw pitch: a quarter-tone accidental sits on a half-semitone
        // (B half-sharp ≈ MIDI 71.5) that would round across the B/C boundary and
        // mislabel B4 as B5. Anchor on the natural name (a whole semitone) and take
        // the nearest octave copy. A4 = MIDI 69.
        let rel_nat =
            (note.name.semitones_from_c() - Name::A.semitones_from_c()).rem_euclid(12.0) / 12.0;
        let pos_nat = rel_nat + (pos - rel_nat).round();
        let octave = ((69.0 + pos_nat * 12.0).round() as i32).div_euclid(12) - 1;
        Some(Self {
            freq,
            note,
            octave,
            cents,
        })
    }

    /// True when within a tight tolerance of the target pitch.
    pub fn in_tune(&self) -> bool {
        self.cents.abs() <= IN_TUNE_CENTS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(freq: f32, scale: Scale, transpose: Transposition) -> NoteReading {
        NoteReading::from_freq(freq, 440.0, scale, transpose).expect("should detect")
    }

    fn note(name: Name, accidental: Option<Accidental>) -> Note {
        Note { name, accidental }
    }

    #[test]
    fn a4_is_exact() {
        let r = reading(440.0, Scale::Chromatic, Transposition::Concert);
        assert_eq!(r.note, note(Name::A, None));
        assert_eq!(r.octave, 4);
        assert!(r.cents.abs() < 0.01);
        assert!(r.in_tune());
    }

    #[test]
    fn middle_c() {
        let r = reading(261.63, Scale::Chromatic, Transposition::Concert);
        assert_eq!(r.note, note(Name::C, None));
        assert_eq!(r.octave, 4);
        assert!(r.cents.abs() < 1.0);
    }

    #[test]
    fn slightly_flat() {
        // ~30 cents flat of A4.
        let f = 440.0 * 2f32.powf(-30.0 / 1200.0);
        let r = reading(f, Scale::Chromatic, Transposition::Concert);
        assert_eq!(r.note, note(Name::A, None));
        assert!(r.cents < -25.0 && r.cents > -35.0);
        assert!(!r.in_tune());
    }

    #[test]
    fn guitar_keeps_string_notes() {
        // A2 (110 Hz) is an open string — snaps to A, in tune.
        let r = reading(110.0, Scale::Guitar, Transposition::Concert);
        assert_eq!(r.note, note(Name::A, None));
        assert_eq!(r.octave, 2);
        assert!(r.cents.abs() < 1.0);
    }

    #[test]
    fn guitar_snaps_c_to_b() {
        // C4 isn't a string; the nearest string note is B (a semitone below).
        let r = reading(261.63, Scale::Guitar, Transposition::Concert);
        assert_eq!(r.note, note(Name::B, None));
        assert!((r.cents - 100.0).abs() < 2.0);
    }

    #[test]
    fn quarter_tone_half_sharp() {
        // +50 cents above A4 → A half-sharp.
        let f = 440.0 * 2f32.powf(0.5 / 12.0);
        let r = reading(f, Scale::QuarterTone, Transposition::Concert);
        assert_eq!(r.note, note(Name::A, Some(Accidental::Demisharp)));
        assert!(r.cents.abs() < 1.0);
    }

    #[test]
    fn quarter_tone_half_flat() {
        // Quarter tone between A# and B → B half-flat.
        let f = 440.0 * 2f32.powf(1.5 / 12.0);
        let r = reading(f, Scale::QuarterTone, Transposition::Concert);
        assert_eq!(r.note, note(Name::B, Some(Accidental::Demiflat)));
        assert!(r.cents.abs() < 1.0);
    }

    #[test]
    fn quarter_tone_octave_follows_letter() {
        // B half-sharp (~508 Hz) is a raised B4. Its pitch rounds up across the
        // B/C octave boundary, but the octave must track the *letter*: B4, not B5.
        let f = 440.0 * 2f32.powf(2.5 / 12.0);
        let r = reading(f, Scale::QuarterTone, Transposition::Concert);
        assert_eq!(r.note, note(Name::B, Some(Accidental::Demisharp)));
        assert_eq!(r.octave, 4);
    }

    #[test]
    fn bflat_transposes_up_a_tone() {
        // Concert A4 on a B♭ instrument reads as written B4.
        let r = reading(440.0, Scale::Chromatic, Transposition::BFlat);
        assert_eq!(r.note, note(Name::B, None));
        assert_eq!(r.octave, 4);
        assert!(r.cents.abs() < 0.01);
    }

    #[test]
    fn chromatic_is_sharp_only() {
        // Whatever the pitch, Chromatic never spells a flat or demi-accidental.
        let f = 440.0 * 2f32.powf(-4.5 / 12.0);
        let r = reading(f, Scale::Chromatic, Transposition::Concert);
        assert!(matches!(r.note.accidental, None | Some(Accidental::Sharp)));
    }
}
