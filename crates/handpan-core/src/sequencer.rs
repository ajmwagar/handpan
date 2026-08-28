//! Generative sequencer — the "brain". Target-neutral and voice-free: it walks
//! a scale on a clock and emits note events. It drives the built-in
//! [`crate::HandpanInstrument`], and on its own it powers a standalone brain
//! module that outputs Gate + 1V/oct CV to play any voice (Unix-style split).

use crate::rng::Rng;

/// How the sequencer chooses the next field on each clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayMode {
    /// No auto-play; the host strikes fields directly.
    Manual,
    Up,
    Down,
    UpDown,
    /// Uniformly random field each clock.
    Random,
    /// Random walk — small steps around the scale, the ambient wander.
    Wander,
    /// Euclidean rhythm; pitch ascends the scale on each hit.
    Euclid,
}

/// What a clock produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// No note this clock (a rest, or `Manual`).
    Rest,
    /// Strike this field index at this velocity.
    Strike { field: usize, velocity: f32 },
}

/// A voice-independent generative sequencer over `fields` scale positions.
#[derive(Clone)]
pub struct Sequencer {
    fields: usize,
    mode: PlayMode,
    pos: usize,
    dir: i32,
    rng: Rng,
    e_len: u8,
    e_fill: u8,
    e_rot: u8,
    e_step: u8,
    humanize: f32,
    rest_chance: f32,
}

impl Sequencer {
    pub fn new(fields: usize, seed: u32) -> Self {
        Self {
            fields: fields.max(1),
            mode: PlayMode::Manual,
            pos: 0,
            dir: 1,
            rng: Rng::new(seed),
            e_len: 8,
            e_fill: 5,
            e_rot: 0,
            e_step: 0,
            humanize: 0.18,
            rest_chance: 0.0,
        }
    }

    pub fn set_fields(&mut self, fields: usize) {
        self.fields = fields.max(1);
        self.pos = self.pos.min(self.fields - 1);
        self.e_step = 0;
    }

    pub fn set_mode(&mut self, mode: PlayMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> PlayMode {
        self.mode
    }

    pub fn set_euclid(&mut self, len: u8, fill: u8, rot: u8) {
        self.e_len = len.max(1);
        self.e_fill = fill.min(self.e_len);
        self.e_rot = rot;
    }

    pub fn set_feel(&mut self, humanize: f32, rest_chance: f32) {
        self.humanize = humanize.clamp(0.0, 1.0);
        self.rest_chance = rest_chance.clamp(0.0, 1.0);
    }

    /// The field the sequencer currently points at.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Advance one clock and return the resulting event. `base_vel` is the
    /// level before humanization.
    pub fn clock(&mut self, base_vel: f32) -> Step {
        if self.mode == PlayMode::Manual || self.fields == 0 {
            return Step::Rest;
        }
        // Euclidean gates the rhythm; other modes fire every clock.
        if self.mode == PlayMode::Euclid {
            let hit = euclid_hit(self.e_step, self.e_len, self.e_fill, self.e_rot);
            self.e_step = (self.e_step + 1) % self.e_len;
            if !hit {
                return Step::Rest;
            }
        }
        if self.rest_chance > 0.0 && self.rng.next_unit() < self.rest_chance {
            self.advance();
            return Step::Rest;
        }
        self.advance();
        let v = (base_vel * (1.0 + self.humanize * self.rng.next_bipolar())).clamp(0.05, 1.0);
        Step::Strike { field: self.pos, velocity: v }
    }

    fn advance(&mut self) {
        let n = self.fields;
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
                let r = self.rng.next_unit();
                let step: i32 = if r < 0.12 {
                    (self.rng.next_u32() % 5) as i32 - 2 // occasional leap
                } else if r < 0.56 {
                    1
                } else {
                    -1
                };
                self.pos = (self.pos as i32 + step).clamp(0, n as i32 - 1) as usize;
            }
        }
    }
}

/// Even-distribution Euclidean hit test (Bresenham "bucket" method): an onset
/// falls on `step` when `floor(step·fill/len)` increments.
pub(crate) fn euclid_hit(step: u8, len: u8, fill: u8, rot: u8) -> bool {
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
    fn wander_stays_in_range() {
        let mut seq = Sequencer::new(9, 0x1234);
        seq.set_mode(PlayMode::Wander);
        for _ in 0..10_000 {
            if let Step::Strike { field, .. } = seq.clock(0.8) {
                assert!(field < 9);
            }
        }
    }

    #[test]
    fn euclid_emits_five_of_eight() {
        let mut seq = Sequencer::new(9, 1);
        seq.set_mode(PlayMode::Euclid);
        seq.set_euclid(8, 5, 0);
        seq.set_feel(0.0, 0.0);
        let hits = (0..8).filter(|_| matches!(seq.clock(0.8), Step::Strike { .. })).count();
        assert_eq!(hits, 5);
    }
}
