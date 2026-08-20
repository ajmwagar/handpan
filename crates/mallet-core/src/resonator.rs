//! Second-order modal resonator (impulse-normalized decaying sinusoid). The
//! reusable Tier-1 primitive — a bank of these is any struck/plucked timbre.

use crate::mathf;

#[derive(Clone, Copy, Default)]
pub struct Resonator {
    a1: f32,
    a2: f32,
    g: f32,
    y1: f32,
    y2: f32,
}

impl Resonator {
    /// Tune to `freq` Hz, −60 dB decay time `t60` s, linear amplitude `gain`.
    pub fn set(&mut self, freq: f32, t60: f32, gain: f32, fs: f32) {
        let w = core::f32::consts::TAU * freq / fs;
        let r = mathf::exp(-6.907_755_5 / (t60 * fs));
        self.a1 = 2.0 * r * mathf::cos(w);
        self.a2 = -(r * r);
        // g = gain·sin(ω) → a unit strike rings at amplitude `gain`.
        self.g = gain * mathf::sin(w);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.a1 * self.y1 + self.a2 * self.y2 + self.g * x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    #[inline]
    pub fn damp_state(&mut self, factor: f32) {
        self.y1 *= factor;
        self.y2 *= factor;
    }

    pub fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// A gentle constant-peak-gain bandpass (RBJ biquad) — the tuned-tube resonance
/// sitting under a marimba/vibraphone bar. Fed the bar output continuously, it
/// emphasises the struck fundamental and adds the hollow body "hoot" without
/// shifting the perceived pitch. Kept subtle via a small dry/wet mix.
#[derive(Clone, Copy, Default)]
pub struct BandPass {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BandPass {
    /// Tune to `freq` Hz with quality `q` (higher = narrower). The band peaks at
    /// unity gain at `freq`, so the wet path only colours, never runs away.
    pub fn set(&mut self, freq: f32, q: f32, fs: f32) {
        let w0 = core::f32::consts::TAU * freq / fs;
        let cw = mathf::cos(w0);
        let sw = mathf::sin(w0);
        let alpha = sw / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        // Constant-peak-gain bandpass: b0 = alpha, b1 = 0, b2 = -alpha.
        self.b0 = alpha / a0;
        self.b2 = -alpha / a0;
        self.a1 = (-2.0 * cw) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}
