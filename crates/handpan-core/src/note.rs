//! A single handpan tone field: one fundamental voiced as a bank of modal
//! resonators, driven by a short strike-excitation burst.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::mathf;
use crate::resonator::Resonator;
use crate::rng::Rng;

/// One partial of a tone field: frequency ratio to the fundamental, linear
/// amplitude, and a decay-time multiplier (higher partials die away sooner).
#[derive(Clone, Copy)]
pub struct ModeSpec {
    pub ratio: f32,
    pub gain: f32,
    pub decay: f32,
}

/// Default handpan timbre. The character comes from the harmonically-tuned
/// octave (2:1) and compound fifth (3:1) — the intervals handpan makers
/// hand-tune into every note. The fundamental and octave are each voiced as
/// a *doublet* a few cents apart: real struck shells split every mode into
/// two close frequencies, and the resulting slow beating is what makes a
/// handpan shimmer instead of sounding like a synth bell.
pub const HANDPAN_TIMBRE: &[ModeSpec] = &[
    ModeSpec { ratio: 1.0000, gain: 0.58, decay: 1.00 }, // fundamental
    ModeSpec { ratio: 1.0041, gain: 0.52, decay: 1.00 }, // ...split ~+7 cents
    ModeSpec { ratio: 2.0000, gain: 0.34, decay: 0.70 }, // octave
    ModeSpec { ratio: 2.0058, gain: 0.30, decay: 0.70 }, // ...split ~+5 cents
    ModeSpec { ratio: 3.0000, gain: 0.30, decay: 0.55 }, // compound fifth
    ModeSpec { ratio: 4.0000, gain: 0.13, decay: 0.40 }, // double octave
    ModeSpec { ratio: 5.4200, gain: 0.09, decay: 0.28 }, // inharmonic shimmer
    ModeSpec { ratio: 6.7900, gain: 0.06, decay: 0.22 }, // inharmonic shimmer
];

struct ModeVoice {
    res: Resonator,
    ratio: f32,
}

/// A struck tone field. `process()` advances one sample; `strike()` injects a
/// fresh excitation burst.
pub struct NoteVoice {
    modes: Vec<ModeVoice>,
    exc_remaining: u32,
    exc_len: u32,
    exc_amp: f32,
    exc_vel: f32,
    // One-pole low-pass on the excitation — the difference between a hard
    // mallet and a soft fingertip attack.
    lp: f32,
    lp_a: f32,
    rng: Rng,
}

impl NoteVoice {
    /// `tune_mult` applies a per-note fabrication detune (a real set of hammered
    /// notes is never mathematically perfect); pass 1.0 for exact tuning.
    pub fn new(
        freq: f32,
        t60_base: f32,
        timbre: &[ModeSpec],
        fs: f32,
        seed: u32,
        tune_mult: f32,
    ) -> Self {
        let f0 = freq * tune_mult;
        let mut modes = Vec::with_capacity(timbre.len());
        for m in timbre {
            let f = f0 * m.ratio;
            // Drop partials that would alias above Nyquist.
            if f >= fs * 0.49 {
                continue;
            }
            let mut res = Resonator::default();
            res.set(f, t60_base * m.decay, m.gain, fs);
            modes.push(ModeVoice { res, ratio: m.ratio });
        }
        // ~5.5 kHz attack low-pass.
        let lp_a = 1.0 - mathf::exp(-core::f32::consts::TAU * 5500.0 / fs);
        Self {
            modes,
            exc_remaining: 0,
            exc_len: (0.004 * fs) as u32, // ~4 ms hand-strike burst
            exc_amp: 0.0,
            exc_vel: 0.0,
            lp: 0.0,
            lp_a,
            rng: Rng::new(seed),
        }
    }

    /// Trigger a strike. `velocity` in [0, 1] sets loudness and brightness.
    pub fn strike(&mut self, velocity: f32) {
        let v = velocity.clamp(0.0, 1.0);
        // Keep the louder of an in-flight strike and this one, so soft
        // sympathetic excitation never stomps a hard direct hit.
        self.exc_amp = self.exc_amp.max(v);
        self.exc_vel = self.exc_vel.max(v);
        self.exc_remaining = self.exc_len;
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        let raw = if self.exc_remaining > 0 {
            let n = self.exc_len - self.exc_remaining;
            let t = n as f32 / self.exc_len as f32;
            let env = mathf::exp(-6.0 * t);
            self.exc_remaining -= 1;
            let noise = self.rng.next_bipolar();
            let click = if n == 0 { 1.0 } else { 0.0 };
            self.exc_amp * (env * 0.7 * noise + click)
        } else {
            0.0
        };
        // Soften the attack; a fingertip is not a click.
        self.lp += self.lp_a * (raw - self.lp);
        let exc = self.lp;

        let mut sum = 0.0;
        for m in &mut self.modes {
            // Harder strikes throw relatively more energy into the upper
            // partials — the "bloom" of a struck shell.
            let w = 1.0 + self.exc_vel * (m.ratio - 1.0) * 0.12;
            sum += m.res.process(exc * w);
        }

        // Decay the brightness state once the burst is spent.
        if self.exc_remaining == 0 {
            self.exc_vel *= 0.999;
            self.exc_amp = 0.0;
        }
        sum
    }
}
