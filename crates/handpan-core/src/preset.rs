//! Instrument presets: the character of the voice as a function of **build**
//! (a dimpled/domed handpan vs. a cut steel tongue drum) and **size**.
//!
//! A [`VoiceProfile`] is the full set of timbre + physical-character knobs the
//! engine builds a voice from. Presets are just named profiles; every field is
//! public so a plugin can start from a preset and nudge it.

use crate::note::{ModeSpec, HANDPAN_TIMBRE, TONGUE_DRUM_TIMBRE};

/// How the note fields are formed in the steel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    /// Dimpled/domed tone fields, hammered — the classic handpan: rich,
    /// long-sustaining, harmonic octave + compound fifth, strong shimmer.
    Handpan,
    /// Cut tongues (steel tongue drum / tank drum): purer, more percussive,
    /// beam-like inharmonic overtones, little shimmer or coupling.
    TongueDrum,
}

/// Physical size of the instrument (shell diameter / tongue mass). Larger
/// instruments ring longer, sit lower, and sound warmer; smaller ones are
/// brighter and shorter. For a tongue drum this reads as the tongue size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// Small shell / short tongues — bright, quick decay (e.g. ~43 cm).
    Small,
    /// Standard concert size (e.g. ~53 cm).
    Standard,
    /// Large shell / long tongues — warm, long sustain (e.g. ~60 cm).
    Large,
    /// Bass instrument — deep, very long sustain, dark.
    Bass,
}

/// The complete character of a voice. Build one with [`VoiceProfile::preset`].
#[derive(Clone)]
pub struct VoiceProfile {
    /// Modal partial structure (frequency ratios, gains, decay multipliers).
    pub timbre: &'static [ModeSpec],
    /// Global decay-time multiplier (sustain).
    pub decay_scale: f32,
    /// Upper-partial tilt: >1 brighter, <1 warmer.
    pub brightness: f32,
    /// Excitation low-pass cutoff (Hz): the mallet-vs-fingertip attack colour.
    pub attack_cutoff_hz: f32,
    /// Excitation burst length (ms).
    pub attack_ms: f32,
    /// Pitch-bloom depth in cents: how sharp a full-velocity strike starts
    /// before it settles to true pitch (0 = off).
    pub bloom_cents: f32,
    /// Pitch-bloom settling time (ms).
    pub bloom_ms: f32,
    /// Shared shell / "gu" body-resonance frequency (Hz).
    pub body_freq: f32,
    /// Body-resonance decay time (s).
    pub body_decay: f32,
    /// Body-resonance level.
    pub body: f32,
    /// Geometric-nonlinearity amount: how strongly the fundamental generates
    /// octave/fifth energy on hard strikes (the "distortion process" measured
    /// in handpans/steelpans). 0 = purely linear.
    pub nonlin: f32,
    /// Sympathetic coupling — the halo (0 = dry).
    pub coupling: f32,
    /// Per-note fabrication detune spread (cents).
    pub detune_cents: f32,
    /// Built-in room ambience ("Air") wet level — the space premium demos are
    /// recorded in (0 = dry).
    pub air: f32,
    /// Seed for deterministic per-note noise and detune.
    pub seed: u32,
}

impl VoiceProfile {
    /// The named profile for a `build` at a given `size`.
    pub fn preset(build: Build, size: Size) -> Self {
        let base = match build {
            Build::Handpan => VoiceProfile {
                timbre: HANDPAN_TIMBRE,
                decay_scale: 1.0,
                brightness: 1.0,
                attack_cutoff_hz: 5500.0,
                attack_ms: 4.0,
                bloom_cents: 14.0,
                bloom_ms: 120.0,
                // Center "gu" Helmholtz cavity ≈ 82 Hz (Rossing et al., 2007).
                body_freq: 82.0,
                body_decay: 0.5,
                body: 0.12,
                nonlin: 0.18,
                coupling: 0.07,
                detune_cents: 3.0,
                air: 0.16,
                seed: 0x1234_5678,
            },
            Build::TongueDrum => VoiceProfile {
                timbre: TONGUE_DRUM_TIMBRE,
                decay_scale: 0.7,
                brightness: 1.05,
                attack_cutoff_hz: 4200.0,
                attack_ms: 3.0,
                bloom_cents: 6.0,
                bloom_ms: 70.0,
                body_freq: 95.0,
                body_decay: 0.3,
                body: 0.06,
                nonlin: 0.05,
                coupling: 0.015,
                detune_cents: 1.5,
                air: 0.10,
                seed: 0x1234_5678,
            },
        };
        base.scaled_for(size)
    }

    /// Apply size scaling to a base profile.
    fn scaled_for(mut self, size: Size) -> Self {
        let (decay, body_f, bright, cutoff) = match size {
            Size::Small => (0.80, 1.40, 1.20, 1.30),
            Size::Standard => (1.00, 1.00, 1.00, 1.00),
            Size::Large => (1.30, 0.80, 0.85, 0.82),
            Size::Bass => (1.65, 0.62, 0.72, 0.66),
        };
        self.decay_scale *= decay;
        self.body_freq *= body_f;
        self.brightness *= bright;
        self.attack_cutoff_hz *= cutoff;
        self
    }
}

impl Default for VoiceProfile {
    fn default() -> Self {
        VoiceProfile::preset(Build::Handpan, Size::Standard)
    }
}
