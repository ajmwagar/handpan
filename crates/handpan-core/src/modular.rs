//! Modular-instrument layer. Composes the polyphonic [`Handpan`] voice with a
//! [`Sequencer`] so one type covers both module roles:
//!
//! * **Voice module** — strike a field by index / normalized CV / quantized
//!   1V/oct; stereo out. Set [`PlayMode::Manual`] and drive it externally.
//! * **Self-playing** — pick a play mode and clock it; the internal sequencer
//!   walks the scale (the normalled-generative convenience).
//!
//! For the standalone **brain module** (Gate + V/oct CV out, no audio), use
//! [`Sequencer`] directly plus [`voct_of_field`]. Target-neutral (`no_std`).

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::sequencer::{Sequencer, Step};
use crate::{Artic, Build, Handpan, PlayMode, Scale, Size};

/// A playable handpan/tongue-drum instrument for a modular context.
pub struct HandpanInstrument {
    hp: Handpan,
    freqs: Vec<f32>,
    fs: f32,
    seq: Sequencer,
}

impl HandpanInstrument {
    /// Build from a named scale and preset.
    pub fn new(fs: f32, scale: &Scale, build: Build, size: Size) -> Self {
        let freqs = scale.freqs();
        let seq = Sequencer::new(freqs.len(), 0x51ED_C0DE);
        Self {
            hp: Handpan::from_preset(fs, scale, build, size),
            freqs,
            fs,
            seq,
        }
    }

    /// Swap scale/preset, rebuilding the voice.
    pub fn reconfigure(&mut self, scale: &Scale, build: Build, size: Size) {
        self.hp = Handpan::from_preset(self.fs, scale, build, size);
        self.freqs = scale.freqs();
        self.seq.set_fields(self.freqs.len());
    }

    pub fn field_count(&self) -> usize {
        self.freqs.len()
    }

    pub fn set_mode(&mut self, mode: PlayMode) {
        self.seq.set_mode(mode);
    }
    pub fn set_euclid(&mut self, len: u8, fill: u8, rot: u8) {
        self.seq.set_euclid(len, fill, rot);
    }
    pub fn set_feel(&mut self, humanize: f32, rest_chance: f32) {
        self.seq.set_feel(humanize, rest_chance);
    }
    pub fn set_air(&mut self, wet: f32) {
        self.hp.set_air(wet);
    }
    pub fn set_coupling(&mut self, amount: f32) {
        self.hp.set_coupling(amount);
    }

    /// Strike a specific field index.
    pub fn strike_field(&mut self, index: usize, velocity: f32) {
        self.hp.strike(index, velocity);
    }

    /// Strike the field selected by a normalized control voltage in [0, 1].
    pub fn strike_cv(&mut self, norm: f32, velocity: f32) {
        let n = self.freqs.len();
        if n == 0 {
            return;
        }
        let idx = ((norm.clamp(0.0, 1.0) * n as f32) as usize).min(n - 1);
        self.hp.strike(idx, velocity);
    }

    /// Strike the field nearest a 1V/oct pitch (`volts` relative to the ding),
    /// quantizing to the scale.
    pub fn strike_voct(&mut self, volts: f32, velocity: f32) {
        if let Some(idx) = nearest_field(&self.freqs, volts) {
            self.hp.strike(idx, velocity);
        }
    }

    /// Quantized strike with a playing articulation and position — the module's
    /// main play input (V/Oct + Strike + Artic/Position CV).
    pub fn strike_voct_artic(&mut self, volts: f32, velocity: f32, artic: Artic, position: f32) {
        if let Some(idx) = nearest_field(&self.freqs, volts) {
            self.hp.strike_artic(idx, velocity, artic, position);
        }
    }

    /// Strike the gu (bottom-port bass hit).
    pub fn strike_gu(&mut self, velocity: f32) {
        self.hp.strike_gu(velocity);
    }

    /// Continuous palm-mute pressure (0 = open, 1 = muted).
    pub fn set_damp(&mut self, amount: f32) {
        self.hp.set_damp(amount);
    }

    /// Shared-shell interaction (cross-note intermodulation).
    pub fn set_shell(&mut self, amount: f32) {
        self.hp.set_shell_nonlin(amount);
    }

    /// 1V/oct value that selects `field` (for a Pitch-CV thru output).
    pub fn field_voct(&self, field: usize) -> f32 {
        voct_of_field(&self.freqs, field)
    }

    /// Advance the internal sequencer one clock and strike (unless resting or
    /// in `Manual`). `velocity` is the base level before humanization.
    pub fn clock(&mut self, velocity: f32) {
        if let Step::Strike { field, velocity: v } = self.seq.clock(velocity) {
            self.hp.strike(field, v);
        }
    }

    /// Rest a hand on all fields (a full mute).
    pub fn damp_all(&mut self) {
        self.hp.damp_all();
    }

    /// One stereo output sample.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        self.hp.process()
    }
}

/// Index of the scale field nearest a 1V/oct pitch relative to `freqs[0]`.
fn nearest_field(freqs: &[f32], volts: f32) -> Option<usize> {
    if freqs.is_empty() {
        return None;
    }
    let target = freqs[0] * crate::mathf::powf(2.0, volts);
    let mut best = 0usize;
    let mut bd = f32::INFINITY;
    for (i, &f) in freqs.iter().enumerate() {
        let d = (f - target).abs();
        if d < bd {
            bd = d;
            best = i;
        }
    }
    Some(best)
}

/// The 1V/oct value (volts, relative to the ding) that selects `field` in
/// `freqs` — the brain module's CV output for a chosen field.
pub fn voct_of_field(freqs: &[f32], field: usize) -> f32 {
    if freqs.is_empty() || field >= freqs.len() {
        return 0.0;
    }
    crate::mathf::log2(freqs[field] / freqs[0])
}
