//! A single tone field (handpan note or drum tongue): one fundamental voiced
//! as a bank of modal resonators, driven by a short strike-excitation burst,
//! with an amplitude-dependent pitch bloom on the low partials.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::mathf;
use crate::preset::VoiceProfile;
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

/// Dimpled/domed handpan tone field. Measured-informed: partial ratios, gains,
/// and decays come from analysis of real handpan recordings (see
/// `docs/measured_modes.md`). The defining correction over a naive model is
/// that the **octave and compound fifth ring ~1.7–2× longer than the
/// fundamental** — the fundamental fades first while the upper partials sing,
/// which is the sustained shimmer of a good handpan. The fundamental, octave,
/// and fifth are voiced as detuned doublets because real shells split those
/// modes (measured), giving the slow beating; two fast high modes give the
/// metallic attack "tak".
pub const HANDPAN_TIMBRE: &[ModeSpec] = &[
    ModeSpec { ratio: 1.0000, gain: 0.58, decay: 0.90 }, // fundamental
    ModeSpec { ratio: 1.0041, gain: 0.50, decay: 0.90 }, // ...split ~+7 cents
    ModeSpec { ratio: 2.0000, gain: 0.41, decay: 1.50 }, // octave (rings long)
    ModeSpec { ratio: 2.0060, gain: 0.34, decay: 1.50 }, // ...split (measured)
    ModeSpec { ratio: 3.0000, gain: 0.26, decay: 1.70 }, // fifth (rings longest)
    ModeSpec { ratio: 3.0050, gain: 0.13, decay: 1.70 }, // ...split (measured)
    ModeSpec { ratio: 4.0000, gain: 0.05, decay: 0.90 }, // double octave
    ModeSpec { ratio: 5.2500, gain: 0.05, decay: 0.55 }, // inharmonic shimmer
    ModeSpec { ratio: 6.8100, gain: 0.05, decay: 0.20 }, // metallic transient
    ModeSpec { ratio: 8.9000, gain: 0.03, decay: 0.07 }, // metallic transient
];

/// Cut steel-tongue-drum field. A cut tongue vibrates like a clamped beam:
/// fundamental-dominant, a weak tuned octave, a stretched inharmonic overtone,
/// and little of the shimmer/coupling of a handpan — purer, more "music box".
pub const TONGUE_DRUM_TIMBRE: &[ModeSpec] = &[
    ModeSpec { ratio: 1.0000, gain: 0.90, decay: 1.00 }, // fundamental (dominant)
    ModeSpec { ratio: 2.0000, gain: 0.16, decay: 0.55 }, // weak tuned octave
    ModeSpec { ratio: 2.7600, gain: 0.13, decay: 0.42 }, // stretched (beam-like)
    ModeSpec { ratio: 5.4000, gain: 0.05, decay: 0.16 }, // high inharmonic
    ModeSpec { ratio: 8.1000, gain: 0.03, decay: 0.06 }, // metallic transient
];

/// Control-rate block for the pitch-bloom retune (samples).
const CTL: u32 = 32;

struct ModeVoice {
    res: Resonator,
    base_freq: f32,
    t60: f32,
    gain: f32,
    ratio: f32,
    /// Whether this mode participates in the pitch bloom (low partials only).
    bloom: bool,
    /// Whether this is a fundamental partial (drives the nonlinearity).
    is_fund: bool,
}

/// A struck tone field. `process()` advances one sample; `strike()` injects a
/// fresh excitation burst.
pub struct NoteVoice {
    modes: Vec<ModeVoice>,
    fs: f32,
    exc_remaining: u32,
    exc_len: u32,
    exc_amp: f32,
    exc_vel: f32,
    // One-pole low-pass on the excitation — mallet vs fingertip.
    lp: f32,
    lp_a: f32,
    // Amplitude-dependent pitch bloom: struck hard, the low partials start
    // slightly sharp and glide down to true pitch as the note settles.
    bloom_cur: f32,   // current offset, cents
    bloom_peak: f32,  // cents at full velocity
    bloom_decay: f32, // per-control-block decay factor
    ctl: u32,
    // Hand-mute: when damping, bleed resonator energy each sample.
    damping: bool,
    damp_coef: f32,
    // Geometric-nonlinearity harmonic generation: the fundamental pumps the
    // octave (quadratic) and fifth (cubic), so hard strikes bloom brighter —
    // the "distortion process" measured in handpans/steelpans.
    nonlin: f32,
    dc_x1: f32,
    dc_y1: f32,
    rng: Rng,
}

