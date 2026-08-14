//! Modular-instrument layer: wraps the polyphonic [`Handpan`] voice into the
//! thing a Eurorack module actually is — an instrument you *play* (strike a
//! field by CV/gate) or that *plays itself* (an internal generative sequencer
//! walks the scale on a clock). Target-neutral (`no_std` + `alloc`): the Daisy
//! firmware and a host test drive the exact same code.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::rng::Rng;
use crate::{Build, Handpan, Scale, Size};

/// How the internal sequencer chooses the next field on each clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayMode {
    /// No auto-play; strike fields externally via CV/gate.
    Manual,
    Up,
    Down,
    UpDown,
    /// Uniformly random field each clock.
    Random,
    /// Random walk — small steps around the scale, the classic ambient wander.
    Wander,
    /// Euclidean rhythm; pitch ascends the scale on each hit.
    Euclid,
}

/// A playable handpan/tongue-drum instrument for a modular context.
pub struct HandpanInstrument {
    hp: Handpan,
    freqs: Vec<f32>,
    fs: f32,
    mode: PlayMode,
    pos: usize,
    dir: i32,
    rng: Rng,
    // Euclidean state.
    e_len: u8,
    e_fill: u8,
    e_rot: u8,
    e_step: u8,
    // Feel.
    humanize: f32,
    rest_chance: f32,
}

impl HandpanInstrument {
    /// Build from a named scale and preset.
    pub fn new(fs: f32, scale: &Scale, build: Build, size: Size) -> Self {
        Self {
            hp: Handpan::from_preset(fs, scale, build, size),
            freqs: scale.freqs(),
            fs,
            mode: PlayMode::Manual,
            pos: 0,
            dir: 1,
            rng: Rng::new(0x51ED_C0DE),
            e_len: 8,
            e_fill: 5,
            e_rot: 0,
            e_step: 0,
            humanize: 0.18,
            rest_chance: 0.0,
        }
    }

    /// Swap scale/preset, rebuilding the voice.
    pub fn reconfigure(&mut self, scale: &Scale, build: Build, size: Size) {
        self.hp = Handpan::from_preset(self.fs, scale, build, size);
        self.freqs = scale.freqs();
        self.pos = self.pos.min(self.freqs.len().saturating_sub(1));
        self.e_step = 0;
    }

    pub fn field_count(&self) -> usize {
        self.freqs.len()
    }

    pub fn set_mode(&mut self, mode: PlayMode) {
        self.mode = mode;
    }

    /// Euclidean rhythm parameters (length, filled steps, rotation).
    pub fn set_euclid(&mut self, len: u8, fill: u8, rot: u8) {
        self.e_len = len.max(1);
        self.e_fill = fill.min(self.e_len);
        self.e_rot = rot;
    }

    /// Velocity jitter (0..1) and probability of resting on a clock (0..1).
    pub fn set_feel(&mut self, humanize: f32, rest_chance: f32) {
        self.humanize = humanize.clamp(0.0, 1.0);
        self.rest_chance = rest_chance.clamp(0.0, 1.0);
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

    /// Strike the field nearest a 1V/oct pitch, where `volts` is relative to
    /// the ding (0 V = the ding's pitch). Quantizes to the scale.
    pub fn strike_voct(&mut self, volts: f32, velocity: f32) {
        if self.freqs.is_empty() {
            return;
        }
        let target = self.freqs[0] * powf2(volts);
        let mut best = 0usize;
        let mut bd = f32::INFINITY;
        for (i, &f) in self.freqs.iter().enumerate() {
            let d = (f - target).abs();
            if d < bd {
                bd = d;
                best = i;
            }
        }
        self.hp.strike(best, velocity);
    }

    /// Advance the internal sequencer one clock and strike (unless resting or
    /// in `Manual`). `velocity` is the base level before humanization.
    pub fn clock(&mut self, velocity: f32) {
        if self.mode == PlayMode::Manual {
            return;
        }
        let n = self.freqs.len();
        if n == 0 {
            return;
        }

        // Euclidean gates the rhythm; other modes strike every clock.
        let play = if self.mode == PlayMode::Euclid {
            let step = self.e_step;
            self.e_step = (self.e_step + 1) % self.e_len;
            euclid_hit(step, self.e_len, self.e_fill, self.e_rot)
        } else {
            true
        };
        if !play {
            return;
        }
        if self.rest_chance > 0.0 && self.rng.next_unit() < self.rest_chance {
            self.advance(n); // keep the melodic contour moving through the rest
            return;
        }

        self.advance(n);
        let v = (velocity * (1.0 + self.humanize * self.rng.next_bipolar())).clamp(0.05, 1.0);
        self.hp.strike(self.pos, v);
    }

    fn advance(&mut self, n: usize) {
        match self.mode {
            PlayMode::Manual => {}
            PlayMode::Up | PlayMode::Euclid => self.pos = (self.pos + 1) % n,
            PlayMode::Down => self.pos = (self.pos + n - 1) % n,
            PlayMode::UpDown => {
                if n > 1 {
                    let mut p = self.pos as i32 + self.dir;
                    if p >= n as i32 {
                        p = n as i32 - 2;
                        self.dir = -1;
                    } else if p < 0 {
                        p = 1;
                        self.dir = 1;
                    }
                    self.pos = p.clamp(0, n as i32 - 1) as usize;
                }
            }
            PlayMode::Random => self.pos = (self.rng.next_u32() as usize) % n,
            PlayMode::Wander => {
                // Mostly ±1–2 steps, with the odd larger leap.
                let r = self.rng.next_unit();
                let step: i32 = if r < 0.12 {
                    (self.rng.next_u32() % 5) as i32 - 2 // occasional leap
                } else if r < 0.56 {
                    1
                } else {
                    -1
                };
                let p = (self.pos as i32 + step).clamp(0, n as i32 - 1);
                self.pos = p as usize;
            }
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

#[inline]
fn powf2(volts: f32) -> f32 {
    crate::mathf::powf(2.0, volts)
}

/// Even-distribution Euclidean hit test (Bresenham "bucket" method): an onset
/// falls on `step` when `floor(step·fill/len)` increments.
fn euclid_hit(step: u8, len: u8, fill: u8, rot: u8) -> bool {
    if fill == 0 || len == 0 {
        return false;
    }
    let s = ((step as u32 + rot as u32) % len as u32) * fill as u32;
    (s % len as u32) < fill as u32
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    fn hit_count(len: u8, fill: u8) -> u8 {
        (0..len).filter(|&s| euclid_hit(s, len, fill, 0)).count() as u8
    }

    #[test]
    fn euclid_has_requested_onsets() {
        assert_eq!(hit_count(8, 5), 5);
        assert_eq!(hit_count(8, 3), 3);
        assert_eq!(hit_count(16, 7), 7);
        assert_eq!(hit_count(4, 0), 0);
    }

    #[test]
    fn wander_stays_in_range_and_sounds() {
        let mut inst =
            HandpanInstrument::new(48_000.0, &Scale::DHijaz9, Build::Handpan, Size::Large);
        inst.set_mode(PlayMode::Wander);
        let mut peak = 0.0f32;
        for i in 0..48_000 * 3 {
            if i % 9_000 == 0 {
                inst.clock(0.8);
                assert!(inst.pos < inst.field_count());
            }
            let (l, r) = inst.process();
            assert!(l.is_finite() && r.is_finite());
            peak = peak.max(l.abs().max(r.abs()));
        }
        assert!(peak > 0.01, "instrument produced no sound");
    }
}
