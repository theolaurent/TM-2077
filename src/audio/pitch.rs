//! Shared pitch detection: turns a window of PCM samples into a fundamental
//! frequency using the McLeod (MPM) method, which is well suited to the
//! single-voice, monophonic signal of an instrument being tuned.

use pitch_detection::detector::PitchDetector;
use pitch_detection::detector::mcleod::McLeodDetector;

/// Analysis window size (samples). ~43 ms at 48 kHz — long enough to resolve
/// low notes, short enough to feel responsive.
pub const WINDOW: usize = 2048;

pub struct PitchTracker {
    detector: McLeodDetector<f32>,
    sample_rate: usize,
}

impl PitchTracker {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            detector: McLeodDetector::new(WINDOW, WINDOW / 2),
            sample_rate: sample_rate as usize,
        }
    }

    /// Detect the fundamental from the most recent `WINDOW` samples of `buf`.
    /// Returns `None` on silence or an unclear (noisy/polyphonic) signal.
    pub fn detect(&mut self, buf: &[f32]) -> Option<f32> {
        if buf.len() < WINDOW {
            return None;
        }
        let window = &buf[buf.len() - WINDOW..];

        // Cheap RMS gate so background noise doesn't chase the needle.
        let rms = (window.iter().map(|s| s * s).sum::<f32>() / WINDOW as f32).sqrt();
        if rms < 0.01 {
            return None;
        }

        // power_threshold scales with the window energy; clarity rejects
        // ambiguous (non-tonal) input.
        const POWER_THRESHOLD: f32 = 0.15;
        const CLARITY_THRESHOLD: f32 = 0.6;
        self.detector
            .get_pitch(window, self.sample_rate, POWER_THRESHOLD, CLARITY_THRESHOLD)
            .map(|p| p.frequency)
            .filter(|f| f.is_finite() && *f > 20.0 && *f < 5000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, sr: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (std::f32::consts::TAU * freq * i as f32 / sr as f32).sin() * 0.8)
            .collect()
    }

    #[test]
    fn detects_a440() {
        let sr = 48_000;
        let mut t = PitchTracker::new(sr);
        let f = t.detect(&sine(440.0, sr, WINDOW)).expect("should detect");
        assert!((f - 440.0).abs() < 2.0, "got {f}");
    }

    #[test]
    fn detects_low_and_high() {
        let sr = 48_000;
        let mut t = PitchTracker::new(sr);
        for target in [110.0f32, 220.0, 660.0] {
            let f = t.detect(&sine(target, sr, WINDOW)).unwrap();
            assert!(
                (f - target).abs() / target < 0.02,
                "target {target}, got {f}"
            );
        }
    }

    #[test]
    fn silence_is_none() {
        let sr = 48_000;
        let mut t = PitchTracker::new(sr);
        assert!(t.detect(&vec![0.0; WINDOW]).is_none());
    }
}
