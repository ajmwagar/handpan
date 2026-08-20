//! Microtonal tuning: **Scala (`.scl`) scales** and a **quantizer**.
//!
//! A [`ScalaScale`] is a list of pitch degrees within a repeating period (the
//! octave for most scales, but Scala allows any period — Bohlen-Pierce's 3/1,
//! stretched octaves, etc.). [`parse_scl`] reads the standard `.scl` text format
//! (comments `!`, a description line, a count, then N ratios or cents; the last
//! entry is the period). [`Quantizer`] snaps a continuous pitch to the nearest
//! scale degree — the heart of a microtonal V/Oct quantizer module.
//!
//! `no_std` + `alloc`: parsing works on a `&str` (the caller reads the file);
//! there is no filesystem dependency, so this runs on firmware too.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

use crate::mathf;

/// A parsed Scala scale: degrees in cents within one period.
#[derive(Clone, Debug)]
pub struct ScalaScale {
    /// Free-text description from the file (first non-comment line).
    pub description: String,
    /// The repeat interval in cents (2/1 = 1200 for an octave scale).
    pub period_cents: f32,
    /// Ascending degree offsets in cents within `[0, period)`, starting at the
    /// implicit `0.0` (the 1/1). The period itself is not included (it wraps to
    /// `0.0` of the next period).
    pub degrees: Vec<f32>,
}

/// Parse the standard Scala `.scl` text format. Returns `None` on malformed
/// input (bad count, unparseable pitch, empty scale).
pub fn parse_scl(text: &str) -> Option<ScalaScale> {
    // Meaningful lines: skip blanks and `!` comments.
    let mut lines = text.lines().filter_map(|l| {
        let t = l.trim();
        if t.is_empty() || t.starts_with('!') {
            None
        } else {
            Some(t)
        }
    });

    let description = String::from(lines.next()?);
    let count: usize = lines.next()?.split_whitespace().next()?.parse().ok()?;
    if count == 0 {
        return None;
    }

    let mut pitches = Vec::with_capacity(count);
    for _ in 0..count {
        let tok = lines.next()?.split_whitespace().next()?;
        pitches.push(parse_pitch(tok)?);
    }

    // The last pitch is the period; the rest (with an implicit 0) are the degrees.
    let period_cents = *pitches.last()?;
    if period_cents <= 0.0 {
        return None;
    }
    let mut degrees = Vec::with_capacity(count);
    degrees.push(0.0);
    for &p in &pitches[..pitches.len() - 1] {
        degrees.push(p);
    }
    Some(ScalaScale { description, period_cents, degrees })
}

/// One Scala pitch token → cents. A token with `.` is cents; otherwise it is a
/// ratio `a/b` (or a bare integer `a` meaning `a/1`).
fn parse_pitch(tok: &str) -> Option<f32> {
    if tok.contains('.') {
        tok.parse::<f32>().ok()
    } else if let Some((n, d)) = tok.split_once('/') {
        let num: f32 = n.trim().parse().ok()?;
        let den: f32 = d.trim().parse().ok()?;
        if num <= 0.0 || den <= 0.0 {
            return None;
        }
        Some(1200.0 * mathf::log2(num / den))
    } else {
        let num: f32 = tok.parse().ok()?;
        if num <= 0.0 {
            return None;
        }
        Some(1200.0 * mathf::log2(num))
    }
}

impl ScalaScale {
    /// 12-tone equal temperament — the built-in default.
    pub fn edo12() -> Self {
        let mut degrees = Vec::with_capacity(12);
        for i in 0..12 {
            degrees.push(i as f32 * 100.0);
        }
        ScalaScale { description: String::from("12-EDO"), period_cents: 1200.0, degrees }
    }

    /// Equal division of the octave into `n` steps.
    pub fn edo(n: usize) -> Self {
        let n = n.max(1);
        let step = 1200.0 / n as f32;
        let degrees = (0..n).map(|i| i as f32 * step).collect();
        ScalaScale { description: String::from("EDO"), period_cents: 1200.0, degrees }
    }

