//! # handpan-core
//!
//! Modal physical-modeling synthesis for handpans and steel tongue drums.
//!
//! The voice is a bank of tuned modal resonators per tone field — no samples,
//! no circuit/WDF machinery — with amplitude-dependent pitch bloom, a metallic
//! attack transient, sympathetic coupling, and a shared shell resonance. The
//! whole instrument's character (dimpled handpan vs. cut tongue drum, and
//! size) is captured in a [`VoiceProfile`]. It's dependency-free and
//! `no_std`-ready, so the same DSP compiles to both a plugin and firmware.
//!
//! ```
//! use handpan_core::{Handpan, Build, Size, scale::Scale};
//! let mut hp = Handpan::from_preset(48_000.0, &Scale::DKurd9, Build::Handpan, Size::Standard);
//! hp.strike(0, 0.9);                 // hit the ding
//! let (l, r) = hp.process();         // one stereo sample
//! # let _ = (l, r);
//! ```
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

mod air;
mod mathf;
mod modular;
mod note;
mod preset;
mod resonator;
mod rng;
mod sequencer;

pub mod scale;

pub use modular::{voct_of_field, HandpanInstrument};
pub use note::{ModeSpec, HANDPAN_TIMBRE, TONGUE_DRUM_TIMBRE};
pub use preset::{Build, Size, VoiceProfile};
pub use scale::Scale;
pub use sequencer::{PlayMode, Sequencer, Step};

use air::Air;
use note::NoteVoice;
use resonator::Resonator;

/// A polyphonic handpan / tongue drum: a ring of tone fields with sympathetic
/// coupling, a shared shell resonance, room ambience, and per-note stereo
/// placement.
pub struct Handpan {
    fs: f32,
    notes: Vec<NoteVoice>,
    pan: Vec<(f32, f32)>,
    coupling: f32,
    // Harmonic relatedness weights: octave/fifth neighbours couple harder.
    couple_w: Vec<Vec<f32>>,
    body: Resonator,
    body_amount: f32,
    body_exc: f32,
    // Shared-shell nonlinearity → cross-note intermodulation.
    shell_nonlin: f32,
    shell_x1: f32,
    shell_y1: f32,
    shell_lp: f32,   // band-limit the drive so only low modes intermodulate
    shell_lp_a: f32,
    shell_out: f32,  // smooth the generated combination tones
    shell_out_a: f32,
    air: Air,
    air_wet: f32,
}

impl Handpan {
    /// Build with the default handpan profile from a list of fundamentals.
    pub fn new(fs: f32, freqs: &[f32]) -> Self {
        Self::with_profile(fs, freqs, &VoiceProfile::default())
    }

    /// Build a named instrument (`build` × `size`) tuned to a [`Scale`].
    pub fn from_preset(fs: f32, scale: &Scale, build: Build, size: Size) -> Self {
        Self::with_profile(fs, &scale.freqs(), &VoiceProfile::preset(build, size))
    }

    /// Build from an explicit [`VoiceProfile`] and a set of fundamentals.
    pub fn with_profile(fs: f32, freqs: &[f32], profile: &VoiceProfile) -> Self {
        let n = freqs.len();
        let mut notes = Vec::with_capacity(n);
        let mut pan = Vec::with_capacity(n);
        for (i, &f) in freqs.iter().enumerate() {
            // Fundamental decay; the octave/fifth extend well past this via
            // their timbre multipliers. Lower notes ring longest.
            let t60_base = (5.5 * (150.0 / f)).clamp(1.3, 7.0);

            let mut r = rng::Rng::new(profile.seed ^ (0x9E37_79B9u32.wrapping_mul(i as u32 + 1)));
            let cents = r.next_bipolar() * profile.detune_cents;
            let tune_mult = mathf::powf(2.0, cents / 1200.0);

            notes.push(NoteVoice::new(f, t60_base, profile, fs, r.next_u32(), tune_mult));

            let pos = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 };
            let x = if i == 0 { 0.0 } else { (pos * 2.0 - 1.0) * 0.6 };
            let theta = (x + 1.0) * 0.25 * core::f32::consts::PI; // constant-power
            pan.push((mathf::cos(theta), mathf::sin(theta)));
        }

        // Precompute harmonic-relatedness coupling weights between fields.
        let couple_w = (0..n)
            .map(|i| (0..n).map(|j| harmonic_relatedness(freqs[i], freqs[j])).collect())
            .collect();

        let mut body = Resonator::default();
        body.set(profile.body_freq, profile.body_decay, 1.0, fs);

