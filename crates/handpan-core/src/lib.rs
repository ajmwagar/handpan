//! # handpan-core
//!
//! Modal physical-modeling synthesis for polyphonic handpans.
//!
//! The voice is a bank of tuned modal resonators per tone field — no samples,
//! no circuit/WDF machinery. That makes it cheap, endlessly variable, and
//! fully expressive (velocity bloom, sympathetic ring, shell resonance), and
//! it compiles to both a plugin (host, libstd) and modular firmware
//! (`no_std` + `alloc`).
//!
//! ```
//! use handpan_core::{Handpan, scale};
//! let mut hp = Handpan::new(48_000.0, &scale::d_kurd_9());
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

mod mathf;
mod note;
mod resonator;
mod rng;

pub mod scale;

pub use note::{ModeSpec, HANDPAN_TIMBRE};
pub use scale::Scale;

use note::NoteVoice;
use resonator::Resonator;

/// Tunable voice parameters. [`HandpanConfig::default`] is a good handpan.
#[derive(Clone, Copy)]
pub struct HandpanConfig {
    /// Fraction of a strike bled into every other tone field — the halo.
    pub coupling: f32,
    /// Per-note fabrication detune spread, in cents. Real hammered sets are
    /// never mathematically perfect; a little spread reads as "organic".
    pub detune_cents: f32,
    /// Level of the shared shell / "gu" low-body resonance.
    pub body: f32,
    /// Global decay-time multiplier (sustain).
    pub sustain: f32,
    /// Seed for deterministic per-note noise and detune.
    pub seed: u32,
}

impl Default for HandpanConfig {
    fn default() -> Self {
        Self {
            coupling: 0.06,
            detune_cents: 3.0,
            body: 0.12,
            sustain: 1.0,
            seed: 0x1234_5678,
        }
    }
}

/// A polyphonic handpan instrument: a ring of tone fields with sympathetic
/// coupling, a shared shell resonance, and per-note stereo placement.
pub struct Handpan {
    fs: f32,
    notes: Vec<NoteVoice>,
    pan: Vec<(f32, f32)>,
    coupling: f32,
    // Shared shell/"gu" body resonance, excited by every strike.
    body: Resonator,
    body_amount: f32,
    body_exc: f32,
}

impl Handpan {
    /// Build a handpan at sample rate `fs` (Hz) from a list of fundamental
    /// frequencies, using the default timbre and config.
    pub fn new(fs: f32, freqs: &[f32]) -> Self {
        Self::with_config(fs, freqs, HANDPAN_TIMBRE, HandpanConfig::default())
    }

    /// Build a handpan from a named or custom [`Scale`].
    pub fn from_scale(fs: f32, scale: &Scale, cfg: HandpanConfig) -> Self {
        Self::with_config(fs, &scale.freqs(), HANDPAN_TIMBRE, cfg)
    }

    /// Full control: sample rate, fundamentals, modal timbre, and config.
    pub fn with_config(fs: f32, freqs: &[f32], timbre: &[ModeSpec], cfg: HandpanConfig) -> Self {
        let n = freqs.len();
        let mut notes = Vec::with_capacity(n);
        let mut pan = Vec::with_capacity(n);
        for (i, &f) in freqs.iter().enumerate() {
            // Lower notes sustain longer, as on a real shell.
            let t60_base = (7.0 * (146.83 / f)).clamp(1.2, 9.0) * cfg.sustain;

            // Deterministic per-note detune in [-detune_cents, +detune_cents].
            let mut r = rng::Rng::new(cfg.seed ^ (0x9E37_79B9u32.wrapping_mul(i as u32 + 1)));
            let cents = r.next_bipolar() * cfg.detune_cents;
            let tune_mult = mathf::powf(2.0, cents / 1200.0);

            notes.push(NoteVoice::new(f, t60_base, timbre, fs, r.next_u32(), tune_mult));

            // Ding centered; ring notes spread around the field for width.
            let pos = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 };
            let x = if i == 0 { 0.0 } else { (pos * 2.0 - 1.0) * 0.6 };
            let theta = (x + 1.0) * 0.25 * core::f32::consts::PI; // constant-power
            pan.push((mathf::cos(theta), mathf::sin(theta)));
        }

        // Shell resonance: a low, broad body mode under the whole instrument.
        let mut body = Resonator::default();
        body.set(62.0, 0.5, 1.0, fs);

        Self {
            fs,
            notes,
            pan,
            coupling: cfg.coupling,
            body,
            body_amount: cfg.body,
            body_exc: 0.0,
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

    /// Set the sympathetic-coupling amount (0 = dry, ~0.06 default).
    pub fn set_coupling(&mut self, amount: f32) {
        self.coupling = amount.clamp(0.0, 0.5);
    }

    /// Strike tone field `index` with `velocity` in [0, 1]. Neighboring fields
    /// receive a scaled sympathetic excitation (the halo), and the shared
    /// shell resonance is driven in proportion to the hit.
    pub fn strike(&mut self, index: usize, velocity: f32) {
        if index >= self.notes.len() {
            return;
        }
        let bleed = velocity * self.coupling;
        for (j, note) in self.notes.iter_mut().enumerate() {
            if j == index {
                note.strike(velocity);
            } else {
                note.strike(bleed);
            }
        }
        self.body_exc += velocity;
    }

    /// Advance one sample, returning interleaved `(left, right)`.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let mut mono = 0.0;
        let mut l = 0.0;
        let mut r = 0.0;
        for (note, &(pl, pr)) in self.notes.iter_mut().zip(self.pan.iter()) {
            let s = note.process();
            l += s * pl;
            r += s * pr;
            mono += s;
        }
        // Shell resonance is centered and excited impulsively per strike.
        let b = self.body.process(self.body_exc) * self.body_amount;
        self.body_exc = 0.0;
        let _ = mono;
        (l + b, r + b)
    }
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
    fn d_kurd_ding_is_d3() {
        let f = scale::d_kurd_9()[0];
        assert!((f - 146.83).abs() < 0.5, "ding should be ~D3, got {f}");
    }

    #[test]
    fn scales_have_expected_sizes() {
        assert_eq!(Scale::DKurd9.freqs().len(), 9);
        assert_eq!(Scale::DCelticMinor9.freqs().len(), 9);
        let c = Scale::Custom(vec![60.0, 64.0, 67.0]);
        assert_eq!(c.freqs().len(), 3);
    }
}