    /// Snap an absolute pitch (cents from the 1/1) to the nearest degree.
    pub fn quantize_cents(&self, cents: f32) -> f32 {
        let p = self.period_cents;
        let period_idx = mathf::floor(cents / p);
        let within = cents - period_idx * p;
        // Candidates: every degree in this period plus the period itself
        // (= the 1/1 of the next period, i.e. an exact wrap).
        let mut best = self.degrees[0];
        let mut best_d = (within - best).abs();
        for &deg in &self.degrees[1..] {
            let d = (within - deg).abs();
            if d < best_d {
                best_d = d;
                best = deg;
            }
        }
        if (within - p).abs() < best_d {
            best = p;
        }
        period_idx * p + best
    }
}

/// A real-time microtonal quantizer: snaps a frequency (or V/Oct) to the nearest
/// degree of a [`ScalaScale`], relative to a root frequency (the scale's 1/1).
#[derive(Clone, Debug)]
pub struct Quantizer {
    scale: ScalaScale,
    root_hz: f32,
}

impl Quantizer {
    /// Build with a scale and the root frequency its 1/1 sits on.
    pub fn new(scale: ScalaScale, root_hz: f32) -> Self {
        Quantizer { scale, root_hz: root_hz.max(1.0) }
    }

    /// Replace the scale (e.g. loading a new `.scl`).
    pub fn set_scale(&mut self, scale: ScalaScale) {
        self.scale = scale;
    }

    /// Retune the root (the frequency the scale's 1/1 maps to).
    pub fn set_root_hz(&mut self, hz: f32) {
        self.root_hz = hz.max(1.0);
    }

    /// The scale being used.
    pub fn scale(&self) -> &ScalaScale {
        &self.scale
    }

    /// Snap a frequency to the nearest scale degree (Hz in → Hz out).
    pub fn quantize_hz(&self, hz: f32) -> f32 {
        if hz <= 0.0 {
            return self.root_hz;
        }
        let cents = 1200.0 * mathf::log2(hz / self.root_hz);
        let q = self.scale.quantize_cents(cents);
        self.root_hz * mathf::exp2(q / 1200.0)
    }

    /// Snap a 1 V/oct pitch (volts, where 0 V = `root_hz`) to the nearest degree,
    /// returned as a frequency. Standard Eurorack V/Oct: one volt per octave.
    pub fn quantize_volts(&self, volts: f32) -> f32 {
        self.quantize_hz(self.root_hz * mathf::exp2(volts))
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn edo12_snaps_to_semitones() {
        let q = Quantizer::new(ScalaScale::edo12(), 440.0);
        // 466 Hz ≈ A#4 (100 cents) — snaps exactly.
        let f = q.quantize_hz(470.0);
        assert!((f - 466.16).abs() < 1.0, "got {f}");
        // A pitch 30 cents sharp of A4 snaps back to 440.
        let f = q.quantize_hz(440.0 * 2f32.powf(30.0 / 1200.0));
        assert!((f - 440.0).abs() < 0.5, "got {f}");
    }

    #[test]
    fn parses_scl_ratios_and_cents_and_nonoctave_period() {
        // A tiny mixed scale: a just fifth (ratio) and a cents value; octave period.
        let scl = "! test.scl\nMy scale\n 3\n 386.31\n 3/2\n 2/1\n";
        let s = parse_scl(scl).expect("parse");
        assert_eq!(s.description, "My scale");
        assert!((s.period_cents - 1200.0).abs() < 0.01);
        // degrees: 0, 386.31, 701.955
        assert_eq!(s.degrees.len(), 3);
        assert!((s.degrees[1] - 386.31).abs() < 0.1);
        assert!((s.degrees[2] - 701.955).abs() < 0.1);

        // Bohlen–Pierce style non-octave period (3/1 = 1901.955 cents).
        let bp = "! bp\nBohlen-Pierce-ish\n 2\n 1/1\n 3/1\n";
        let s = parse_scl(bp).expect("parse bp");
        assert!((s.period_cents - 1901.955).abs() < 0.1, "period {}", s.period_cents);
    }

    #[test]
    fn quantize_is_idempotent_on_scale_degrees() {
        let q = Quantizer::new(ScalaScale::edo(19), 261.63); // 19-EDO on C4
        for v in [-1.5f32, -0.3, 0.0, 0.7, 2.1] {
            let f = q.quantize_volts(v);
            // Re-quantizing a quantized frequency must not move it.
            assert!((q.quantize_hz(f) - f).abs() < 0.01, "not idempotent at {v}: {f}");
        }
    }
}
