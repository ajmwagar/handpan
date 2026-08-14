//! # handpan-core
//!
//! Modal physical-modeling synthesis for polyphonic handpans.
//!
//! The voice is a bank of tuned modal resonators per tone field — no samples,
//! no circuit/WDF machinery. That makes it cheap, endlessly variable, and
//! fully expressive (velocity bloom, sympathetic ring), and it compiles to
//! both a plugin (host, libstd) and modular firmware (`no_std` + `alloc`).
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

use note::NoteVoice;

/// A polyphonic handpan instrument: a ring of tone fields with sympathetic
/// coupling and per-note stereo placement.
pub struct Handpan {
    fs: f32,
    notes: Vec<NoteVoice>,
    pan: Vec<(f32, f32)>,
    /// Fraction of a strike bled into every other tone field (the halo).
    coupling: f32,
}

impl Handpan {
    /// Build a handpan at sample rate `fs` (Hz) from a list of fundamental
    /// frequencies, using the default handpan timbre.
    pub fn new(fs: f32, freqs: &[f32]) -> Self {
        Self::with_timbre(fs, freqs, HANDPAN_TIMBRE)
    }

    /// As [`Handpan::new`], with a custom modal timbre.
    pub fn with_timbre(fs: f32, freqs: &[f32], timbre: &[ModeSpec]) -> Self {
        let n = freqs.len();
        let mut notes = Vec::with_capacity(n);
        let mut pan = Vec::with_capacity(n);
        for (i, &f) in freqs.iter().enumerate() {
            // Lower notes sustain longer, as on a real shell.
            let t60_base = (7.0 * (146.83 / f)).clamp(1.2, 9.0);
            notes.push(NoteVoice::new(f, t60_base, timbre, fs, 0x9E37_79B9 ^ (i as u32 + 1)));

            // Ding centered; ring notes spread around the field for width.
            let pos = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 };
            let x = if i == 0 { 0.0 } else { (pos * 2.0 - 1.0) * 0.6 };
            let theta = (x + 1.0) * 0.25 * core::f32::consts::PI; // constant-power
            pan.push((mathf_cos(theta), mathf_sin(theta)));
        }
        Self { fs, notes, pan, coupling: 0.06 }
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
    /// receive a scaled sympathetic excitation, producing the handpan halo.
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
    }

    /// Advance one sample, returning interleaved `(left, right)`.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let mut l = 0.0;
        let mut r = 0.0;
        for (note, &(pl, pr)) in self.notes.iter_mut().zip(self.pan.iter()) {
            let s = note.process();
            l += s * pl;
            r += s * pr;
        }
        (l, r)
    }
}

// Small wrappers so lib.rs can use the feature-gated math without exposing it.
#[inline]
fn mathf_cos(x: f32) -> f32 {
    mathf::cos(x)
}
#[inline]
fn mathf_sin(x: f32) -> f32 {
    mathf::sin(x)
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
}