impl NoteVoice {
    /// Build a tone field at fundamental `freq` (Hz), using `profile` for
    /// timbre and character. `t60_base` is the fundamental's decay time before
    /// per-mode scaling; `tune_mult` applies fabrication detune.
    pub fn new(freq: f32, t60_base: f32, profile: &VoiceProfile, fs: f32, seed: u32, tune_mult: f32) -> Self {
        let f0 = freq * tune_mult;
        let mut modes = Vec::with_capacity(profile.timbre.len());
        for m in profile.timbre {
            let f = f0 * m.ratio;
            if f >= fs * 0.49 {
                continue; // anti-alias
            }
            // Size/brightness: attenuate upper partials for larger, warmer
            // instruments and lift them for small bright ones.
            let gain = m.gain * mathf::powf(profile.brightness, (m.ratio - 1.0).max(0.0));
            let t60 = t60_base * profile.decay_scale * m.decay;
            let mut res = Resonator::default();
            res.set(f, t60, gain, fs);
            modes.push(ModeVoice {
                res,
                base_freq: f,
                t60,
                gain,
                ratio: m.ratio,
                bloom: m.ratio <= 2.1,
                is_fund: m.ratio <= 1.05,
            });
        }

        let lp_a = 1.0 - mathf::exp(-core::f32::consts::TAU * profile.attack_cutoff_hz / fs);
        let dt = CTL as f32 / fs;
        let bloom_decay = mathf::exp(-dt / (profile.bloom_ms * 0.001).max(1e-4));

        Self {
            modes,
            fs,
            exc_remaining: 0,
            exc_len: ((profile.attack_ms * 0.001) * fs) as u32,
            exc_amp: 0.0,
            exc_vel: 0.0,
            lp: 0.0,
            lp_a,
            bloom_cur: 0.0,
            bloom_peak: profile.bloom_cents,
            bloom_decay,
            ctl: 0,
            damping: false,
            // ~90 ms mute at 48 kHz; recomputed for fs below.
            damp_coef: mathf::exp(-1.0 / (0.09 * fs)),
            nonlin: profile.nonlin,
            dc_x1: 0.0,
            dc_y1: 0.0,
            rng: Rng::new(seed),
        }
    }

    /// Trigger a strike. `velocity` in [0, 1] sets loudness, brightness, and
    /// bloom depth. Each strike varies slightly so repeats never sound
    /// machine-gunned.
    pub fn strike(&mut self, velocity: f32) {
        let v = velocity.clamp(0.0, 1.0);
        self.damping = false; // a strike lifts the hand
        // Per-strike micro-variation in level and bloom depth.
        let jitter_amp = 1.0 + 0.06 * self.rng.next_bipolar();
        let jitter_bloom = 1.0 + 0.15 * self.rng.next_bipolar();
        self.exc_amp = self.exc_amp.max(v * jitter_amp);
        self.exc_vel = self.exc_vel.max(v);
        self.exc_remaining = self.exc_len.max(1);
        self.bloom_cur = self.bloom_cur.max(v * self.bloom_peak * jitter_bloom);
    }

    /// Rest a hand on the field — a fast, natural mute.
    pub fn damp(&mut self) {
        self.damping = true;
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
        self.lp += self.lp_a * (raw - self.lp);
        let exc = self.lp;

        let mut sum = 0.0;
        let mut fund = 0.0;
        for m in &mut self.modes {
            if self.damping {
                m.res.damp_state(self.damp_coef);
            }
            let w = 1.0 + self.exc_vel * (m.ratio - 1.0) * 0.12;
            let y = m.res.process(exc * w);
            sum += y;
            if m.is_fund {
                fund += y;
            }
        }

        // Geometric nonlinearity (the "distortion process"): waveshape the
        // fundamental into octave (f²) and twelfth (f³) content and mix it to
        // the output. Amplitude-dependent, so hard strikes bloom brighter, and
        // bounded — no resonant runaway. K_OCTAVE/K_FIFTH tag which tuned modes
        // the same harmonics reinforce perceptually.
        let f = fund.clamp(-3.0, 3.0);
        sum += self.nonlin * (f * f + f * f * f);

        // Block the DC the squared term introduces (one-pole high-pass).
        let out = sum - self.dc_x1 + 0.999 * self.dc_y1;
        self.dc_x1 = sum;
        self.dc_y1 = out;
        let sum = out;

        // Control-rate pitch-bloom retune of the low partials.
        self.ctl += 1;
        if self.ctl >= CTL {
            self.ctl = 0;
            self.update_bloom();
        }

        if self.exc_remaining == 0 {
            self.exc_vel *= 0.999;
            self.exc_amp = 0.0;
        }
        sum
    }

    fn update_bloom(&mut self) {
        if self.bloom_cur > 0.4 {
            self.bloom_cur *= self.bloom_decay;
            let mult = mathf::powf(2.0, self.bloom_cur / 1200.0);
            for m in &mut self.modes {
                if m.bloom {
                    m.res.set(m.base_freq * mult, m.t60, m.gain, self.fs);
                }
            }
        } else if self.bloom_cur != 0.0 {
            // Settle exactly back to true pitch.
            self.bloom_cur = 0.0;
            for m in &mut self.modes {
                if m.bloom {
                    m.res.set(m.base_freq, m.t60, m.gain, self.fs);
                }
            }
        }
    }
}
