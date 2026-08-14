//! Handpan scales expressed as MIDI note numbers, plus MIDI→frequency.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::mathf;

/// Equal-tempered MIDI note number → frequency in Hz (A4 = 440).
#[inline]
pub fn midi_to_freq(midi: f32) -> f32 {
    440.0 * mathf::powf(2.0, (midi - 69.0) / 12.0)
}

/// D Kurd 9 — the canonical D-minor handpan layout: a D3 "ding" surrounded by
/// A3 Bb3 C4 D4 E4 F4 G4 A4. The most widely played handpan scale.
pub const D_KURD_9_MIDI: &[f32] = &[50.0, 57.0, 58.0, 60.0, 62.0, 64.0, 65.0, 67.0, 69.0];

/// Frequencies (Hz) of the D Kurd 9 scale, ding first.
pub fn d_kurd_9() -> Vec<f32> {
    D_KURD_9_MIDI.iter().map(|&m| midi_to_freq(m)).collect()
}