        Self {
            fs,
            notes,
            pan,
            coupling: profile.coupling,
            couple_w,
            body,
            body_amount: profile.body,
            body_exc: 0.0,
            shell_nonlin: profile.shell_nonlin,
            shell_x1: 0.0,
            shell_y1: 0.0,
            shell_lp: 0.0,
            // ~900 Hz: only the strong low modes drive the nonlinearity.
            shell_lp_a: 1.0 - mathf::exp(-core::f32::consts::TAU * 900.0 / fs),
            shell_out: 0.0,
            // ~2.5 kHz: tame the top of the generated combination tones.
            shell_out_a: 1.0 - mathf::exp(-core::f32::consts::TAU * 2500.0 / fs),
            air: Air::new(fs),
            air_wet: profile.air,
        }
    }

    /// Number of tone fields.
    pub fn note_count(&self) -> usize {
        self.notes.len()
    }

    /// Sample rate in Hz.
    pub fn sample_rate(&self) -> f32 {
        self.fs
    }

    /// Set the sympathetic-coupling amount (0 = dry).
    pub fn set_coupling(&mut self, amount: f32) {
        self.coupling = amount.clamp(0.0, 0.5);
    }

    /// Set the room-ambience ("Air") wet level (0 = dry).
    pub fn set_air(&mut self, wet: f32) {
        self.air_wet = wet.clamp(0.0, 1.0);
    }

    /// Set the shared-shell nonlinearity (cross-note intermodulation). 0 =
    /// notes sum independently; higher = they interact through one surface.
    pub fn set_shell_nonlin(&mut self, amount: f32) {
        self.shell_nonlin = amount.clamp(0.0, 0.4);
    }

    /// Rest a hand on tone field `index` — a fast, natural mute.
    pub fn damp(&mut self, index: usize) {
        if let Some(n) = self.notes.get_mut(index) {
            n.damp();
        }
    }

    /// Mute every tone field.
    pub fn damp_all(&mut self) {
        for n in &mut self.notes {
            n.damp();
        }
    }

    /// Strike tone field `index` with `velocity` in [0, 1]. Neighbouring fields
    /// receive a sympathetic excitation weighted by harmonic relatedness (the
    /// halo), and the shared shell resonance is driven by the hit.
    pub fn strike(&mut self, index: usize, velocity: f32) {
        if index >= self.notes.len() {
            return;
        }
        for j in 0..self.notes.len() {
            if j == index {
                self.notes[j].strike(velocity);
            } else {
                let bleed = velocity * self.coupling * self.couple_w[index][j];
                if bleed > 1e-4 {
                    self.notes[j].strike(bleed);
                }
            }
        }
        self.body_exc += velocity;
    }

    /// Advance one sample, returning interleaved `(left, right)`.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let mut l = 0.0;
        let mut r = 0.0;
        let mut mono = 0.0;
        for (note, &(pl, pr)) in self.notes.iter_mut().zip(self.pan.iter()) {
            let s = note.process();
            l += s * pl;
            r += s * pr;
            mono += s;
        }

        // Shared-shell nonlinearity: the whole membrane is one nonlinear
        // surface, so every ringing field intermodulates through it. Squaring
        // the summed displacement yields sum/difference (combination) tones
        // between simultaneous notes. Feed-forward + DC-blocked → bounded.
        if self.shell_nonlin > 1e-6 {
            // Band-limit the drive: the shell's nonlinear coupling is dominated
            // by the large low-mode displacement, so squaring only the low end
            // yields warm difference tones instead of broadband HF hash (which
            // also kept it near Nyquist and aliasing).
            self.shell_lp += self.shell_lp_a * (mono - self.shell_lp);
            let m = self.shell_lp.clamp(-4.0, 4.0);
            let raw = self.shell_nonlin * m * m;
            // DC-block, then soft-saturate so hard hits fold gently.
            let hp = raw - self.shell_x1 + 0.999 * self.shell_y1;
            self.shell_x1 = raw;
            self.shell_y1 = hp;
            const CAP: f32 = 0.35;
            let sat = CAP * mathf::tanh(hp / CAP);
            // Smooth the top of the generated tones.
            self.shell_out += self.shell_out_a * (sat - self.shell_out);
            l += self.shell_out * 0.5;
            r += self.shell_out * 0.5;
        }

        let b = self.body.process(self.body_exc) * self.body_amount;
        self.body_exc = 0.0;
        let (mut dl, mut dr) = (l + b, r + b);

        if self.air_wet > 1e-4 {
            let (wl, wr) = self.air.process(dl, dr);
            dl += wl * self.air_wet;
            dr += wr * self.air_wet;
        }
        (dl, dr)
    }
}

