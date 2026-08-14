//! Second-order modal resonator — the parallel-biquad primitive a modal
//! voice is built from. Its impulse response is a decaying sinusoid, so a
//! bank of these reproduces the inharmonic partial structure of a struck
//! metal shell directly, without any circuit/WDF machinery.

use crate::mathf;

/// A single mode: a two-pole resonator (a1·y[n-1] + a2·y[n-2] + g·x[n]).
#[derive(Clone, Copy, Debug, Default)]
pub struct Resonator {
    a1: f32,
    a2: f32,
    g: f32,
    y1: f32,
    y2: f32,
}

impl Resonator {
    /// Tune this mode to `freq` Hz with a −60 dB decay time of `t60` seconds,
    /// contributing linear amplitude `gain`, at sample rate `fs`.
    pub fn set(&mut self, freq: f32, t60: f32, gain: f32, fs: f32) {
        let w = core::f32::consts::TAU * freq / fs;
        // Pole radius from T60:  r^(t60·fs) = 10^(-3)  =>  r = e^(-ln(1000)/(t60·fs)).
        let r = mathf::exp(-6.907_755_5 / (t60 * fs));
        self.a1 = 2.0 * r * mathf::cos(w);
        self.a2 = -(r * r);
        // Impulse normalization: this filter's impulse response is
        // g·rⁿ·sin(ω(n+1))/sin(ω), so g = gain·sin(ω) makes a unit strike
        // ring at amplitude `gain` regardless of decay time.
        self.g = gain * mathf::sin(w);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.a1 * self.y1 + self.a2 * self.y2 + self.g * x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}
