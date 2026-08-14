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

/// C# Kurd 9 — C# natural minor, one of the most popular pro tunings.
/// C#3 ding with G#3 A3 B3 C#4 D#4 E4 F#4 G#4.
pub const CSH_KURD_9_MIDI: &[f32] = &[49.0, 56.0, 57.0, 59.0, 61.0, 63.0, 64.0, 66.0, 68.0];

/// D Hijaz 9 — Middle-Eastern colour (1 ♭2 3 4 5 ♭6 ♭7). D3 ding with
/// A3 B♭3 C4 D4 E♭4 F♯4 G4 A4.
pub const D_HIJAZ_9_MIDI: &[f32] = &[50.0, 57.0, 58.0, 60.0, 62.0, 63.0, 66.0, 67.0, 69.0];

/// D Minor Pentatonic 9 — D3 ding with A3 C4 D4 F4 G4 A4 C5 D5. No semitones,
/// endlessly consonant.
pub const D_MINOR_PENTATONIC_9_MIDI: &[f32] =
    &[50.0, 57.0, 60.0, 62.0, 65.0, 67.0, 69.0, 72.0, 74.0];

/// D Major 9 — a bright major layout. D3 ding with A3 D4 E4 F♯4 G4 A4 B4 C♯5.
pub const D_MAJOR_9_MIDI: &[f32] = &[50.0, 57.0, 62.0, 64.0, 66.0, 67.0, 69.0, 71.0, 73.0];

/// D Hijaz Kar 9 — double-harmonic / Byzantine (1 ♭2 3 4 5 ♭6 7): even more
/// dramatic than Hijaz, with two augmented-second steps. D3 ding with
/// A3 B♭3 C♯4 D4 E♭4 F♯4 G4 A4.
pub const D_HIJAZ_KAR_9_MIDI: &[f32] = &[50.0, 57.0, 58.0, 61.0, 62.0, 63.0, 66.0, 67.0, 69.0];

/// D Insen 9 — Japanese pentatonic (1 ♭2 4 5 ♭7): sparse and haunting.
/// D3 ding with A3 C4 D4 E♭4 G4 A4 C5 D5.
pub const D_INSEN_9_MIDI: &[f32] = &[50.0, 57.0, 60.0, 62.0, 63.0, 67.0, 69.0, 72.0, 74.0];

/// A named or custom handpan tuning.
#[derive(Clone)]
pub enum Scale {
    DKurd9,
    DCelticMinor9,
    CshKurd9,
    DHijaz9,
    DMinorPentatonic9,
    DMajor9,
    DHijazKar9,
    DInsen9,
    /// Arbitrary tuning as MIDI note numbers, ding first.
    Custom(Vec<f32>),
}

impl Scale {
    /// MIDI note numbers for this tuning, ding first.
    pub fn midi(&self) -> Vec<f32> {
        match self {
            Scale::DKurd9 => D_KURD_9_MIDI.to_owned(),
            Scale::DCelticMinor9 => D_CELTIC_MINOR_9_MIDI.to_owned(),
            Scale::CshKurd9 => CSH_KURD_9_MIDI.to_owned(),
            Scale::DHijaz9 => D_HIJAZ_9_MIDI.to_owned(),
            Scale::DMinorPentatonic9 => D_MINOR_PENTATONIC_9_MIDI.to_owned(),
            Scale::DMajor9 => D_MAJOR_9_MIDI.to_owned(),
            Scale::DHijazKar9 => D_HIJAZ_KAR_9_MIDI.to_owned(),
            Scale::DInsen9 => D_INSEN_9_MIDI.to_owned(),
            Scale::Custom(m) => m.clone(),
        }
    }

    /// All named tunings (excludes `Custom`), for enumerating in a UI.
    pub fn all_named() -> [Scale; 8] {
        [
            Scale::DKurd9,
            Scale::DCelticMinor9,
            Scale::CshKurd9,
            Scale::DHijaz9,
            Scale::DMinorPentatonic9,
            Scale::DMajor9,
            Scale::DHijazKar9,
            Scale::DInsen9,
        ]
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Scale::DKurd9 => "D Kurd 9",
            Scale::DCelticMinor9 => "D Celtic Minor 9",
            Scale::CshKurd9 => "C# Kurd 9",
            Scale::DHijaz9 => "D Hijaz 9",
            Scale::DMinorPentatonic9 => "D Minor Pentatonic 9",
            Scale::DMajor9 => "D Major 9",
            Scale::DHijazKar9 => "D Hijaz Kar 9",
            Scale::DInsen9 => "D Insen 9",
            Scale::Custom(_) => "Custom",
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