/// How strongly two fundamentals couple sympathetically: 1.0 for unison/
/// octave/fifth relationships, tapering to a small floor for unrelated notes.
fn harmonic_relatedness(a: f32, b: f32) -> f32 {
    let ratio = if a > b { a / b } else { b / a };
    // Distance (in semitones) to the nearest strong interval.
    const STRONG: [f32; 5] = [1.0, 1.5, 2.0, 3.0, 4.0]; // unison, P5, 8ve, 8ve+5, 15ma
    let mut best = f32::INFINITY;
    for s in STRONG {
        let cents = 1200.0 * mathf::log2(ratio / s).abs();
        best = best.min(cents);
    }
    // Within ~50 cents -> full coupling; falls off over ~4 semitones.
    let w = 1.0 - ((best - 50.0).max(0.0) / 400.0);
    w.clamp(0.15, 1.0)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn strike_is_stable_and_decays() {
        let mut hp = Handpan::new(48_000.0, &scale::d_kurd_9());
        hp.strike(0, 1.0);

        let mut peak_early = 0.0f32;
        let mut peak_late = 0.0f32;
        for i in 0..48_000 * 8 {
            let (l, r) = hp.process();
            assert!(l.is_finite() && r.is_finite(), "non-finite output");
            let a = l.abs().max(r.abs());
            if i < 48_000 {
                peak_early = peak_early.max(a);
            } else if i > 48_000 * 7 {
                peak_late = peak_late.max(a);
            }
        }
        assert!(peak_early > 0.01, "no sound produced");
        assert!(peak_late < peak_early, "energy did not decay");
    }

    #[test]
    fn tongue_drum_is_stable() {
        let mut hp = Handpan::from_preset(48_000.0, &Scale::DCelticMinor9, Build::TongueDrum, Size::Small);
        for n in 0..hp.note_count() {
            hp.strike(n, 0.9);
        }
        let mut peak = 0.0f32;
        for _ in 0..48_000 * 4 {
            let (l, r) = hp.process();
            assert!(l.is_finite() && r.is_finite());
            peak = peak.max(l.abs().max(r.abs()));
        }
        assert!(peak > 0.01, "no sound produced");
    }

    #[test]
    fn air_and_full_strike_stay_bounded() {
        // Hit everything hard with ambience on and make sure nothing blows up.
        let mut hp = Handpan::from_preset(48_000.0, &Scale::DKurd9, Build::Handpan, Size::Large);
        for n in 0..hp.note_count() {
            hp.strike(n, 1.0);
        }
        let mut peak = 0.0f32;
        for i in 0..48_000 * 12 {
            let (l, r) = hp.process();
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
            peak = peak.max(l.abs().max(r.abs()));
        }
        assert!(peak < 100.0, "runaway feedback: peak {peak}");
    }

    #[test]
    fn damping_kills_the_ring() {
        let mut hp = Handpan::new(48_000.0, &scale::d_kurd_9());
        hp.strike(0, 1.0);
        for _ in 0..48_000 {
            hp.process(); // let it ring 1s
        }
        let mut before = 0.0f32;
        for _ in 0..2_400 {
            let (l, _) = hp.process();
            before = before.max(l.abs());
        }
        hp.damp_all();
        for _ in 0..24_000 {
            hp.process(); // 0.5s of muting
        }
        let mut after = 0.0f32;
        for _ in 0..2_400 {
            let (l, _) = hp.process();
            after = after.max(l.abs());
        }
        assert!(after < before * 0.1, "mute did not silence ({before} -> {after})");
    }

    #[test]
    fn dyad_intermodulation_is_stable() {
        // Two related notes struck hard together, shell nonlinearity engaged.
        let mut hp = Handpan::from_preset(48_000.0, &Scale::DKurd9, Build::Handpan, Size::Large);
        hp.set_shell_nonlin(0.2);
        hp.strike(0, 1.0); // ding
        hp.strike(4, 1.0); // a fifth-related field, same instant
        let mut peak = 0.0f32;
        for _ in 0..48_000 * 6 {
            let (l, r) = hp.process();
            assert!(l.is_finite() && r.is_finite(), "non-finite output");
            peak = peak.max(l.abs().max(r.abs()));
        }
        assert!(peak > 0.01, "no sound");
        assert!(peak < 50.0, "shell nonlinearity ran away: {peak}");
    }

    #[test]
    fn d_kurd_ding_is_d3() {
        let f = scale::d_kurd_9()[0];
        assert!((f - 146.83).abs() < 0.5, "ding should be ~D3, got {f}");
    }

    #[test]
    fn relatedness_ranks_intervals() {
        let unison = harmonic_relatedness(220.0, 220.0);
        let octave = harmonic_relatedness(220.0, 440.0);
        let fifth = harmonic_relatedness(220.0, 330.0);
        let tritone = harmonic_relatedness(220.0, 311.13);
        assert!(unison >= 0.99 && octave >= 0.99 && fifth >= 0.99);
        assert!(tritone < fifth, "tritone should couple less than a fifth");
    }
}
