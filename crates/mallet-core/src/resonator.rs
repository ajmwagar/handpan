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
