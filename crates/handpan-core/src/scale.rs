//! Handpan scales expressed as MIDI note numbers, plus MIDI→frequency.
//!
//! Layouts are ding-first. Only tunings that are well established in the
//! handpan world are named here; use [`Scale::Custom`] or raw MIDI for
//! anything else.

#[cfg(not(feature = "std"))]
use alloc::{vec::Vec, borrow::ToOwned};

use crate::mathf;

/// Equal-tempered MIDI note number → frequency in Hz (A4 = 440).
#[inline]
pub fn midi_to_freq(midi: f32) -> f32 {
    440.0 * mathf::powf(2.0, (midi - 69.0) / 12.0)
}

/// D Kurd 9 — the canonical D-minor handpan layout: a D3 "ding" surrounded by
/// A3 Bb3 C4 D4 E4 F4 G4 A4. The most widely played handpan scale.
pub const D_KURD_9_MIDI: &[f32] = &[50.0, 57.0, 58.0, 60.0, 62.0, 64.0, 65.0, 67.0, 69.0];

/// D Celtic Minor 9 — D3 ding with A3 C4 D4 E4 F4 G4 A4 C5. Bright, open,
/// the other most-common beginner layout.
pub const D_CELTIC_MINOR_9_MIDI: &[f32] = &[50.0, 57.0, 60.0, 62.0, 64.0, 65.0, 67.0, 69.0, 72.0];

/// A named or custom handpan tuning.
#[derive(Clone)]
pub enum Scale {
    DKurd9,
    DCelticMinor9,
    /// Arbitrary tuning as MIDI note numbers, ding first.
    Custom(Vec<f32>),
}

impl Scale {
    /// MIDI note numbers for this tuning, ding first.
    pub fn midi(&self) -> Vec<f32> {
        match self {
            Scale::DKurd9 => D_KURD_9_MIDI.to_owned(),
            Scale::DCelticMinor9 => D_CELTIC_MINOR_9_MIDI.to_owned(),
            Scale::Custom(m) => m.clone(),
        }
    }

    /// Fundamental frequencies (Hz) for this tuning, ding first.
    pub fn freqs(&self) -> Vec<f32> {
        self.midi().iter().map(|&m| midi_to_freq(m)).collect()
    }
}

/// Frequencies (Hz) of the D Kurd 9 scale, ding first.
pub fn d_kurd_9() -> Vec<f32> {
    Scale::DKurd9.freqs()
}
