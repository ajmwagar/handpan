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
/// hand-tune into every note — plus a few inharmonic shimmer modes on top.
pub const HANDPAN_TIMBRE: &[ModeSpec] = &[
    ModeSpec { ratio: 1.00, gain: 1.00, decay: 1.00 }, // fundamental
    ModeSpec { ratio: 2.00, gain: 0.55, decay: 0.70 }, // octave
    ModeSpec { ratio: 3.00, gain: 0.32, decay: 0.55 }, // compound fifth
    ModeSpec { ratio: 4.00, gain: 0.14, decay: 0.40 }, // double octave
    ModeSpec { ratio: 5.42, gain: 0.09, decay: 0.28 }, // inharmonic shimmer
    ModeSpec { ratio: 6.79, gain: 0.06, decay: 0.22 }, // inharmonic shimmer
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
    rng: Rng,
}

impl NoteVoice {
    pub fn new(freq: f32, t60_base: f32, timbre: &[ModeSpec], fs: f32, seed: u32) -> Self {
        let mut modes = Vec::with_capacity(timbre.len());
        for m in timbre {
            let f = freq * m.ratio;
            // Drop partials that would alias above Nyquist.
            if f >= fs * 0.49 {
                continue;
            }
            let mut res = Resonator::default();
            res.set(f, t60_base * m.decay, m.gain, fs);
            modes.push(ModeVoice { res, ratio: m.ratio });
        }
        Self {
            modes,
            exc_remaining: 0,
            exc_len: (0.004 * fs) as u32, // ~4 ms hand-strike burst
            exc_amp: 0.0,
            exc_vel: 0.0,
            rng: Rng::new(seed),
        }
    }

    /// Trigger a strike. `velocity` in [0, 1] sets loudness and brightness.
    pub fn strike(&mut self, velocity: f32) {
        let v = velocity.clamp(0.0, 1.0);
        // Keep the louder of an in-flight strike and this one, so soft
        // sympathetic excitation never stomps a hard direct hit.
        self.exc_amp = self.exc_amp.max(v);
        self.exc_vel = v;
        self.exc_remaining = self.exc_len;
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        let exc = if self.exc_remaining > 0 {
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

        let mut sum = 0.0;
        for m in &mut self.modes {
            // Harder strikes throw relatively more energy into the upper
            // partials — the "bloom" of a struck shell.
            let w = 1.0 + self.exc_vel * (m.ratio - 1.0) * 0.12;
            sum += m.res.process(exc * w);
        }
        sum
    }
}
